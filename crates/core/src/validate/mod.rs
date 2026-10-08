//! Structural (parse) and semantic validation. Always runs before render;
//! a document with any error is never rendered.

// `!(v >= 0.0)` is deliberate throughout: it also rejects NaN, which
// non-JSON inputs (MessagePack) can carry.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Deserialize;

use crate::date::parse_rfc3339;
use crate::error::{ValidationCode as Code, ValidationError};
use crate::schema::{
    Color, Document, Length, LengthPercent, Node, NodeKind, OptEdges, PageSpec, Style, Table,
    TextRun, TrackSize, SCHEMA_VERSION,
};
use crate::{FontStore, ImageStore, InputFormat};

/// Stop collecting after this many problems; the rest is usually noise.
const MAX_ERRORS: usize = 100;

/// Largest page edge PDF viewers accept (200 in, PDF 1.6+ user unit aside).
const MAX_PAGE_EDGE: f32 = 14_400.0;

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

pub(crate) fn parse(doc: &[u8], format: InputFormat) -> Result<Document, ValidationError> {
    match format {
        InputFormat::Json => parse_json(doc),
    }
}

fn parse_json(doc: &[u8]) -> Result<Document, ValidationError> {
    let mut de = serde_json::Deserializer::from_slice(doc);
    let result: Result<Document, _> = serde_path_to_error::deserialize(&mut de);
    let err = match result {
        Ok(d) => {
            return de.end().map(|_| d).map_err(|e| ValidationError {
                code: Code::ParseError,
                path: String::new(),
                message: e.to_string(),
            })
        }
        Err(e) => e,
    };

    // A document from another schema version usually fails structurally;
    // report the version, not the symptoms.
    if let Some(v) = probe_version(doc) {
        if v != SCHEMA_VERSION {
            return Err(unsupported_version(&v.to_string()));
        }
    }

    let path = format_path(err.path());
    let inner = err.into_inner();
    let raw = inner.to_string();
    let msg = match raw.rfind(" at line ") {
        Some(i) if !inner.is_syntax() && !inner.is_eof() => raw[..i].to_string(),
        _ => raw,
    };
    if inner.is_syntax() || inner.is_eof() || inner.is_io() {
        return Err(ValidationError {
            code: Code::ParseError,
            path: String::new(),
            message: msg,
        });
    }
    Err(classify(path, msg))
}

fn probe_version(doc: &[u8]) -> Option<serde_json::Value> {
    #[derive(Deserialize)]
    struct Probe {
        version: Option<serde_json::Value>,
    }
    serde_json::from_slice::<Probe>(doc).ok()?.version
}

fn unsupported_version(found: &str) -> ValidationError {
    ValidationError {
        code: Code::UnsupportedVersion,
        path: "version".into(),
        message: format!(
            "document version {found} is not supported; this engine reads version {SCHEMA_VERSION}"
        ),
    }
}

fn format_path(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    let mut out = String::new();
    for seg in path.iter() {
        match seg {
            Segment::Seq { index } => {
                let _ = write!(out, "[{index}]");
            }
            Segment::Map { key } => push_key(&mut out, key),
            Segment::Enum { variant } => push_key(&mut out, variant),
            Segment::Unknown => push_key(&mut out, "?"),
        }
    }
    out
}

fn push_key(out: &mut String, key: &str) {
    if !out.is_empty() {
        out.push('.');
    }
    out.push_str(key);
}

fn backticked(msg: &str) -> Option<&str> {
    let start = msg.find('`')? + 1;
    let len = msg[start..].find('`')?;
    Some(&msg[start..start + len])
}

/// Maps a serde message to a structured error. Our own deserialisers prefix
/// messages with `[CODE]` or `[CODE@field]`.
fn classify(mut path: String, msg: String) -> ValidationError {
    if let Some(rest) = msg.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let (tag, message) = (&rest[..end], rest[end + 1..].trim_start());
            let (code, field) = match tag.split_once('@') {
                Some((c, f)) => (c, Some(f)),
                None => (tag, None),
            };
            if let Some(code) = Code::from_str(code) {
                if let Some(f) = field {
                    push_key(&mut path, f);
                }
                return ValidationError {
                    code,
                    path,
                    message: message.to_string(),
                };
            }
        }
    }
    let (code, field) = if msg.starts_with("unknown field") {
        (Code::UnknownField, backticked(&msg))
    } else if msg.starts_with("missing field") {
        (Code::MissingField, backticked(&msg))
    } else if msg.starts_with("unknown variant")
        || msg.starts_with("invalid value")
        || msg.starts_with("invalid length")
    {
        (Code::InvalidValue, None)
    } else if msg.starts_with("invalid type") || msg.starts_with("data did not match") {
        (Code::InvalidType, None)
    } else {
        (Code::ParseError, None)
    };
    let message = match (code, field) {
        // serde lists every accepted field; for the raw node struct that
        // list is misleading, so keep the message short.
        (Code::UnknownField, Some(f)) => format!("unknown field `{f}`"),
        (Code::MissingField, Some(f)) => format!("missing required field `{f}`"),
        _ => msg.clone(),
    };
    // serde_path_to_error already ends the path at an unknown key.
    if let (Code::MissingField, Some(f)) = (code, field) {
        push_key(&mut path, f);
    }
    ValidationError {
        code,
        path,
        message,
    }
}

// ---------------------------------------------------------------------------
// Semantic checks
// ---------------------------------------------------------------------------

pub(crate) fn check(
    doc: &Document,
    fonts: &FontStore,
    images: &ImageStore,
) -> Vec<ValidationError> {
    let mut c = Checker {
        path: String::with_capacity(128),
        errors: Vec::new(),
        declared: BTreeSet::new(),
        fonts,
        images,
    };
    c.document(doc);
    c.errors
}

struct Checker<'a> {
    path: String,
    errors: Vec<ValidationError>,
    declared: BTreeSet<&'a str>,
    fonts: &'a FontStore,
    images: &'a ImageStore,
}

/// Where a node sits; some node types are only valid in the flow.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Flow,
    Fixed,
    Cell,
}

impl<'a> Checker<'a> {
    fn err(&mut self, code: Code, field: Option<&str>, message: impl Into<String>) {
        if self.errors.len() >= MAX_ERRORS {
            return;
        }
        let mut path = self.path.clone();
        if let Some(f) = field {
            push_key(&mut path, f);
        }
        self.errors.push(ValidationError {
            code,
            path,
            message: message.into(),
        });
    }

    /// Runs `f` with `seg` appended to the current path.
    fn at<R>(
        &mut self,
        seg: impl std::fmt::Display,
        index: bool,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let len = self.path.len();
        if index {
            let _ = write!(self.path, "[{seg}]");
        } else {
            if !self.path.is_empty() {
                self.path.push('.');
            }
            let _ = write!(self.path, "{seg}");
        }
        let r = f(self);
        self.path.truncate(len);
        r
    }

    fn document(&mut self, doc: &'a Document) {
        if doc.version != SCHEMA_VERSION {
            self.errors
                .push(unsupported_version(&doc.version.to_string()));
            return;
        }
        if let Some(date) = &doc.meta.creation_date {
            if parse_rfc3339(date).is_none() {
                self.at("meta", false, |c| {
                    c.err(
                        Code::InvalidValue,
                        Some("creationDate"),
                        format!("`{date}` is not an RFC 3339 timestamp"),
                    )
                });
            }
        }

        let mut seen = BTreeMap::new();
        for (i, f) in doc.fonts.iter().enumerate() {
            self.at("fonts", false, |c| {
                c.at(i, true, |c| {
                    if let Some(prev) = seen.insert(f.id.as_str(), i) {
                        c.err(Code::DuplicateId, Some("id"), format!("font id `{}` is already declared at fonts[{prev}]", f.id));
                    }
                    if !(1..=1000).contains(&f.weight) {
                        c.err(Code::OutOfRange, Some("weight"), "font weight must be within 1..=1000");
                    }
                    if !c.fonts.contains(&f.id) {
                        c.err(
                            Code::FontNotRegistered,
                            Some("id"),
                            format!("font `{}` is declared but no font data was registered under that id", f.id),
                        );
                    }
                })
            });
            self.declared.insert(&f.id);
        }
        for (i, id) in doc.fallback_fonts.iter().enumerate() {
            if !self.declared.contains(id.as_str()) {
                self.at("fallbackFonts", false, |c| {
                    c.at(i, true, |c| {
                        c.err(
                            Code::UnknownFont,
                            None,
                            format!("fallback font `{id}` is not declared in `fonts`"),
                        )
                    })
                });
            }
        }
        for (i, page) in doc.pages.iter().enumerate() {
            self.at("pages", false, |c| c.at(i, true, |c| c.page(page)));
        }
    }

    fn page(&mut self, page: &PageSpec) {
        let s = page.size;
        for (name, v) in [("width", s.width), ("height", s.height)] {
            if !(v > 0.0 && v <= MAX_PAGE_EDGE) {
                self.at("size", false, |c| {
                    c.err(
                        Code::OutOfRange,
                        Some(name),
                        format!("page {name} must be within (0, {MAX_PAGE_EDGE}] points"),
                    )
                });
            }
        }
        let m = page.margin;
        for (name, v) in [
            ("top", m.top),
            ("right", m.right),
            ("bottom", m.bottom),
            ("left", m.left),
        ] {
            if !(v >= 0.0) {
                self.at("margin", false, |c| {
                    c.err(Code::OutOfRange, Some(name), "margin must be non-negative")
                });
            }
        }
        if m.left + m.right >= s.width || m.top + m.bottom >= s.height {
            self.err(
                Code::OutOfRange,
                Some("margin"),
                "margins leave no room for content",
            );
        }
        if let Some(bg) = &page.background {
            self.color(Some("background"), bg);
        }
        if let Some(h) = &page.header {
            self.at("header", false, |c| c.node(h, Ctx::Fixed));
        }
        if let Some(f) = &page.footer {
            self.at("footer", false, |c| c.node(f, Ctx::Fixed));
        }
        self.children(&page.children, Ctx::Flow);
    }

    fn children(&mut self, children: &[Node], ctx: Ctx) {
        self.at("children", false, |c| {
            for (i, n) in children.iter().enumerate() {
                c.at(i, true, |c| c.node(n, ctx));
            }
        });
    }

    fn node(&mut self, node: &Node, ctx: Ctx) {
        if self.errors.len() >= MAX_ERRORS {
            return;
        }
        let is_text = matches!(node.kind, NodeKind::Text { .. });
        self.at("style", false, |c| c.style(&node.style, is_text, false));
        if let Some(v) = node.brk.min_presence_ahead {
            if !(v >= 0.0) {
                self.at("break", false, |c| {
                    c.err(
                        Code::OutOfRange,
                        Some("minPresenceAhead"),
                        "must be non-negative",
                    )
                });
            }
        }
        if ctx != Ctx::Flow && node.brk != Default::default() {
            self.err(Code::InvalidValue, Some("break"), "pagination controls only apply to flowing content, not inside headers, footers or table cells");
        }

        match &node.kind {
            NodeKind::Box { children } => self.children(children, ctx),
            NodeKind::Text { runs } => {
                self.at("runs", false, |c| {
                    for (i, r) in runs.iter().enumerate() {
                        c.at(i, true, |c| c.run(r));
                    }
                });
                if node.style.ellipsis == Some(true) && node.style.max_lines.is_none() {
                    self.at("style", false, |c| {
                        c.err(
                            Code::InvalidValue,
                            Some("ellipsis"),
                            "`ellipsis` requires `maxLines`",
                        )
                    });
                }
            }
            NodeKind::Image { source, .. } => match source {
                crate::schema::ImageSource::Ref(id) => match self.images.get(id) {
                    None => self.err(
                        Code::UnknownImage,
                        Some("src"),
                        format!("image `{id}` was not provided in the render options"),
                    ),
                    Some(data) => {
                        if let Err(e) = crate::images::probe(&data.bytes) {
                            self.err(
                                Code::InvalidImage,
                                Some("src"),
                                format!("image `{id}`: {e}"),
                            );
                        }
                    }
                },
                crate::schema::ImageSource::Inline(data) => {
                    match crate::images::decode_inline(data) {
                        Err(e) => self.err(Code::InvalidImage, Some("data"), e),
                        Ok(bytes) => {
                            if let Err(e) = crate::images::probe(&bytes) {
                                self.err(Code::InvalidImage, Some("data"), e);
                            }
                        }
                    }
                }
            },
            NodeKind::Table(t) => self.table(t),
            NodeKind::PageBreak => {
                if ctx != Ctx::Flow {
                    self.err(
                        Code::InvalidNode,
                        None,
                        "`pageBreak` is only allowed in flowing content",
                    );
                }
            }
            NodeKind::Spacer => {}
        }
    }

    fn run(&mut self, r: &TextRun) {
        match (&r.text, &r.placeholder) {
            (Some(_), Some(_)) => self.err(
                Code::InvalidValue,
                Some("placeholder"),
                "a run takes either `text` or `placeholder`, not both",
            ),
            (None, None) => self.err(
                Code::MissingField,
                Some("text"),
                "a run requires `text` (or a `placeholder`)",
            ),
            _ => {}
        }
        if !self.declared.contains(r.font.as_str()) {
            self.err(
                Code::UnknownFont,
                Some("font"),
                format!("font `{}` is not declared in `fonts`", r.font),
            );
        }
        if !(r.size > 0.0 && r.size <= 1000.0) {
            self.err(
                Code::OutOfRange,
                Some("size"),
                "font size must be within (0, 1000]",
            );
        }
        if let Some(col) = &r.color {
            self.color(Some("color"), col);
        }
        if let Some(ls) = r.letter_spacing {
            if ls.abs() > 1000.0 {
                self.err(
                    Code::OutOfRange,
                    Some("letterSpacing"),
                    "letter spacing must be within ±1000 points",
                );
            }
        }
    }

    fn table(&mut self, t: &Table) {
        let ncols = t.columns.len();
        if ncols == 0 {
            self.err(
                Code::InvalidTable,
                Some("columns"),
                "a table needs at least one column",
            );
            return;
        }
        if t.header_rows as usize > t.rows.len() {
            self.err(
                Code::InvalidTable,
                Some("headerRows"),
                format!(
                    "headerRows ({}) exceeds the number of rows ({})",
                    t.header_rows,
                    t.rows.len()
                ),
            );
        }
        // Occupancy: how many more rows each column is covered by a rowSpan from above.
        let mut carry = vec![0u32; ncols];
        let nrows = t.rows.len();
        self.at("rows", false, |c| {
            for (ri, row) in t.rows.iter().enumerate() {
                c.at(ri, true, |c| {
                    if let Some(h) = row.min_height {
                        if !(h >= 0.0) {
                            c.err(Code::OutOfRange, Some("minHeight"), "must be non-negative");
                        }
                    }
                    if let Some(bg) = &row.background_color {
                        c.color(Some("backgroundColor"), bg);
                    }
                    let mut col = 0usize;
                    for (ci, cell) in row.cells.iter().enumerate() {
                        c.at("cells", false, |c| {
                            c.at(ci, true, |c| {
                                while col < ncols && carry[col] > 0 {
                                    col += 1;
                                }
                                let span = cell.col_span.max(1) as usize;
                                if cell.col_span == 0 || cell.row_span == 0 {
                                    c.err(
                                        Code::InvalidTable,
                                        None,
                                        "colSpan and rowSpan must be at least 1",
                                    );
                                }
                                if col + span > ncols || (col..col + span).any(|k| carry[k] > 0) {
                                    c.err(
                                        Code::InvalidTable,
                                        None,
                                        format!("cell does not fit: row has {ncols} columns"),
                                    );
                                } else if ri + cell.row_span as usize > nrows {
                                    c.err(
                                        Code::InvalidTable,
                                        Some("rowSpan"),
                                        "rowSpan extends past the last row",
                                    );
                                } else if ri < t.header_rows as usize
                                    && ri + cell.row_span as usize > t.header_rows as usize
                                {
                                    c.err(
                                        Code::InvalidTable,
                                        Some("rowSpan"),
                                        "a header cell cannot span into body rows",
                                    );
                                } else {
                                    carry[col..col + span].fill(cell.row_span);
                                }
                                col += span;
                                c.at("style", false, |c| c.style(&cell.style, false, true));
                                c.children(&cell.children, Ctx::Cell);
                            })
                        });
                    }
                    while col < ncols && carry[col] > 0 {
                        col += 1;
                    }
                    if col < ncols {
                        c.err(
                            Code::InvalidTable,
                            Some("cells"),
                            format!("row covers {col} of {ncols} columns"),
                        );
                    }
                    for v in carry.iter_mut() {
                        *v = v.saturating_sub(1);
                    }
                })
            }
        });
    }

    fn color(&mut self, field: Option<&str>, c: &Color) {
        if c.iter().any(|v| !(0.0..=1.0).contains(v)) {
            self.err(
                Code::OutOfRange,
                field,
                "colour components must be within [0, 1]",
            );
        }
    }

    fn non_neg(&mut self, field: &str, v: Option<f32>) {
        if let Some(v) = v {
            if !(v >= 0.0) {
                self.err(Code::OutOfRange, Some(field), "must be non-negative");
            }
        }
    }

    fn length(&mut self, field: &str, v: Option<Length>, allow_negative: bool) {
        match v {
            Some(Length::Points(p) | Length::Percent(p)) if !allow_negative && p < 0.0 => {
                self.err(Code::OutOfRange, Some(field), "must be non-negative")
            }
            _ => {}
        }
    }

    fn style(&mut self, s: &Style, is_text: bool, is_cell: bool) {
        if !is_text {
            let props: Vec<_> = s.text_props_set().collect();
            for p in props {
                self.err(
                    Code::StyleNotApplicable,
                    Some(p),
                    format!("`{p}` only applies to text nodes"),
                );
            }
        }
        if !is_cell && s.vertical_align.is_some() {
            self.err(
                Code::StyleNotApplicable,
                Some("verticalAlign"),
                "`verticalAlign` only applies to table cells",
            );
        }
        self.non_neg("flexGrow", s.flex_grow);
        self.non_neg("flexShrink", s.flex_shrink);
        self.non_neg("borderRadius", s.border_radius);
        self.length("flexBasis", s.flex_basis, false);
        for (name, v) in [
            ("width", s.width),
            ("height", s.height),
            ("minWidth", s.min_width),
            ("minHeight", s.min_height),
            ("maxWidth", s.max_width),
            ("maxHeight", s.max_height),
        ] {
            self.length(name, v, false);
        }
        for (name, v) in [
            ("gap", s.gap),
            ("rowGap", s.row_gap),
            ("columnGap", s.column_gap),
        ] {
            if let Some(LengthPercent::Points(p) | LengthPercent::Percent(p)) = v {
                if p < 0.0 {
                    self.err(Code::OutOfRange, Some(name), "must be non-negative");
                }
            }
        }
        if let Some(p) = &s.padding {
            self.edges_lp("padding", p);
        }
        if let Some(bw) = &s.border_width {
            for (k, v) in bw.iter() {
                if !(*v >= 0.0) {
                    let f = format!("borderWidth.{k}");
                    self.err(Code::OutOfRange, Some(&f), "must be non-negative");
                }
            }
        }
        if let Some(bc) = &s.border_color {
            for (k, v) in bc.iter() {
                let f = format!("borderColor.{k}");
                self.color(Some(&f), v);
            }
        }
        if let Some(bg) = &s.background_color {
            self.color(Some("backgroundColor"), bg);
        }
        if let Some(o) = s.opacity {
            if !(0.0..=1.0).contains(&o) {
                self.err(Code::OutOfRange, Some("opacity"), "must be within [0, 1]");
            }
        }
        if let Some(a) = s.aspect_ratio {
            if !(a > 0.0) {
                self.err(Code::OutOfRange, Some("aspectRatio"), "must be positive");
            }
        }
        if let Some(lh) = s.line_height {
            if !(lh > 0.0 && lh <= 100.0) {
                self.err(
                    Code::OutOfRange,
                    Some("lineHeight"),
                    "must be within (0, 100]",
                );
            }
        }
        if s.max_lines == Some(0) {
            self.err(Code::OutOfRange, Some("maxLines"), "must be at least 1");
        }
        for (name, tracks) in [
            ("gridTemplateColumns", &s.grid_template_columns),
            ("gridTemplateRows", &s.grid_template_rows),
        ] {
            if let Some(tracks) = tracks {
                if tracks
                    .iter()
                    .any(|t| matches!(t, TrackSize::Points(v) | TrackSize::Percent(v) if *v < 0.0))
                {
                    self.err(
                        Code::OutOfRange,
                        Some(name),
                        "track sizes must be non-negative",
                    );
                }
            }
        }
    }

    fn edges_lp(&mut self, field: &str, e: &OptEdges<LengthPercent>) {
        for (k, v) in e.iter() {
            let (LengthPercent::Points(p) | LengthPercent::Percent(p)) = v;
            if *p < 0.0 {
                let f = format!("{field}.{k}");
                self.err(Code::OutOfRange, Some(&f), "must be non-negative");
            }
        }
    }
}

#[cfg(test)]
mod tests;
