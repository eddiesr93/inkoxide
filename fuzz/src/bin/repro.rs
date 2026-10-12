//! Reproduces a fuzz finding:
//!
//! - `repro tree <input>` regenerates the document a `render_tree` input
//!   encodes, prints it to stdout and runs the oracle on it;
//! - `repro json <input>` does the same with the bytes as they are;
//! - `repro image <input>` runs the `render_svg` oracle on an image;
//! - `repro svg <input>` regenerates the SVG a `render_svg_tree` input
//!   encodes, prints it, and runs the oracle with its time and verdict;
//! - `repro font <input>` runs the `register_font` oracle on a font;
//! - `repro stats <dir>` reports how many inputs of a `render_tree`
//!   corpus make a valid document.

const USAGE: &str = "usage: repro tree|json|image|svg|font <input>\n       repro stats <dir>";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, mode, path] = &args[..] else {
        usage();
    };
    let read = || std::fs::read(path).expect("read input");
    match mode.as_str() {
        "tree" | "json" => {
            let doc = if mode == "tree" {
                inkoxide_fuzz::document(&read())
            } else {
                read()
            };
            println!("{}", String::from_utf8_lossy(&doc));
            inkoxide_fuzz::check(&doc);
            eprintln!("ok");
        }
        "image" => eprintln!("ok ({})", verdict(&read())),
        "svg" => {
            let svg = inkoxide_fuzz::svg::svg(&read());
            println!("{}", String::from_utf8_lossy(&svg));
            let t = std::time::Instant::now();
            let verdict = verdict(&svg);
            eprintln!("ok ({verdict} in {:?})", t.elapsed());
        }
        "font" => {
            let registered = inkoxide_fuzz::check_font(&read());
            eprintln!("ok ({registered} of the variants registered)");
        }
        "stats" => stats(path),
        _ => usage(),
    }
}

fn usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2);
}

/// Runs the image oracle: "rendered", or why the engine rejected it.
fn verdict(image: &[u8]) -> String {
    match inkoxide_fuzz::check_image(image) {
        Ok(()) => "rendered".into(),
        Err(e) => format!("rejected: {e}"),
    }
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
