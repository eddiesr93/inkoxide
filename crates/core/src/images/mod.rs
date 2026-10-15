//! Images: registry, header probing, decoding into PDF-ready samples, and
//! the per-render set of unique images (deduplicated by content hash).
//!
//! JPEG is not decoded: its bytes go into the PDF as `DCTDecode` and only
//! its markers are read (a CMYK JPEG in PDF/A output is the exception: it
//! is converted to RGB samples). PNG, GIF (its first frame) and WebP are
//! decoded behind [`ImageDecoder`] and written as Flate-compressed samples
//! plus an optional alpha `SMask`. SVG is parsed into a `usvg` tree, with
//! limits for untrusted input (`svg.rs`), and written as a vector form
//! `XObject`.

pub(crate) mod base64;
mod cache;
mod decode;
mod probe;
pub(crate) mod svg;
pub(crate) mod svg_text;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

pub(crate) use decode::{
    DecodedImage, ImageBody, ImageCrateDecoder, ImageDecoder, JpegColor, RawColor,
};
pub(crate) use probe::{probe, ProbeError};

use crate::layout::{AvailableSpace, Size};
use crate::paint::Rect;
use crate::schema::{
    ImageFit, ImageSource, LengthPercent, Node, NodeKind, ObjectPosition, PageSpec,
};
use crate::InkoxideError;

/// Largest accepted image, in pixels (decoded-memory guard).
pub(crate) const MAX_PIXELS: u64 = 50_000_000;

/// Points per CSS pixel: images have a 96 dpi intrinsic size.
const PT_PER_PX: f32 = 0.75;

/// Image content as a map key: equal keys hold equal bytes. Keys are
/// hashed by the content's xxh3 alone, and the bytes are compared on a
/// match, so a hash collision never swaps one image for another.
#[derive(Debug, Clone)]
pub(crate) struct ContentKey<B> {
    hash: u64,
    bytes: B,
}

impl<B: Deref<Target = T>, T: AsRef<[u8]> + ?Sized> ContentKey<B> {
    pub fn new(bytes: B) -> Self {
        let hash = xxhash_rust::xxh3::xxh3_64(bytes.as_ref());
        Self::with_hash(bytes, hash)
    }

    /// A key for `bytes` whose xxh3 is `hash`, already computed.
    pub fn with_hash(bytes: B, hash: u64) -> Self {
        Self { hash, bytes }
    }
}

impl<B: Deref<Target = T>, T: AsRef<[u8]> + ?Sized> PartialEq for ContentKey<B> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.bytes.as_ref() == other.bytes.as_ref()
    }
}

impl<B: Deref<Target = T>, T: AsRef<[u8]> + ?Sized> Eq for ContentKey<B> {}

impl<B> Hash for ContentKey<B> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

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

/// The base64 `data` of an image source (an image node, a background or
/// image run; not to be confused with the schema's `InlineImage`, an image
/// run). It is decoded once, on first
/// use: validation decodes it, and the render that follows reuses the
/// bytes and their decoded image.
#[derive(Debug, Clone)]
pub(crate) struct InlineData {
    b64: String,
    data: OnceLock<Result<Arc<ImageData>, String>>,
}

impl InlineData {
    pub fn new(b64: String) -> Self {
        Self {
            b64,
            data: OnceLock::new(),
        }
    }

    /// The decoded bytes, or why the payload is not base64.
    pub fn data(&self) -> Result<&Arc<ImageData>, &str> {
        self.data
            .get_or_init(|| {
                base64::decode(&self.b64).map(|bytes| Arc::new(ImageData::new(bytes.into())))
            })
            .as_ref()
            .map_err(String::as_str)
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
    /// uses them: [`crate::validate`] reports images that do not decode
    /// (`INVALID_IMAGE`). A JPEG is not decoded, only its structure is
    /// checked, so corrupt compressed data inside a complete JPEG is
    /// embedded as it is.
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

/// How an image fills its box: its `fit`, and its `objectPosition` there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    pub fit: ImageFit,
    pub position: ObjectPosition,
}

impl Placement {
    /// Stretched over the box.
    pub const FILL: Self = Self {
        fit: ImageFit::Fill,
        position: ObjectPosition::CENTER,
    };

    pub fn new(fit: ImageFit, position: ObjectPosition) -> Self {
        Self { fit, position }
    }
}

/// The unique images of one document, decoded, with the image each node
/// shows and the background image each box paints.
pub(crate) struct ImageSet {
    pub images: Vec<Arc<DecodedImage>>,
    /// Image node address → image.
    by_node: HashMap<usize, ImageKey>,
    /// Box address → its background image.
    by_background: HashMap<usize, ImageKey>,
    /// Image run address → its image.
    by_run: HashMap<usize, ImageKey>,
    /// Emoji sequence (without U+FE0F) → its image.
    by_emoji: BTreeMap<String, ImageKey>,
    /// The content hash of each image of the document → its key.
    by_hash: HashMap<u64, ImageKey>,
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
        let source_data = |source: &ImageSource| match source {
            ImageSource::Ref(id) => store
                .get(id)
                .cloned()
                .ok_or_else(|| InkoxideError::Image(format!("image `{id}` is not registered"))),
            ImageSource::Inline(inline) => inline
                .data()
                .map_err(|e| InkoxideError::Image(format!("inline image: {e}")))
                .cloned(),
        };
        let mut found: Vec<(usize, Arc<ImageData>)> = Vec::new();
        let mut backgrounds: Vec<(usize, Arc<ImageData>)> = Vec::new();
        let mut runs: Vec<(usize, Arc<ImageData>)> = Vec::new();
        let mut stack: Vec<&Node> = Vec::new();
        for p in pages {
            stack.extend(p.children.iter());
            stack.extend(p.header.as_deref());
            stack.extend(p.footer.as_deref());
        }
        while let Some(n) = stack.pop() {
            if let Some(bg) = &n.background_image {
                backgrounds.push((node_key(n), source_data(&bg.source)?));
            }
            match &n.kind {
                NodeKind::Image { source, .. } => found.push((node_key(n), source_data(source)?)),
                NodeKind::Text { runs: text } => {
                    for r in text {
                        if let Some(image) = &r.image {
                            runs.push((crate::text::run_key(r), source_data(&image.source)?));
                        }
                    }
                }
                _ => stack.extend(n.children()),
            }
        }

        // Dedupe by content hash; keys follow first appearance in hash
        // order so numbering is deterministic.
        let mut unique: BTreeMap<u64, Arc<ImageData>> = BTreeMap::new();
        for (_, d) in found.iter().chain(&backgrounds).chain(&runs) {
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
        let keys = |found: Vec<(usize, Arc<ImageData>)>| {
            found
                .into_iter()
                .map(|(n, d)| (n, key_of[&d.hash]))
                .collect()
        };
        Ok(Self {
            images,
            by_node: keys(found),
            by_background: keys(backgrounds),
            by_run: keys(runs),
            by_emoji: BTreeMap::new(),
            by_hash: key_of,
        })
    }

    /// Adds the images of the document's `emojiImages` (sequence → image
    /// id), each distinct image once, an image the document shows already
    /// included. Assumes the document validated.
    pub fn load_emoji(
        &mut self,
        emoji: &BTreeMap<String, String>,
        store: &ImageStore,
        decoder: &dyn ImageDecoder,
    ) -> Result<(), InkoxideError> {
        for (sequence, id) in emoji {
            let data = store
                .get(id)
                .ok_or_else(|| InkoxideError::Image(format!("image `{id}` is not registered")))?;
            let key = if let Some(&k) = self.by_hash.get(&data.hash) {
                k
            } else {
                let img = data
                    .decoded(decoder)
                    .map_err(|e| InkoxideError::Image(e.clone()))?;
                let k = self.add(img);
                self.by_hash.insert(data.hash, k);
                k
            };
            self.by_emoji.insert(crate::text::emoji_key(sequence), key);
        }
        Ok(())
    }

    /// Converts the SVGs with text again, each text laid out with this
    /// render's fonts through `shaper` (in image order, so glyph mappings
    /// do not depend on threads). Each becomes this render's own copy, of
    /// the size it has without its text.
    pub fn lay_out_text<S: crate::text::TextShaper>(
        &mut self,
        shaper: &mut S,
    ) -> Result<(), InkoxideError> {
        for image in &mut self.images {
            let ImageBody::Svg {
                tree,
                text: Some(source),
                view_box,
                ..
            } = &image.body
            else {
                continue;
            };
            let (tree, probed) = svg::parse_with_text(source, tree.size(), &mut |t| {
                crate::text::lay_out_svg_text(t, shaper)
            })
            .map_err(InkoxideError::Image)?;
            *image = Arc::new(DecodedImage {
                width: image.width,
                height: image.height,
                orientation: 1,
                body: ImageBody::Svg {
                    tree,
                    view_box: *view_box,
                    text: None,
                    probed: Some(Arc::new(probed)),
                },
            });
        }
        Ok(())
    }

    /// The glyphs the text of this render's SVGs uses, of the fonts
    /// `wanted` picks.
    pub fn svg_text_glyphs(
        &self,
        wanted: impl Fn(crate::fonts::FontKey) -> bool,
    ) -> BTreeMap<crate::fonts::FontKey, BTreeSet<u16>> {
        let mut out: BTreeMap<_, BTreeSet<u16>> = BTreeMap::new();
        for image in &self.images {
            let ImageBody::Svg {
                probed: Some(probed),
                ..
            } = &image.body
            else {
                continue;
            };
            for g in probed.texts.iter().flat_map(|t| &t.glyphs) {
                if wanted(g.font) {
                    out.entry(g.font).or_default().insert(g.id);
                }
            }
        }
        out
    }

    /// The images of the document's image runs, for the shaper.
    pub fn inline_objects(&self) -> crate::text::InlineObjects {
        let source = |&key: &ImageKey| crate::text::ObjectSource {
            key,
            intrinsic: self.intrinsic_of(key),
        };
        let runs = self.by_run.iter().map(|(&r, k)| (r, source(k))).collect();
        let emoji = self
            .by_emoji
            .iter()
            .map(|(s, k)| (s.clone(), source(k)))
            .collect();
        crate::text::InlineObjects::new(runs, emoji)
    }

    /// Adds an image the engine draws of its own accord (a colour
    /// glyph); returns its key.
    pub fn add(&mut self, image: Arc<DecodedImage>) -> ImageKey {
        self.images.push(image);
        (self.images.len() - 1) as ImageKey
    }

    pub fn key(&self, node: &Node) -> Option<ImageKey> {
        self.by_node.get(&node_key(node)).copied()
    }

    /// The image of image run `run`.
    pub fn run_image_key(&self, run: &crate::schema::TextRun) -> Option<ImageKey> {
        self.by_run.get(&crate::text::run_key(run)).copied()
    }

    /// The images of the document's `emojiImages`, each once.
    pub fn emoji_image_keys(&self) -> BTreeSet<ImageKey> {
        self.by_emoji.values().copied().collect()
    }

    /// The background image of box `node`.
    pub fn background_key(&self, node: &Node) -> Option<ImageKey> {
        self.by_background.get(&node_key(node)).copied()
    }

    pub fn get(&self, key: ImageKey) -> &DecodedImage {
        &self.images[key as usize]
    }

    /// Intrinsic size in points (96 dpi, after EXIF orientation).
    pub fn intrinsic(&self, node: &Node) -> Option<(f32, f32)> {
        Some(self.intrinsic_of(self.key(node)?))
    }

    /// Intrinsic size of image `key` in points (96 dpi, after EXIF
    /// orientation).
    pub fn intrinsic_of(&self, key: ImageKey) -> (f32, f32) {
        let (w, h) = self.get(key).display_size();
        (w * PT_PER_PX, h * PT_PER_PX)
    }

    /// Size of an image leaf: an explicit dimension wins and the other
    /// follows the aspect ratio; with neither, the intrinsic size scaled
    /// down to fit a definite available width.
    pub fn measure(
        &self,
        node: &Node,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let Some((iw, ih)) = self.intrinsic(node) else {
            return Size::ZERO;
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
        Size {
            width: w,
            height: h,
        }
    }
}

impl ImageSet {
    /// Paints an image node into its content box according to `fit` and
    /// `objectPosition`.
    pub fn paint(
        &self,
        node: &Node,
        geo: &crate::layout::Geometry,
        ctx: &crate::paint::ContentCtx<'_>,
        ops: &mut Vec<crate::paint::Op>,
    ) {
        let NodeKind::Image {
            fit,
            object_position,
            ..
        } = &node.kind
        else {
            return;
        };
        let Some(key) = self.key(node) else {
            return;
        };
        let Rect {
            x: cx,
            y: cy,
            w: cw,
            h: ch,
        } = geo.content_box();
        let bx = Rect::new(cx + ctx.dx, cy + ctx.shift + ctx.dy, cw, ch);
        // `borderRadius` rounds the image without `overflow: hidden`.
        let radii = crate::paint::content_radii(&node.style, geo);
        let placement = Placement::new(*fit, *object_position);
        self.paint_in(key, bx, placement, &radii, ops);
    }

    /// Paints image `key` into `bx` (page coordinates) as `placement`
    /// says, clipped to `bx` rounded by `radii`.
    pub fn paint_in(
        &self,
        key: ImageKey,
        bx: Rect,
        placement: Placement,
        radii: &crate::paint::Radii,
        ops: &mut Vec<crate::paint::Op>,
    ) {
        use crate::paint::Op;
        if bx.w <= 0.0 || bx.h <= 0.0 {
            return;
        }
        let img = self.get(key);
        let intrinsic = self.intrinsic_of(key);
        let Placement { fit, position } = placement;
        let (rect, clip) = match (&img.body, fit) {
            // The box is the SVG's viewport, which its `viewBox` fits.
            (
                ImageBody::Svg {
                    view_box: Some(vb), ..
                },
                ImageFit::Fill,
            ) => vb.place(intrinsic, bx),
            _ => fit_rect(fit, position, bx, intrinsic),
        };
        ops.push(Op::Save);
        if !crate::paint::is_square(radii) {
            ops.push(Op::ClipPath {
                path: crate::paint::rounded_rect(bx, radii),
                rule: crate::paint::FillRule::NonZero,
            });
        } else if clip {
            ops.push(Op::ClipRect(bx));
        }
        ops.push(Op::Image {
            key,
            matrix: image_matrix(img.orientation, rect),
        });
        ops.push(Op::Restore);
    }
}

/// Placement of an image inside a content box: the drawn rectangle and
/// whether it must be clipped to the box. `fit` sizes the image and
/// `objectPosition` places it; `fill` covers the box and ignores the
/// position.
pub(crate) fn fit_rect(
    fit: ImageFit,
    position: ObjectPosition,
    bx: Rect,
    (iw, ih): (f32, f32),
) -> (Rect, bool) {
    /// Overflow below this (points) is rounding, not worth a clip.
    const CLIP_EPS: f32 = 1e-3;
    let Rect { x, y, w, h } = bx;
    if iw <= 0.0 || ih <= 0.0 {
        return (bx, false);
    }
    let (dw, dh) = match fit {
        ImageFit::Fill => return (bx, false),
        ImageFit::Contain => {
            let s = (w / iw).min(h / ih);
            (iw * s, ih * s)
        }
        ImageFit::Cover => {
            let s = (w / iw).max(h / ih);
            (iw * s, ih * s)
        }
        ImageFit::None => (iw, ih),
    };
    let (rx, ry) = (place(x, position.x, w - dw), place(y, position.y, h - dh));
    let clip = dw > w + CLIP_EPS
        || dh > h + CLIP_EPS
        || rx < x - CLIP_EPS
        || ry < y - CLIP_EPS
        || rx + dw > x + w + CLIP_EPS
        || ry + dh > y + h + CLIP_EPS;
    (Rect::new(rx, ry, dw, dh), clip)
}

/// Start of an image along one axis: the box's start plus the offset.
/// Validation bounds `objectPosition`, so this stays finite for a validated
/// document; should a placement still overflow `f32` (a caller that skipped
/// validation), the image is centred instead, so that `inf` or `NaN` never
/// reaches the content stream, where it would make the PDF invalid.
fn place(start: f32, p: LengthPercent, free: f32) -> f32 {
    [start + offset(p, free), start + free / 2.0]
        .into_iter()
        .find(|v| v.is_finite())
        .unwrap_or(start)
}

/// Offset of an image from the start edge of its box along one axis, given
/// the free space (box minus image; negative when the image is larger): a
/// percentage lines up that point of the image with that point of the box.
fn offset(p: LengthPercent, free: f32) -> f32 {
    match p {
        LengthPercent::Points(v) => v,
        LengthPercent::Percent(f) => free * f,
    }
}

/// Transform mapping the image unit square onto `rect` in layout space
/// (y down), honouring the EXIF orientation (1–8).
pub(crate) fn image_matrix(orientation: u8, rect: Rect) -> [f32; 6] {
    let Rect { x, y, w, h } = rect;
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

/// Decodes a JPEG, PNG, GIF or WebP embedded in an SVG, through the
/// process-wide cache.
pub(crate) fn decode_embedded(bytes: &[u8]) -> Result<Arc<DecodedImage>, String> {
    ImageData::new(bytes.into()).decoded(&ImageCrateDecoder)
}
