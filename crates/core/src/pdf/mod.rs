//! Display list → PDF bytes, through `pdf-writer`.

use std::io::Write as _;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect as PdfRect, Ref, TextStr};

use crate::date::Timestamp;
use std::collections::BTreeMap;

use pdf_writer::types::LineCapStyle;

use crate::paint::{FillRule, Op, PageContent, PathSeg};
use crate::schema::{Color, Meta};
use crate::InkoxideError;

/// Maximum kids per page-tree node (keeps viewers fast on long documents).
const PAGE_TREE_FANOUT: usize = 50;

pub(crate) struct DocumentOut<'a> {
    pub pages: &'a [PageContent],
    pub meta: &'a Meta,
    pub creation_date: Option<Timestamp>,
}

/// Dependency firewall around the PDF writer (§5.1).
pub(crate) trait PdfSink {
    fn write(&mut self, doc: &DocumentOut<'_>) -> Result<Vec<u8>, InkoxideError>;
}

pub(crate) struct PdfWriterSink {
    /// Flate level for content streams; `None` writes them uncompressed.
    pub compression: Option<u32>,
}

/// Rounds to 1/1000 pt so output has at most three decimals.
#[inline]
pub(crate) fn q(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

struct RefAlloc(i32);

impl RefAlloc {
    fn next(&mut self) -> Ref {
        self.0 += 1;
        Ref::new(self.0)
    }
}

impl PdfSink for PdfWriterSink {
    fn write(&mut self, doc: &DocumentOut<'_>) -> Result<Vec<u8>, InkoxideError> {
        let mut pdf = Pdf::new();
        let mut alloc = RefAlloc(0);
        let catalog = alloc.next();
        let info = alloc.next();

        // Pre-allocate page refs in document order: numbering stays
        // deterministic even once content streams are built in parallel.
        let page_refs: Vec<(Ref, Ref)> = doc
            .pages
            .iter()
            .map(|_| (alloc.next(), alloc.next()))
            .collect();
        let tree_root = write_page_tree(
            &mut pdf,
            &mut alloc,
            &page_refs.iter().map(|p| p.0).collect::<Vec<_>>(),
        );

        // Shared graphics states, one per distinct opacity, in value order.
        let mut opacities: BTreeMap<u16, Ref> = BTreeMap::new();
        for page in doc.pages {
            for op in &page.ops {
                if let Op::SetOpacity(a) = op {
                    opacities.entry(alpha_key(*a)).or_insert(Ref::new(1));
                }
            }
        }
        for (key, r) in opacities.iter_mut() {
            *r = alloc.next();
            let a = f32::from(*key) / 1000.0;
            pdf.ext_graphics(*r).non_stroking_alpha(a).stroking_alpha(a);
        }

        for (i, (page, (page_ref, content_ref))) in doc.pages.iter().zip(&page_refs).enumerate() {
            let parent = tree_root.leaf_parents[i / PAGE_TREE_FANOUT];
            let mut p = pdf.page(*page_ref);
            p.parent(parent)
                .media_box(PdfRect::new(0.0, 0.0, q(page.width), q(page.height)))
                .contents(*content_ref);
            let mut res = p.resources();
            let used = page_opacities(page);
            if !used.is_empty() {
                let mut gs = res.ext_g_states();
                for key in used {
                    gs.pair(Name(gs_name(key).as_bytes()), opacities[&key]);
                }
            }
            res.finish();
            p.finish();

            let raw = content_stream(page);
            match self.compression {
                Some(level) => {
                    let data = deflate(&raw, level)?;
                    pdf.stream(*content_ref, &data).filter(Filter::FlateDecode);
                }
                None => {
                    pdf.stream(*content_ref, &raw);
                }
            }
        }

        let mut cat = pdf.catalog(catalog);
        cat.pages(tree_root.root);
        if let Some(lang) = &doc.meta.lang {
            cat.lang(TextStr(lang));
        }
        cat.finish();

        let mut inf = pdf.document_info(info);
        let m = doc.meta;
        if let Some(v) = &m.title {
            inf.title(TextStr(v));
        }
        if let Some(v) = &m.author {
            inf.author(TextStr(v));
        }
        if let Some(v) = &m.subject {
            inf.subject(TextStr(v));
        }
        if let Some(v) = &m.keywords {
            inf.keywords(TextStr(v));
        }
        if let Some(v) = &m.creator {
            inf.creator(TextStr(v));
        }
        inf.producer(TextStr(concat!("Inkoxide ", env!("CARGO_PKG_VERSION"))));
        if let Some(t) = doc.creation_date {
            inf.creation_date(pdf_date(t));
        }
        inf.finish();

        Ok(pdf.finish())
    }
}

fn pdf_date(t: Timestamp) -> pdf_writer::Date {
    let off = t.offset_minutes;
    let date = pdf_writer::Date::new(t.year)
        .month(t.month)
        .day(t.day)
        .hour(t.hour)
        .minute(t.minute)
        .second(t.second);
    if off == 0 {
        date.utc_offset_hour(0).utc_offset_minute(0)
    } else {
        date.utc_offset_hour((off / 60) as i8)
            .utc_offset_minute((off.unsigned_abs() % 60) as u8)
    }
}

fn deflate(data: &[u8], level: u32) -> Result<Vec<u8>, InkoxideError> {
    let mut enc = ZlibEncoder::new(
        Vec::with_capacity(data.len() / 3 + 64),
        Compression::new(level),
    );
    enc.write_all(data)
        .and_then(|_| enc.finish())
        .map_err(|e| InkoxideError::Pdf(format!("compression failed: {e}")))
}

/// Page tree: flat when small, otherwise intermediate nodes of at most
/// [`PAGE_TREE_FANOUT`] kids.
struct PageTree {
    root: Ref,
    /// Parent of pages `[i * FANOUT, (i + 1) * FANOUT)`.
    leaf_parents: Vec<Ref>,
}

fn write_page_tree(pdf: &mut Pdf, alloc: &mut RefAlloc, pages: &[Ref]) -> PageTree {
    let root = alloc.next();
    if pages.len() <= PAGE_TREE_FANOUT {
        pdf.pages(root)
            .kids(pages.iter().copied())
            .count(pages.len() as i32);
        return PageTree {
            root,
            leaf_parents: vec![root],
        };
    }

    // Build bottom-up: level 0 groups pages; each next level groups nodes.
    // `level` holds (node ref, kids, page count).
    let mut level: Vec<(Ref, Vec<Ref>, usize)> = pages
        .chunks(PAGE_TREE_FANOUT)
        .map(|c| (alloc.next(), c.to_vec(), c.len()))
        .collect();
    let leaf_parents: Vec<Ref> = level.iter().map(|n| n.0).collect();
    let mut parent_of: std::collections::HashMap<Ref, Ref> = Default::default();
    let mut nodes: Vec<(Ref, Vec<Ref>, usize)> = Vec::new();
    while level.len() > PAGE_TREE_FANOUT {
        let next: Vec<(Ref, Vec<Ref>, usize)> = level
            .chunks(PAGE_TREE_FANOUT)
            .map(|c| {
                let r = alloc.next();
                (
                    r,
                    c.iter().map(|n| n.0).collect(),
                    c.iter().map(|n| n.2).sum(),
                )
            })
            .collect();
        for (r, kids, _) in &next {
            for k in kids {
                parent_of.insert(*k, *r);
            }
        }
        nodes.extend(level);
        level = next;
    }
    for n in &level {
        parent_of.insert(n.0, root);
    }
    nodes.extend(level.iter().cloned());
    let total = pages.len() as i32;
    pdf.pages(root).kids(level.iter().map(|n| n.0)).count(total);
    for (r, kids, count) in nodes {
        let parent = parent_of.get(&r).copied().unwrap_or(root);
        pdf.pages(r).parent(parent).kids(kids).count(count as i32);
    }
    PageTree { root, leaf_parents }
}

/// Opacity quantised to 1/1000: the key of its shared graphics state.
fn alpha_key(a: f32) -> u16 {
    (a.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn gs_name(key: u16) -> String {
    format!("A{key}")
}

fn page_opacities(page: &PageContent) -> std::collections::BTreeSet<u16> {
    page.ops
        .iter()
        .filter_map(|op| match op {
            Op::SetOpacity(a) => Some(alpha_key(*a)),
            _ => None,
        })
        .collect()
}

fn set_fill(c: &mut Content, color: &Color) {
    c.set_fill_rgb(q(color[0]), q(color[1]), q(color[2]));
}

fn path(c: &mut Content, segs: &[PathSeg]) {
    for s in segs {
        match *s {
            PathSeg::MoveTo(x, y) => {
                c.move_to(q(x), q(y));
            }
            PathSeg::LineTo(x, y) => {
                c.line_to(q(x), q(y));
            }
            PathSeg::CubicTo(x1, y1, x2, y2, x3, y3) => {
                c.cubic_to(q(x1), q(y1), q(x2), q(y2), q(x3), q(y3));
            }
            PathSeg::Close => {
                c.close_path();
            }
        }
    }
}

/// Serialises one page's display list. Pure function of the page: safe to
/// run per page in parallel.
pub(crate) fn content_stream(page: &PageContent) -> Vec<u8> {
    let mut c = Content::new();
    // Flip to a top-left origin once; everything below is in layout space.
    c.transform([1.0, 0.0, 0.0, -1.0, 0.0, q(page.height)]);
    for op in &page.ops {
        match op {
            Op::Save => {
                c.save_state();
            }
            Op::Restore => {
                c.restore_state();
            }
            Op::Transform(m) => {
                c.transform(m.map(|v| (v * 1e5).round() / 1e5));
            }
            Op::ClipRect(r) => {
                c.rect(q(r.x), q(r.y), q(r.w), q(r.h));
                c.clip_nonzero();
                c.end_path();
            }
            Op::ClipPath { path: p, rule } => {
                path(&mut c, &p.segs);
                match rule {
                    FillRule::NonZero => c.clip_nonzero(),
                    FillRule::EvenOdd => c.clip_even_odd(),
                };
                c.end_path();
            }
            Op::FillRect { rect, color } => {
                set_fill(&mut c, color);
                c.rect(q(rect.x), q(rect.y), q(rect.w), q(rect.h));
                c.fill_nonzero();
            }
            Op::FillPath {
                path: p,
                color,
                rule,
            } => {
                set_fill(&mut c, color);
                path(&mut c, &p.segs);
                match rule {
                    FillRule::NonZero => c.fill_nonzero(),
                    FillRule::EvenOdd => c.fill_even_odd(),
                };
            }
            Op::StrokePath {
                path: p,
                color,
                width,
                dash,
            } => {
                c.save_state();
                c.set_stroke_rgb(q(color[0]), q(color[1]), q(color[2]));
                c.set_line_width(q(*width));
                if let Some(d) = dash {
                    c.set_dash_pattern(d.pattern.map(q), 0.0);
                    if d.round_cap {
                        c.set_line_cap(LineCapStyle::RoundCap);
                    }
                }
                path(&mut c, &p.segs);
                c.stroke();
                c.restore_state();
            }
            Op::SetOpacity(a) => {
                c.set_parameters(Name(gs_name(alpha_key(*a)).as_bytes()));
            }
        }
    }
    c.finish().into_vec()
}
