//! The paths text goes along (`<textPath>`): the geometry of the element
//! a `textPath` refers to, read from the XML (it is usually in `<defs>`,
//! which the converted tree leaves out), and an arc-length table to find
//! the point and the direction at a distance along it.
//!
//! The table has a fixed number of pieces per curve, each measured with
//! Gauss–Legendre quadrature: the work is bounded and the same on every
//! machine, where an adaptive arc length could recurse deep on a crafted
//! cusp.

use std::str::FromStr;

use svgtypes::{Length, LengthUnit, SimplePathSegment};
use usvg::roxmltree::Node;

use super::extract::TextError;

/// Most segments the paths of an SVG's text may have, over every element
/// a `textPath` refers to (each counted once): their tables take about
/// 200 bytes a segment.
pub(crate) const MAX_TEXT_PATH_SEGMENTS: usize = 100_000;

/// Pieces a curve is measured in.
const PIECES: usize = 8;

/// Gauss–Legendre nodes and weights on [-1, 1], five points.
const GAUSS: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (-0.538_469_310_105_683, 0.478_628_670_499_366_5),
    (0.538_469_310_105_683, 0.478_628_670_499_366_5),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_08),
    (0.906_179_845_938_664, 0.236_926_885_056_189_08),
];

type Point = [f64; 2];

/// A segment: a line, or a cubic Bézier curve.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Segment {
    Line(Point, Point),
    Cubic([Point; 4]),
}

impl Segment {
    fn at(&self, t: f64) -> Point {
        match *self {
            Segment::Line(a, b) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
            Segment::Cubic([p0, p1, p2, p3]) => {
                let u = 1.0 - t;
                let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                [
                    a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
                    a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
                ]
            }
        }
    }

    /// The derivative at `t`.
    fn deriv(&self, t: f64) -> Point {
        match *self {
            Segment::Line(a, b) => [b[0] - a[0], b[1] - a[1]],
            Segment::Cubic([p0, p1, p2, p3]) => {
                let u = 1.0 - t;
                let (a, b, c) = (3.0 * u * u, 6.0 * u * t, 3.0 * t * t);
                [
                    a * (p1[0] - p0[0]) + b * (p2[0] - p1[0]) + c * (p3[0] - p2[0]),
                    a * (p1[1] - p0[1]) + b * (p2[1] - p1[1]) + c * (p3[1] - p2[1]),
                ]
            }
        }
    }

    fn reversed(self) -> Self {
        match self {
            Segment::Line(a, b) => Segment::Line(b, a),
            Segment::Cubic([p0, p1, p2, p3]) => Segment::Cubic([p3, p2, p1, p0]),
        }
    }

    fn ends(&self) -> (Point, Point) {
        match *self {
            Segment::Line(a, b) => (a, b),
            Segment::Cubic(p) => (p[0], p[3]),
        }
    }

    /// The length from `t0` to `t1`.
    fn length(&self, t0: f64, t1: f64) -> f64 {
        if let Segment::Line(a, b) = *self {
            return (b[0] - a[0]).hypot(b[1] - a[1]) * (t1 - t0);
        }
        let (mid, half) = (f64::midpoint(t0, t1), (t1 - t0) / 2.0);
        GAUSS
            .iter()
            .map(|&(x, w)| {
                let d = self.deriv(mid + half * x);
                w * d[0].hypot(d[1])
            })
            .sum::<f64>()
            * half
    }
}

/// A piece of a segment and where it starts along the path.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    segment: u32,
    t0: f32,
    t1: f32,
    /// The length along the path where it starts, and its own.
    start: f32,
    length: f32,
}

/// A path text goes along, in the text's user space, with its
/// arc-length table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextPathGeom {
    segments: Vec<Segment>,
    pieces: Vec<Piece>,
    length: f32,
}

/// Where a distance along a path is, and which way the path goes there:
/// degrees clockwise from the x axis, y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PathPoint {
    pub x: f32,
    pub y: f32,
    pub angle: f32,
}

impl TextPathGeom {
    /// The geometry of `segments` (reversed for `side="right"`); `None`
    /// without any length, or with a point that is not finite.
    fn new(mut segments: Vec<Segment>, reverse: bool) -> Option<Self> {
        if reverse {
            segments.reverse();
            for s in &mut segments {
                *s = s.reversed();
            }
        }
        let mut pieces = Vec::new();
        let mut at = 0.0f64;
        for (i, s) in segments.iter().enumerate() {
            let n = if matches!(s, Segment::Line(..)) {
                1
            } else {
                PIECES
            };
            for k in 0..n {
                let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
                let length = s.length(t0, t1);
                pieces.push(Piece {
                    segment: i as u32,
                    t0: t0 as f32,
                    t1: t1 as f32,
                    start: at as f32,
                    length: length as f32,
                });
                at += length;
            }
        }
        // Points and lengths go out as f32: what overflows it is no path.
        let fits = |v: f64| v.abs() <= f64::from(f32::MAX);
        let finite = segments.iter().all(|s| {
            let (a, b) = s.ends();
            let c = match s {
                Segment::Cubic(p) => [p[1], p[2]],
                Segment::Line(..) => [a, b],
            };
            [a, b, c[0], c[1]].iter().all(|p| fits(p[0]) && fits(p[1]))
        });
        (finite && fits(at) && at > 0.0).then_some(Self {
            segments,
            pieces,
            length: at as f32,
        })
    }

    /// Its total length.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// Bytes it holds.
    pub fn heap_size(&self) -> usize {
        self.segments.capacity() * size_of::<Segment>()
            + self.pieces.capacity() * size_of::<Piece>()
    }

    /// The point `distance` along the path; `None` off it (before its
    /// start or past its end, closed or not).
    pub fn at(&self, distance: f32) -> Option<PathPoint> {
        if !(0.0..=self.length).contains(&distance) {
            return None;
        }
        let i = self
            .pieces
            .partition_point(|p| p.start + p.length < distance)
            .min(self.pieces.len() - 1);
        let p = self.pieces[i];
        let s = &self.segments[p.segment as usize];
        // Within the piece, the parameter in proportion to the length,
        // then two Newton steps on a curve, which is not as fast at every
        // point.
        let (t0, t1) = (f64::from(p.t0), f64::from(p.t1));
        let want = f64::from(distance - p.start);
        let f = if p.length > 0.0 {
            (want / f64::from(p.length)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut t = t0 + (t1 - t0) * f;
        if matches!(s, Segment::Cubic(_)) {
            for _ in 0..2 {
                let d = s.deriv(t);
                let speed = d[0].hypot(d[1]);
                if speed < 1e-9 {
                    break;
                }
                t = (t - (s.length(t0, t) - want) / speed).clamp(t0, t1);
            }
        }
        let point = s.at(t);
        let mut d = s.deriv(t);
        // A cusp has no direction: take the segment's chord.
        if d[0].hypot(d[1]) < 1e-9 {
            let (a, b) = s.ends();
            d = [b[0] - a[0], b[1] - a[1]];
        }
        Some(PathPoint {
            x: point[0] as f32,
            y: point[1] as f32,
            angle: d[1].atan2(d[0]).to_degrees() as f32,
        })
    }
}

/// Collects the segments of path data, of shapes and their transform.
struct Builder {
    segments: Vec<Segment>,
    /// The start of the subpath and the current point.
    start: Point,
    at: Point,
    ts: svgtypes::Transform,
    /// Segments every path read so far has, against
    /// [`MAX_TEXT_PATH_SEGMENTS`].
    budget: usize,
}

impl Builder {
    fn map(&self, [x, y]: Point) -> Point {
        let t = &self.ts;
        [t.a * x + t.c * y + t.e, t.b * x + t.d * y + t.f]
    }

    fn push(&mut self, s: Segment) -> Result<(), TextError> {
        if self.budget == 0 {
            return Err(TextError::Limit(format!(
                "its text paths have more than {MAX_TEXT_PATH_SEGMENTS} segments"
            )));
        }
        self.budget -= 1;
        self.segments.push(s);
        Ok(())
    }

    fn move_to(&mut self, p: Point) {
        self.start = p;
        self.at = p;
    }

    fn line_to(&mut self, p: Point) -> Result<(), TextError> {
        let s = Segment::Line(self.map(self.at), self.map(p));
        self.at = p;
        self.push(s)
    }

    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) -> Result<(), TextError> {
        let s = Segment::Cubic([self.map(self.at), self.map(c1), self.map(c2), self.map(p)]);
        self.at = p;
        self.push(s)
    }

    fn close(&mut self) -> Result<(), TextError> {
        if self.at != self.start {
            self.line_to(self.start)?;
        }
        self.at = self.start;
        Ok(())
    }

    /// A quarter ellipse from the current point, centred at `c`, ending at
    /// `p`.
    fn quarter(&mut self, c: Point, p: Point) -> Result<(), TextError> {
        // The control points are k along the tangents at each end.
        const K: f64 = 0.552_284_749_830_793_4;
        let a = self.at;
        let c1 = [a[0] + K * (p[0] - c[0]), a[1] + K * (p[1] - c[1])];
        let c2 = [p[0] + K * (a[0] - c[0]), p[1] + K * (a[1] - c[1])];
        self.cubic_to(c1, c2, p)
    }

    /// An ellipse, clockwise from its rightmost point (SVG 2, 10.4).
    fn ellipse(&mut self, [cx, cy]: Point, rx: f64, ry: f64) -> Result<(), TextError> {
        self.move_to([cx + rx, cy]);
        self.quarter([cx, cy], [cx, cy + ry])?;
        self.quarter([cx, cy], [cx - rx, cy])?;
        self.quarter([cx, cy], [cx, cy - ry])?;
        self.quarter([cx, cy], [cx + rx, cy])
    }
}

/// The segments of path data `d`, up to its first error.
fn path_data(b: &mut Builder, d: &str) -> Result<(), TextError> {
    for seg in svgtypes::SimplifyingPathParser::from(d) {
        let Ok(seg) = seg else { break };
        match seg {
            SimplePathSegment::MoveTo { x, y } => b.move_to([x, y]),
            SimplePathSegment::LineTo { x, y } => b.line_to([x, y])?,
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => b.cubic_to([x1, y1], [x2, y2], [x, y])?,
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                let a = b.at;
                let c1 = [
                    a[0] + 2.0 / 3.0 * (x1 - a[0]),
                    a[1] + 2.0 / 3.0 * (y1 - a[1]),
                ];
                let c2 = [x + 2.0 / 3.0 * (x1 - x), y + 2.0 / 3.0 * (y1 - y)];
                b.cubic_to(c1, c2, [x, y])?;
            }
            SimplePathSegment::ClosePath => b.close()?,
        }
    }
    Ok(())
}

/// A length attribute of a shape in user units: `%` of `base`.
fn attr(n: Node<'_, '_>, name: &str, base: f64) -> Option<f64> {
    let l = Length::from_str(n.attribute(name)?).ok()?;
    Some(match l.unit {
        LengthUnit::Percent => base * l.number / 100.0,
        _ => f64::from(super::extract::absolute(l, super::DEFAULT_FONT_SIZE)),
    })
}

/// The segments of a basic shape (SVG 2, 10.2-10.7), as browsers draw
/// them; `false` for an element that is not one.
fn shape(b: &mut Builder, n: Node<'_, '_>, (vw, vh): (f64, f64)) -> Result<bool, TextError> {
    let diag = vw.hypot(vh) / 2f64.sqrt();
    let get = |name: &str, base: f64| attr(n, name, base).unwrap_or(0.0);
    match n.tag_name().name() {
        "rect" => {
            let (x, y) = (get("x", vw), get("y", vh));
            let (w, h) = (get("width", vw), get("height", vh));
            if !(w > 0.0 && h > 0.0) {
                return Ok(true);
            }
            let (rx, ry) = match (attr(n, "rx", vw), attr(n, "ry", vh)) {
                (None, None) => (0.0, 0.0),
                (Some(r), None) | (None, Some(r)) => (r, r),
                (Some(rx), Some(ry)) => (rx, ry),
            };
            let (rx, ry) = (rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0));
            b.move_to([x + rx, y]);
            b.line_to([x + w - rx, y])?;
            if rx > 0.0 && ry > 0.0 {
                b.quarter([x + w - rx, y + ry], [x + w, y + ry])?;
            }
            b.line_to([x + w, y + h - ry])?;
            if rx > 0.0 && ry > 0.0 {
                b.quarter([x + w - rx, y + h - ry], [x + w - rx, y + h])?;
            }
            b.line_to([x + rx, y + h])?;
            if rx > 0.0 && ry > 0.0 {
                b.quarter([x + rx, y + h - ry], [x, y + h - ry])?;
            }
            b.line_to([x, y + ry])?;
            if rx > 0.0 && ry > 0.0 {
                b.quarter([x + rx, y + ry], [x + rx, y])?;
            }
            b.close()?;
        }
        "circle" => {
            let r = get("r", diag);
            if r > 0.0 {
                b.ellipse([get("cx", vw), get("cy", vh)], r, r)?;
            }
        }
        "ellipse" => {
            let (rx, ry) = match (attr(n, "rx", vw), attr(n, "ry", vh)) {
                (None, None) => (0.0, 0.0),
                (Some(r), None) | (None, Some(r)) => (r, r),
                (Some(rx), Some(ry)) => (rx, ry),
            };
            if rx > 0.0 && ry > 0.0 {
                b.ellipse([get("cx", vw), get("cy", vh)], rx, ry)?;
            }
        }
        "line" => {
            b.move_to([get("x1", vw), get("y1", vh)]);
            b.line_to([get("x2", vw), get("y2", vh)])?;
        }
        name @ ("polyline" | "polygon") => {
            let mut points = svgtypes::PointsParser::from(n.attribute("points").unwrap_or(""));
            if let Some((x, y)) = points.next() {
                b.move_to([x, y]);
                for (x, y) in points {
                    b.line_to([x, y])?;
                }
                if name == "polygon" {
                    b.close()?;
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The geometry of element `n` (a `path` or a basic shape) in the user
/// space of the text, after its own `transform`, reversed for
/// `side="right"`. `Ok(None)` for an element text cannot go along, or one
/// without length; `budget` is the segments the SVG's paths may still
/// have.
pub(crate) fn geometry(
    n: Node<'_, '_>,
    transform: Option<&str>,
    reverse: bool,
    viewport: (f32, f32),
    budget: &mut usize,
) -> Result<Option<TextPathGeom>, TextError> {
    let ts = match transform {
        Some(t) => match svgtypes::Transform::from_str(t) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        },
        None => svgtypes::Transform::default(),
    };
    let mut b = Builder {
        segments: Vec::new(),
        start: [0.0; 2],
        at: [0.0; 2],
        ts,
        budget: *budget,
    };
    let viewport = (f64::from(viewport.0), f64::from(viewport.1));
    let known = if n.tag_name().name() == "path" {
        path_data(&mut b, n.attribute("d").unwrap_or(""))?;
        true
    } else {
        shape(&mut b, n, viewport)?
    };
    *budget = b.budget;
    Ok(known
        .then(|| TextPathGeom::new(b.segments, reverse))
        .flatten())
}

/// The geometry of path data `d`, for a `textPath` that has its own
/// (`path`, SVG 2).
pub(crate) fn data_geometry(
    d: &str,
    reverse: bool,
    budget: &mut usize,
) -> Result<Option<TextPathGeom>, TextError> {
    let mut b = Builder {
        segments: Vec::new(),
        start: [0.0; 2],
        at: [0.0; 2],
        ts: svgtypes::Transform::default(),
        budget: *budget,
    };
    path_data(&mut b, d)?;
    *budget = b.budget;
    Ok(TextPathGeom::new(b.segments, reverse))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(svg_body: &str, reverse: bool) -> Option<TextPathGeom> {
        let src = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{svg_body}</svg>"#);
        let doc = usvg::roxmltree::Document::parse(&src).unwrap();
        let n = doc.root_element().first_element_child().unwrap();
        let mut budget = MAX_TEXT_PATH_SEGMENTS;
        geometry(
            n,
            n.attribute("transform"),
            reverse,
            (100.0, 100.0),
            &mut budget,
        )
        .unwrap()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn measures_lines_curves_and_arcs() {
        let g = geom(r#"<path d="M0 0H30V40"/>"#, false).unwrap();
        assert!(close(g.length(), 70.0));
        let p = g.at(45.0).unwrap();
        assert!(
            close(p.x, 30.0) && close(p.y, 15.0) && close(p.angle, 90.0),
            "{p:?}"
        );
        // A quadratic curve: the parabola y = x² / 20 from 0 to 20.
        let g = geom(r#"<path d="M0 0Q10 0 20 20"/>"#, false).unwrap();
        let exact = {
            let f = |x: f64| x / 2.0 * (1.0 + x * x).sqrt() + (x + (1.0 + x * x).sqrt()).ln() / 2.0;
            (10.0 * f(2.0)) as f32
        };
        assert!(close(g.length(), exact), "{} {exact}", g.length());
        // A circle, from its rightmost point, clockwise (down first).
        let g = geom(r#"<circle cx="50" cy="50" r="10"/>"#, false).unwrap();
        assert!(
            (g.length() - 20.0 * std::f32::consts::PI).abs() < 0.01,
            "{}",
            g.length()
        );
        let p = g.at(0.0).unwrap();
        assert!(
            close(p.x, 60.0) && close(p.y, 50.0) && close(p.angle, 90.0),
            "{p:?}"
        );
        // An arc in path data becomes curves.
        let g = geom(r#"<path d="M0 0A10 10 0 0 1 20 0"/>"#, false).unwrap();
        assert!(
            (g.length() - 10.0 * std::f32::consts::PI).abs() < 0.01,
            "{}",
            g.length()
        );
    }

    #[test]
    fn reads_shapes_transforms_and_sides() {
        let g = geom(r#"<rect x="10" y="10" width="20" height="10"/>"#, false).unwrap();
        assert!(close(g.length(), 60.0));
        let g = geom(r#"<polyline points="0 0 10 0 10 10"/>"#, false).unwrap();
        assert!(close(g.length(), 20.0));
        let g = geom(r#"<polygon points="0 0 10 0 10 10"/>"#, false).unwrap();
        assert!(close(g.length(), 20.0 + 200f32.sqrt()));
        let g = geom(
            r#"<line x1="0" y1="0" x2="0" y2="10" transform="scale(2) translate(5 0)"/>"#,
            false,
        )
        .unwrap();
        let p = g.at(20.0).unwrap();
        assert!(
            close(g.length(), 20.0) && close(p.x, 10.0) && close(p.y, 20.0),
            "{p:?}"
        );
        // `side="right"` goes the other way.
        let g = geom(r#"<path d="M0 0H30"/>"#, true).unwrap();
        let p = g.at(0.0).unwrap();
        assert!(close(p.x, 30.0) && close(p.angle.abs(), 180.0), "{p:?}");
        // Off the path: before it, after it, and between subpaths nothing
        // is counted.
        let g = geom(r#"<path d="M0 0H10M50 50H60"/>"#, false).unwrap();
        assert!(close(g.length(), 20.0));
        assert!(g.at(-0.1).is_none() && g.at(20.1).is_none());
        assert!(close(g.at(15.0).unwrap().x, 55.0));
    }

    #[test]
    fn has_no_geometry_without_length_or_a_shape() {
        assert!(geom(r#"<path d="M0 0"/>"#, false).is_none());
        assert!(geom(r#"<path d="M0 0L0 0"/>"#, false).is_none());
        assert!(geom("<g/>", false).is_none());
        assert!(geom(
            r#"<path d="M0 0H10" transform="scale(1e308) scale(1e308)"/>"#,
            false
        )
        .is_none());
        assert!(geom(r#"<rect width="0" height="10"/>"#, false).is_none());
        // Finite, but past what f32 holds: its length, or a point.
        assert!(geom(r#"<path d="M0 0H10" transform="scale(1e100)"/>"#, false).is_none());
        assert!(geom(r#"<path d="M1e100 0h10"/>"#, false).is_none());
    }

    #[test]
    fn stops_past_the_most_segments() {
        let d = format!("M0 0{}", "h1".repeat(MAX_TEXT_PATH_SEGMENTS + 1));
        let mut budget = MAX_TEXT_PATH_SEGMENTS;
        assert!(matches!(
            data_geometry(&d, false, &mut budget),
            Err(TextError::Limit(_))
        ));
        let mut budget = MAX_TEXT_PATH_SEGMENTS;
        let d = format!("M0 0{}", "h1".repeat(MAX_TEXT_PATH_SEGMENTS));
        assert!(data_geometry(&d, false, &mut budget).unwrap().is_some());
        assert_eq!(budget, 0);
    }
}
