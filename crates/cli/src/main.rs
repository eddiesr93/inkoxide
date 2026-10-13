//! `inkoxide` — development and CI front end for the engine.
//!
//! ```text
//! inkoxide render doc.json --fonts fixtures/fonts -o out.pdf [--msgpack] [--png-dir out/]
//!   [--hyphenation en=hyph-en-us.tex]
//! inkoxide validate doc.json --fonts fixtures/fonts
//! inkoxide bench doc.json --fonts fixtures/fonts --iterations 30
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use inkoxide::{
    FontStore, HyphenationStore, ImageStore, InkoxideError, InputFormat, PdfALevel, RenderOptions,
    ValidationError,
};

/// `--pdf-a=LEVEL`.
fn pdf_a_level(s: &str) -> Result<PdfALevel, String> {
    PdfALevel::from_name(s)
        .ok_or_else(|| format!("unsupported PDF/A level `{s}` (expected 1b, 2b or 3b)"))
}

#[derive(Parser)]
#[command(
    name = "inkoxide",
    version,
    about = "Render Inkoxide document trees to PDF"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

/// The fonts and images a document uses.
#[derive(Args)]
struct Resources {
    /// Directory of .ttf/.otf/.woff/.woff2 files; each is registered under
    /// its file stem (`Inter-Regular.ttf` → `Inter-Regular`).
    #[arg(long)]
    fonts: Vec<PathBuf>,
    /// Directory of images; each is available to `src` under its file stem.
    #[arg(long)]
    images: Vec<PathBuf>,
}

/// How to read the document.
#[derive(Args)]
struct Input {
    /// Read the document as `MessagePack` (implied by a .msgpack/.mpk extension).
    #[arg(long)]
    msgpack: bool,
}

/// Render options that `validate` has no use for.
#[derive(Args)]
struct Output {
    /// Embed whole font programs instead of subsets.
    #[arg(long)]
    full_fonts: bool,
    /// TeX hyphenation patterns for a language, as LANG=PATH
    /// (`--hyphenation en=hyph-en-us.tex`); repeatable. They win over the
    /// built-in patterns.
    #[arg(long, value_name = "LANG=PATH")]
    hyphenation: Vec<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render a document to PDF.
    Render {
        doc: PathBuf,
        #[command(flatten)]
        res: Resources,
        #[command(flatten)]
        input: Input,
        #[command(flatten)]
        out: Output,
        /// Output file (default: the input path with a .pdf extension).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write uncompressed content streams (for inspection / snapshots).
        #[arg(long)]
        no_compress: bool,
        /// RFC 3339 creation date to embed.
        #[arg(long)]
        creation_date: Option<String>,
        /// Write PDF/A at LEVEL: 1b, 2b (the default) or 3b.
        #[arg(long, value_name = "LEVEL", num_args = 0..=1, default_missing_value = "2b", value_parser = pdf_a_level)]
        pdf_a: Option<PdfALevel>,
        /// Also rasterise every page to PNG with `pdftoppm` (if installed).
        #[arg(long)]
        png_dir: Option<PathBuf>,
        /// Resolution for --png-dir.
        #[arg(long, default_value_t = 72)]
        dpi: u32,
        /// Worker threads (default: all cores; 0 and 1 render on the calling
        /// thread; a pool has at most 256).
        #[arg(long)]
        threads: Option<usize>,
    },
    /// Validate a document; prints the problems found (at most 100).
    Validate {
        doc: PathBuf,
        #[command(flatten)]
        res: Resources,
        #[command(flatten)]
        input: Input,
        /// Print errors as a JSON array.
        #[arg(long)]
        json: bool,
    },
    /// Render repeatedly and report timings.
    Bench {
        doc: PathBuf,
        #[command(flatten)]
        res: Resources,
        #[command(flatten)]
        input: Input,
        #[command(flatten)]
        out: Output,
        #[arg(long, default_value_t = 30)]
        iterations: u32,
        /// Worker threads (default: all cores; 0 and 1 render on the calling
        /// thread; a pool has at most 256).
        #[arg(long)]
        threads: Option<usize>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(2)
        }
    }
}

fn load_resources(res: &Resources) -> Result<(FontStore, ImageStore), String> {
    let mut fonts = FontStore::new();
    for dir in &res.fonts {
        for (stem, path) in dir_files(dir)? {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(ext.as_str(), "ttf" | "otf" | "woff" | "woff2") {
                continue;
            }
            let bytes = read(&path)?;
            fonts
                .register(stem, bytes)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    let mut images = ImageStore::new();
    for dir in &res.images {
        for (stem, path) in dir_files(dir)? {
            images.insert(stem, read(&path)?);
        }
    }
    Ok((fonts, images))
}

fn load_hyphenation(out: &Output) -> Result<HyphenationStore, String> {
    let mut store = HyphenationStore::new();
    for spec in &out.hyphenation {
        let (lang, path) = spec
            .split_once('=')
            .ok_or_else(|| format!("--hyphenation `{spec}`: expected LANG=PATH"))?;
        let bytes = read(Path::new(path))?;
        store
            .register(lang, &bytes)
            .map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(store)
}

/// Regular files of `dir` as (stem, path), sorted for determinism.
fn dir_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_file() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.push((stem.to_string(), path));
            }
        }
    }
    out.sort();
    Ok(out)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn print_errors(errors: &[ValidationError], json: bool) {
    if json {
        let arr: Vec<_> = errors
            .iter()
            .map(|e| serde_json::json!({ "code": e.code.as_str(), "path": e.path, "message": e.message }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr).unwrap_or_default());
    } else {
        for e in errors {
            eprintln!("{e}");
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Cmd::Validate {
            doc,
            res,
            input,
            json,
        } => {
            let (fonts, images) = load_resources(&res)?;
            let errors = inkoxide::validate(&read(&doc)?, format_of(&doc, &input), &fonts, &images);
            print_errors(&errors, json);
            if errors.is_empty() {
                if !json {
                    eprintln!("{}: ok", doc.display());
                }
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        Cmd::Render {
            doc,
            res,
            input,
            out,
            output,
            no_compress,
            creation_date,
            pdf_a,
            png_dir,
            dpi,
            threads,
        } => {
            let (fonts, images) = load_resources(&res)?;
            let bytes = read(&doc)?;
            let mut opts = RenderOptions::default()
                .with_compress(!no_compress)
                .with_threads(threads)
                .with_subset_fonts(!out.full_fonts)
                .with_hyphenation(load_hyphenation(&out)?);
            if let Some(level) = pdf_a {
                opts = opts.with_pdf_a_level(level);
            }
            opts.creation_date = creation_date;
            let start = Instant::now();
            let pdf =
                match inkoxide::render(&bytes, format_of(&doc, &input), &opts, &fonts, &images) {
                    Ok(pdf) => pdf,
                    Err(e) => {
                        eprintln!("{}", describe(&e));
                        return Ok(ExitCode::from(1));
                    }
                };
            let elapsed = start.elapsed();
            let path = output.unwrap_or_else(|| doc.with_extension("pdf"));
            fs::write(&path, &pdf).map_err(|e| format!("{}: {e}", path.display()))?;
            eprintln!(
                "{} ({} bytes) in {:.2} ms",
                path.display(),
                pdf.len(),
                ms(elapsed)
            );
            if let Some(dir) = png_dir {
                rasterise(&path, &dir, dpi)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Bench {
            doc,
            res,
            input,
            out,
            iterations,
            threads,
        } => {
            let bytes = read(&doc)?;
            let format = format_of(&doc, &input);
            let opts = RenderOptions::default()
                .with_threads(threads)
                .with_subset_fonts(!out.full_fonts)
                .with_hyphenation(load_hyphenation(&out)?);
            // Cold start: font registration plus the first render, which
            // also fills the process-wide caches.
            let cold = Instant::now();
            let (fonts, images) = load_resources(&res)?;
            inkoxide::render(&bytes, format, &opts, &fonts, &images).map_err(|e| describe(&e))?;
            let cold = cold.elapsed();
            let mut times = Vec::with_capacity(iterations as usize);
            let mut size = 0;
            for _ in 0..iterations.max(1) {
                let start = Instant::now();
                let pdf = inkoxide::render(&bytes, format, &opts, &fonts, &images)
                    .map_err(|e| describe(&e))?;
                times.push(start.elapsed());
                size = pdf.len();
            }
            times.sort();
            let median = times[times.len() / 2];
            println!(
                "{}: cold {:.3} ms; {} iterations, min {:.3} ms, median {:.3} ms, max {:.3} ms, {} bytes",
                doc.display(),
                ms(cold),
                times.len(),
                ms(times[0]),
                ms(median),
                ms(times[times.len() - 1]),
                size
            );
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `MessagePack` with `--msgpack` or a `.msgpack` / `.mpk` extension;
/// JSON otherwise.
fn format_of(doc: &Path, input: &Input) -> InputFormat {
    match doc.extension().and_then(|e| e.to_str()) {
        _ if input.msgpack => InputFormat::MessagePack,
        Some("msgpack" | "mpk") => InputFormat::MessagePack,
        _ => InputFormat::Json,
    }
}

/// A failed render as one line, led by its code. A validation error's own
/// text already starts with the code.
fn describe(e: &InkoxideError) -> String {
    match e {
        InkoxideError::Validation(v) => v.to_string(),
        other => format!("{} {other}", other.code()),
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn rasterise(pdf: &Path, dir: &Path, dpi: u32) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem = pdf.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    let status = Command::new("pdftoppm")
        .args(["-png", "-r", &dpi.to_string()])
        .arg(pdf)
        .arg(dir.join(stem))
        .status()
        .map_err(|e| format!("could not run pdftoppm (is poppler installed?): {e}"))?;
    if !status.success() {
        return Err(format!("pdftoppm failed with {status}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_print_their_code_once() {
        let e = inkoxide::render(
            br#"{"version":1,"pages":[{"size":{"width":10,"height":10}}],"zz":1}"#,
            InputFormat::Json,
            &RenderOptions::default(),
            &FontStore::new(),
            &ImageStore::new(),
        )
        .unwrap_err();
        assert_eq!(describe(&e), "UNKNOWN_FIELD at zz: unknown field `zz`");
        let e = InkoxideError::Font("bad".into());
        assert_eq!(describe(&e), "FONT_ERROR font error: bad");
    }

    #[test]
    fn validate_refuses_render_only_flags() {
        let parse = |args: &[&str]| Cli::try_parse_from(["inkoxide"].iter().chain(args));
        assert!(parse(&["validate", "d.json", "--msgpack"]).is_ok());
        assert!(parse(&["validate", "d.json", "--full-fonts"]).is_err());
        assert!(parse(&["validate", "d.json", "--hyphenation", "en=x.tex"]).is_err());
        assert!(parse(&[
            "render",
            "d.json",
            "--full-fonts",
            "--hyphenation",
            "en=x.tex"
        ])
        .is_ok());
        assert!(parse(&["bench", "d.json", "--full-fonts", "--msgpack"]).is_ok());
    }

    #[test]
    fn pdf_a_takes_an_optional_level() {
        let level = |args: &[&str]| {
            let cli = Cli::try_parse_from(["inkoxide", "render", "d.json"].iter().chain(args))
                .map_err(|e| e.to_string())?;
            let Cmd::Render { pdf_a, .. } = cli.command else {
                unreachable!()
            };
            Ok::<_, String>(pdf_a)
        };
        assert_eq!(level(&[]), Ok(None));
        assert_eq!(level(&["--pdf-a"]), Ok(Some(PdfALevel::A2b)));
        assert_eq!(level(&["--pdf-a=1b"]), Ok(Some(PdfALevel::A1b)));
        assert_eq!(level(&["--pdf-a=3b"]), Ok(Some(PdfALevel::A3b)));
        // A bare flag before the document does not take it as its level.
        assert_eq!(
            level(&["--pdf-a", "--no-compress"]),
            Ok(Some(PdfALevel::A2b))
        );
        assert!(level(&["--pdf-a=2a"])
            .unwrap_err()
            .contains("unsupported PDF/A level"));
    }
}
