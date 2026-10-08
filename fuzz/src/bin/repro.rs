//! Reproduces a fuzz finding: `repro tree <input>` regenerates the
//! document a `render_tree` input encodes; `repro json <input>` uses the
//! bytes as is. Prints the document to stdout, then runs the oracle.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, mode, path] = &args[..] else {
        eprintln!("usage: repro tree|json <input>");
        std::process::exit(2);
    };
    if mode == "stats" {
        return stats(path);
    }
    let bytes = std::fs::read(path).expect("read input");
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
                    .or_default() += 1
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
