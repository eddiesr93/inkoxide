//! Reproduces a fuzz finding: `repro tree <input>` regenerates the
//! document a `render_tree` input encodes; `repro json <input>` uses the
//! bytes as is. Prints the document to stdout, then runs the oracle.
//! `repro image <input>` runs the `render_svg` oracle on an image, and
//! `repro font <input>` the `register_font` oracle on a font.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, mode, path] = &args[..] else {
        eprintln!("usage: repro tree|json|image|svg|font <input>");
        std::process::exit(2);
    };
    if mode == "stats" {
        return stats(path);
    }
    let bytes = std::fs::read(path).expect("read input");
    if mode == "image" {
        let rendered = inkoxide_fuzz::check_image(&bytes);
        return eprintln!("ok ({})", if rendered { "rendered" } else { "rejected" });
    }
    if mode == "svg" {
        let svg = inkoxide_fuzz::svg::svg(&bytes);
        println!("{}", String::from_utf8_lossy(&svg));
        let t = std::time::Instant::now();
        let rendered = inkoxide_fuzz::check_image(&svg);
        let mut images = inkoxide::ImageStore::new();
        images.insert("fz", svg.clone());
        let doc = br#"{"version":1,"fonts":[],"pages":[{"size":{"width":300,"height":400},"children":[{"type":"image","src":"fz"}]}]}"#;
        let why = inkoxide::validate(
            doc,
            inkoxide::InputFormat::Json,
            &inkoxide::FontStore::new(),
            &images,
        );
        let verdict = if rendered {
            "rendered".to_string()
        } else {
            format!("rejected: {:?}", why.first().map(|e| &e.message))
        };
        return eprintln!("ok ({verdict} in {:?})", t.elapsed());
    }
    if mode == "font" {
        let registered = inkoxide_fuzz::check_font(&bytes);
        return eprintln!("ok ({registered} of the variants registered)");
    }
    let doc = match mode.as_str() {
        "tree" => inkoxide_fuzz::document(&bytes),
        _ => bytes,
    };
    println!("{}", String::from_utf8_lossy(&doc));
    inkoxide_fuzz::check(&doc);
    eprintln!("ok");
}

/// `repro stats <dir>`: how many `render_tree` inputs in a corpus make a
/// valid document, and the most common validation errors otherwise.
fn stats(dir: &str) {
    let mut valid = 0;
    let mut codes = std::collections::BTreeMap::<String, usize>::new();
    let entries: Vec<_> = std::fs::read_dir(dir)
        .expect("read dir")
        .flatten()
        .collect();
    for e in &entries {
        let doc = inkoxide_fuzz::document(&std::fs::read(e.path()).expect("read input"));
        let errors = inkoxide::validate(
            &doc,
            inkoxide::InputFormat::Json,
            &inkoxide_fuzz::FONTS,
            &inkoxide_fuzz::IMAGES,
        );
        match errors.first() {
            None => valid += 1,
            Some(e) => {
                *codes
                    .entry(format!("{} {}", e.code.as_str(), e.message))
                    .or_default() += 1;
            }
        }
    }
    println!("{valid}/{} valid", entries.len());
    let mut top: Vec<_> = codes.into_iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (c, n) in top.into_iter().take(15) {
        println!("{n:5}  {c}");
    }
}
