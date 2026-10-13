//! PDF/A support: an sRGB output intent, XMP metadata that mirrors the
//! document information dictionary, and a file identifier.

use std::fmt::Write as _;

use crate::date::Timestamp;
use crate::schema::Meta;
use crate::PdfALevel;

/// What the output's conformance asks of the writer: plain PDF, or
/// PDF/A. Every part of the writer that differs between them asks this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Profile {
    level: Option<PdfALevel>,
}

impl Profile {
    /// PDF/A at `level`; `None` is plain PDF.
    pub fn new(level: Option<PdfALevel>) -> Self {
        Self { level }
    }

    /// Writing PDF/A: XMP metadata, an output intent, a file identifier,
    /// annotations that print, no `.notdef` glyphs, no CMYK.
    pub fn is_pdf_a(self) -> bool {
        self.level.is_some()
    }

    /// The ISO 19005 part (`pdfaid:part`); 0 for plain PDF.
    pub fn part(self) -> u8 {
        match self.level {
            None => 0,
            Some(PdfALevel::A2b) => 2,
            Some(PdfALevel::A3b) => 3,
        }
    }
}

/// An ICC v2 display profile for sRGB (D50-adapted primaries, the sRGB
/// tone curve sampled at 1024 points). Built here rather than shipped as a
/// binary so its contents are plain to review.
pub(crate) fn srgb_icc() -> Vec<u8> {
    const CURVE_POINTS: usize = 1024;
    let s15 = |v: f64| ((v * 65536.0).round() as i32).to_be_bytes();
    let xyz = |x: f64, y: f64, z: f64| {
        let mut t = b"XYZ \0\0\0\0".to_vec();
        for v in [x, y, z] {
            t.extend(s15(v));
        }
        t
    };
    let mut desc = b"desc\0\0\0\0".to_vec();
    let name = b"sRGB (Inkoxide)\0";
    desc.extend((name.len() as u32).to_be_bytes());
    desc.extend(name);
    // No Unicode or ScriptCode description.
    desc.extend([0u8; 8]);
    desc.extend([0u8; 3 + 67]);
    let mut cprt = b"text\0\0\0\0".to_vec();
    cprt.extend(b"No copyright, use freely\0");
    let mut curve = b"curv\0\0\0\0".to_vec();
    curve.extend((CURVE_POINTS as u32).to_be_bytes());
    for i in 0..CURVE_POINTS {
        let v = i as f64 / (CURVE_POINTS - 1) as f64;
        let lin = if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        };
        curve.extend(((lin * 65535.0).round() as u16).to_be_bytes());
    }

    // (signature, data); the three TRC tags share one curve.
    let data: [(&[u8; 4], Vec<u8>); 7] = [
        (b"desc", desc),
        (b"cprt", cprt),
        (b"wtpt", xyz(0.9642, 1.0, 0.8249)),
        (b"rXYZ", xyz(0.4361, 0.2225, 0.0139)),
        (b"gXYZ", xyz(0.3851, 0.7169, 0.0971)),
        (b"bXYZ", xyz(0.1431, 0.0606, 0.7141)),
        (b"rTRC", curve),
    ];
    let tag_count = data.len() + 2;
    let mut offset = 128 + 4 + 12 * tag_count;
    let mut table = Vec::new();
    let mut body = Vec::new();
    let mut curve_entry = (0, 0);
    for (sig, d) in &data {
        table.extend(*sig);
        table.extend((offset as u32).to_be_bytes());
        table.extend((d.len() as u32).to_be_bytes());
        if *sig == b"rTRC" {
            curve_entry = (offset, d.len());
        }
        body.extend(d);
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        offset = 128 + 4 + 12 * tag_count + body.len();
    }
    for sig in [b"gTRC", b"bTRC"] {
        table.extend(sig);
        table.extend((curve_entry.0 as u32).to_be_bytes());
        table.extend((curve_entry.1 as u32).to_be_bytes());
    }

    let size = 128 + 4 + table.len() + body.len();
    let mut p = Vec::with_capacity(size);
    p.extend((size as u32).to_be_bytes());
    p.extend([0u8; 4]); // preferred CMM
    p.extend([2, 0x10, 0, 0]); // version 2.1
    p.extend(b"mntrRGB XYZ ");
    p.extend([0x07, 0xEA, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0]); // 2026-01-01
    p.extend(b"acsp");
    p.extend([0u8; 4 + 4 + 4 + 4 + 8 + 4]); // platform … rendering intent
    for v in [0.9642, 1.0, 0.8249] {
        p.extend(s15(v)); // PCS illuminant (D50)
    }
    p.extend([0u8; 4 + 16 + 28]); // creator, profile ID, reserved
    debug_assert_eq!(p.len(), 128);
    p.extend((tag_count as u32).to_be_bytes());
    p.extend(table);
    p.extend(body);
    p
}

/// The XMP packet for the catalog `/Metadata` stream. Every entry mirrors
/// the information dictionary, as PDF/A requires.
pub(crate) fn xmp(
    meta: &Meta,
    producer: &str,
    created: Option<Timestamp>,
    profile: Profile,
) -> Vec<u8> {
    let mut d = String::new();
    let mut prop = |name: &str, value: &str| {
        let _ = writeln!(d, "<{name}>{}</{name}>", escape(value));
    };
    prop("pdfaid:part", &profile.part().to_string());
    prop("pdfaid:conformance", "B");
    prop("dc:format", "application/pdf");
    prop("pdf:Producer", producer);
    if let Some(v) = &meta.keywords {
        prop("pdf:Keywords", v);
    }
    if let Some(v) = &meta.creator {
        prop("xmp:CreatorTool", v);
    }
    if let Some(t) = created {
        prop("xmp:CreateDate", &xmp_date(t));
    }
    let alt = |v: &str| {
        format!(
            "<rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt>",
            escape(v)
        )
    };
    if let Some(v) = &meta.title {
        let _ = writeln!(d, "<dc:title>{}</dc:title>", alt(v));
    }
    if let Some(v) = &meta.subject {
        let _ = writeln!(d, "<dc:description>{}</dc:description>", alt(v));
    }
    if let Some(v) = &meta.author {
        let _ = writeln!(
            d,
            "<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>",
            escape(v)
        );
    }
    format!(
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\" \
         xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" \
         xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\n\
         {d}</rdf:Description>\n\
         </rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>"
    )
    .into_bytes()
}

/// `2026-01-31T12:00:00+02:00`, the same instant as the info dictionary's
/// `CreationDate`.
fn xmp_date(t: Timestamp) -> String {
    let off = t.offset_minutes;
    let zone = if off == 0 {
        "Z".to_string()
    } else {
        let sign = if off < 0 { '-' } else { '+' };
        let a = off.unsigned_abs();
        format!("{sign}{:02}:{:02}", a / 60, a % 60)
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{zone}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

/// A metadata value as written to both the information dictionary and
/// XMP: control characters (which XML cannot carry, or normalises) become
/// spaces, so the two stay equal.
pub(crate) fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || c == '\u{FFFE}' || c == '\u{FFFF}' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// XML character data of a [`clean`]ed value.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in clean(s).chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icc_layout() {
        let p = srgb_icc();
        assert_eq!(
            u32::from_be_bytes(p[0..4].try_into().unwrap()) as usize,
            p.len()
        );
        assert_eq!(&p[12..24], b"mntrRGB XYZ ");
        assert_eq!(&p[36..40], b"acsp");
        let n = u32::from_be_bytes(p[128..132].try_into().unwrap()) as usize;
        assert_eq!(n, 9);
        for i in 0..n {
            let e = &p[132 + 12 * i..144 + 12 * i];
            let off = u32::from_be_bytes(e[4..8].try_into().unwrap()) as usize;
            let len = u32::from_be_bytes(e[8..12].try_into().unwrap()) as usize;
            assert!(off.is_multiple_of(4) && off + len <= p.len());
        }
    }

    #[test]
    fn xmp_escapes_and_dates() {
        let meta = Meta {
            title: Some("A & <B>".into()),
            ..Default::default()
        };
        let t = Timestamp {
            year: 2026,
            month: 3,
            day: 4,
            hour: 5,
            minute: 6,
            second: 7,
            offset_minutes: -150,
        };
        let profile = Profile::new(Some(PdfALevel::A2b));
        let x = String::from_utf8(xmp(&meta, "P", Some(t), profile)).unwrap();
        assert!(x.contains(">A &amp; &lt;B&gt;</rdf:li>"), "{x}");
        assert!(x.contains("<xmp:CreateDate>2026-03-04T05:06:07-02:30</xmp:CreateDate>"));
        assert_eq!(
            xmp_date(Timestamp {
                offset_minutes: 0,
                ..t
            }),
            "2026-03-04T05:06:07Z"
        );
        assert_eq!(escape("a\u{1}b\n&"), "a b &amp;");
    }
}
