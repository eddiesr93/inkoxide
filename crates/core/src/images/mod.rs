//! Images: registry, header probing, decoding into PDF-ready samples, and
//! the per-render set of unique images (deduplicated by content hash).
//!
//! JPEG is never decoded: its bytes go into the PDF as `DCTDecode` and only
//! the headers are read (§6.6). PNG is decoded behind [`ImageDecoder`] and
//! written as Flate-compressed samples plus an optional alpha `SMask`.

mod base64;
mod cache;
mod decode;
mod probe;
mod svg;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use taffy::{AvailableSpace, Size as TSize};

pub(crate) use decode::{
    DecodedImage, ImageBody, ImageCrateDecoder, ImageDecoder, JpegColor, RawColor,
};
pub(crate) use probe::probe;

use crate::schema::{ImageFit, ImageSource, Node, NodeKind, PageSpec};
use crate::InkoxideError;

/// Largest accepted image, in pixels (decoded-memory guard).
pub(crate) const MAX_PIXELS: u64 = 50_000_000;

/// Points per CSS pixel: images have a 96 dpi intrinsic size.
const PT_PER_PX: f32 = 0.75;

/// Encoded image bytes plus lazily decoded, process-lifetime data.
#[derive(Debug)]
pub(crate) struct ImageData {
    pub bytes: Arc<[u8]>,
    pub hash: u64,
    decoded: OnceLock<Result<Arc<DecodedImage>, String>>,
}

impl ImageData {
    pub fn new(bytes: Arc<[u8]>) -> Self {
        let hash = xxhash_rust::xxh3::xxh3_64(&bytes);
        Self {
            bytes,
            hash,
            decoded: OnceLock::new(),
        }
    }

    /// Decodes once; later renders reuse the result, as do other stores
    /// holding the same bytes (the process-wide cache).
    pub fn decoded(&self, decoder: &dyn ImageDecoder) -> Result<Arc<DecodedImage>, String> {
        self.decoded
            .get_or_init(|| cache::decode(&self.bytes, self.hash, decoder))
            .clone()
    }
}

/// Encoded image bytes (JPEG, PNG, GIF, WebP or SVG) available to a render, keyed by the id
/// that `image` nodes reference in `src`. Cloning is cheap and clones share
/// decoded data.
#[derive(Debug, Clone, Default)]
pub struct ImageStore {
    images: BTreeMap<String, Arc<ImageData>>,
}

impl ImageStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) image `id`. Bytes are checked when a document
    /// uses them: [`crate::validate`] reports undecodable images.
    pub fn insert(&mut self, id: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.images
            .insert(id.into(), Arc::new(ImageData::new(bytes.into())));
    }

    /// Removes image `id`; returns whether it was present.
    pub fn remove(&mut self, id: &str) -> bool {
        self.images.remove(id).is_some()
    }

    /// Whether image `id` is present.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.images.contains_key(id)
    }

    /// Number of images.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether the store holds no image.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    pub(crate) fn get(&self, id: &str) -> Option<&Arc<ImageData>> {
        self.images.get(id)
    }
}

/// Index of a unique image within one render.
pub(crate) type ImageKey = u32;

/// The unique images of one document, decoded, with the image each node
/// shows.
pub(crate) struct ImageSet {
    pub images: Vec<Arc<DecodedImage>>,
    /// Node address → image.
    by_node: HashMap<usize, ImageKey>,
}

fn node_key(n: &Node) -> usize {
    std::ptr::from_ref::<Node>(n) as usize
}

impl ImageSet {
    /// Resolves and decodes every image of the document. Assumes the
    /// document validated (sources exist, headers are sane).
    pub fn load(
        pages: &[PageSpec],
        store: &ImageStore,
        decoder: &dyn ImageDecoder,
    ) -> Result<Self, InkoxideError> {
        let mut found: Vec<(usize, Arc<ImageData>)> = Vec::new();
        let mut stack: Vec<&Node> = Vec::new();
        for p in pages {
            stack.extend(p.children.iter());
            stack.extend(p.header.as_deref());
            stack.extend(p.footer.as_deref());
        }
        while let Some(n) = stack.pop() {
            match &n.kind {
                NodeKind::Image { source, .. } => {
                    let data = match source {
                        ImageSource::Ref(id) => store.get(id).cloned().ok_or_else(|| {
                            InkoxideError::Image(format!("image `{id}` is not registered"))
                        })?,
                        ImageSource::Inline(b64) => {
                            let bytes = base64::decode(b64)
                                .map_err(|e| InkoxideError::Image(format!("inline image: {e}")))?;
                            Arc::new(ImageData::new(bytes.into()))
                        }
                    };
                    found.push((node_key(n), data));
                }
                NodeKind::Box { children } => stack.extend(children.iter()),
                NodeKind::Table(t) => {
                    for r in &t.rows {
                        for c in &r.cells {
                            stack.extend(c.children.iter());
                        }
                    }
                }
                _ => {}
            }
        }

        // Dedupe by content hash; keys follow first appearance in hash
        // order so numbering is deterministic.
        let mut unique: BTreeMap<u64, Arc<ImageData>> = BTreeMap::new();
        for (_, d) in &found {
            unique.entry(d.hash).or_insert_with(|| d.clone());
        }
        let mut key_of: HashMap<u64, ImageKey> = HashMap::new();
        let mut images = Vec::with_capacity(unique.len());
        for (i, (hash, d)) in unique.iter().enumerate() {
            let img = d
                .decoded(decoder)
                .map_err(|e| InkoxideError::Image(e.clone()))?;
            images.push(img);
            key_of.insert(*hash, i as ImageKey);
        }
        let by_node = found
            .into_iter()
            .map(|(n, d)| (n, key_of[&d.hash]))
            .collect();
        Ok(Self { images, by_node })
    }

    pub fn key(&self, node: &Node) -> Option<ImageKey> {
        self.by_node.get(&node_key(node)).copied()
    }

    pub fn get(&self, key: ImageKey) -> &DecodedImage {
        &self.images[key as usize]
    }

    /// Intrinsic size in points (96 dpi, after EXIF orientation).
    pub fn intrinsic(&self, node: &Node) -> Option<(f32, f32)> {
        let img = self.get(self.key(node)?);
        let (w, h) = img.display_size();
        Some((w * PT_PER_PX, h * PT_PER_PX))
    }

    /// Size of an image leaf: an explicit dimension wins and the other
    /// follows the aspect ratio; with neither, the intrinsic size scaled
    /// down to fit a definite available width.
    pub fn measure(
        &self,
        node: &Node,
        known: TSize<Option<f32>>,
        available: TSize<AvailableSpace>,
    ) -> TSize<f32> {
        let Some((iw, ih)) = self.intrinsic(node) else {
            return TSize::ZERO;
        };
        let ratio = if ih > 0.0 { iw / ih } else { 1.0 };
        let (w, h) = match (known.width, known.height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, if ratio > 0.0 { w / ratio } else { 0.0 }),
            (None, Some(h)) => (h * ratio, h),
            (None, None) => match available.width {
                AvailableSpace::Definite(aw) if aw < iw && ratio > 0.0 => {
                    let aw = aw.max(0.0);
                    (aw, aw / ratio)
                }
                _ => (iw, ih),
            },
        };
        TSize {
            width: w,
            height: h,
        }
    }
}

impl ImageSet {
    /// Paints an image node into its content box according to `fit`.
    pub fn paint(
        &self,
        node: &Node,
        geo: &crate::layout::Geometry,
        ctx: &crate::paint::ContentCtx<'_>,
        ops: &mut Vec<crate::paint::Op>,
    ) {
        use crate::paint::{Op, Rect};
        let NodeKind::Image { fit, .. } = &node.kind else {
            return;
        };
        let (Some(key), Some(intrinsic)) = (self.key(node), self.intrinsic(node)) else {
            return;
        };
        let (cx, cy, cw, ch) = geo.content_box();
        if cw <= 0.0 || ch <= 0.0 {
            return;
        }
        let bx = (cx + ctx.dx, cy + ctx.shift + ctx.dy, cw, ch);
        let (rect, clip) = fit_rect(*fit, bx, intrinsic);
        ops.push(Op::Save);
        if clip {
            ops.push(Op::ClipRect(Rect::new(bx.0, bx.1, bx.2, bx.3)));
        }
        ops.push(Op::Image {
            key,
            matrix: image_matrix(self.get(key).orientation, rect),
        });
        ops.push(Op::Restore);
    }
}

/// Placement of an image inside a content box `(x, y, w, h)`: the drawn
/// rectangle and whether it must be clipped to the box.
pub(crate) fn fit_rect(
    fit: ImageFit,
    (x, y, w, h): (f32, f32, f32, f32),
    (iw, ih): (f32, f32),
) -> ((f32, f32, f32, f32), bool) {
    if iw <= 0.0 || ih <= 0.0 {
        return ((x, y, w, h), false);
    }
    let centred = |dw: f32, dh: f32| (x + (w - dw) / 2.0, y + (h - dh) / 2.0, dw, dh);
    match fit {
        ImageFit::Fill => ((x, y, w, h), false),
        ImageFit::Contain => {
            let s = (w / iw).min(h / ih);
            (centred(iw * s, ih * s), false)
        }
        ImageFit::Cover => {
            let s = (w / iw).max(h / ih);
            let r = centred(iw * s, ih * s);
            (r, r.2 > w + 1e-3 || r.3 > h + 1e-3)
        }
        ImageFit::None => {
            let r = centred(iw, ih);
            (r, iw > w + 1e-3 || ih > h + 1e-3)
        }
    }
}

/// Transform mapping the image unit square onto `(x, y, w, h)` in layout
/// space (y down), honouring the EXIF orientation (1–8).
pub(crate) fn image_matrix(orientation: u8, (x, y, w, h): (f32, f32, f32, f32)) -> [f32; 6] {
    // Display coords (s, t) as affine functions of stored-pixel coords
    // (a, b), both normalised with y down: s = sa·a + sb·b + s0, likewise t.
    let (sa, sb, s0, ta, tb, t0) = match orientation {
        2 => (-1.0, 0.0, 1.0, 0.0, 1.0, 0.0),
        3 => (-1.0, 0.0, 1.0, 0.0, -1.0, 1.0),
        4 => (1.0, 0.0, 0.0, 0.0, -1.0, 1.0),
        5 => (0.0, 1.0, 0.0, 1.0, 0.0, 0.0),
        6 => (0.0, -1.0, 1.0, 1.0, 0.0, 0.0),
        7 => (0.0, -1.0, 1.0, -1.0, 0.0, 1.0),
        8 => (0.0, 1.0, 0.0, -1.0, 0.0, 1.0),
        _ => (1.0, 0.0, 0.0, 0.0, 1.0, 0.0),
    };
    // PDF image space (u, v) has v up: a = u, b = 1 − v.
    [
        w * sa,
        h * ta,
        -w * sb,
        -h * tb,
        x + w * (sb + s0),
        y + h * (tb + t0),
    ]
}

/// Decodes a JPEG or PNG embedded in an SVG, through the process-wide
/// cache.
pub(crate) fn decode_embedded(bytes: &[u8]) -> Result<Arc<DecodedImage>, String> {
    ImageData::new(bytes.into()).decoded(&ImageCrateDecoder)
}

/// Decodes inline base64 image data (an optional `data:…;base64,` prefix
/// is accepted).
pub(crate) fn decode_inline(data: &str) -> Result<Vec<u8>, String> {
    base64::decode(data)
}
