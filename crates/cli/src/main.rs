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
    FontStore, HyphenationStore, ImageStore, InputFormat, RenderOptions, ValidationError,
};

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

#[derive(Args)]
struct Resources {
    /// Directory of .ttf/.otf/.woff/.woff2 files; each is registered under
    /// its file stem (`Inter-Regular.ttf` → `Inter-Regular`).
    #[arg(long)]
    fonts: Vec<PathBuf>,
    /// Directory of images; each is available to `src` under its file stem.
    #[arg(long)]
    images: Vec<PathBuf>,
    /// Embed whole font programs instead of subsets.
    #[arg(long)]
    full_fonts: bool,
    /// Read the document as `MessagePack` (implied by a .msgpack/.mpk extension).
    #[arg(long)]
    msgpack: bool,
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
        /// Output file (default: the input path with a .pdf extension).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write uncompressed content streams (for inspection / snapshots).
        #[arg(long)]
        no_compress: bool,
        /// RFC 3339 creation date to embed.
        #[arg(long)]
        creation_date: Option<String>,
        /// Write PDF/A-2b.
        #[arg(long)]
        pdf_a: bool,
        /// Also rasterise every page to PNG with `pdftoppm` (if installed).
        #[arg(long)]
        png_dir: Option<PathBuf>,
        /// Resolution for --png-dir.
        #[arg(long, default_value_t = 72)]
        dpi: u32,
        /// Worker threads (default: all cores).
        #[arg(long)]
        threads: Option<usize>,
    },
    /// Validate a document; prints the problems found (at most 100).
    Validate {
        doc: PathBuf,
        #[command(flatten)]
        res: Resources,
        /// Print errors as a JSON array.
        #[arg(long)]
        json: bool,
    },
    /// Render repeatedly and report timings.
    Bench {
        doc: PathBuf,
        #[command(flatten)]
        res: Resources,
        #[arg(long, default_value_t = 30)]
        iterations: u32,
        /// Worker threads (default: all cores).
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
            let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            fonts
                .register(stem, bytes)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    let mut images = ImageStore::new();
    for dir in &res.images {
        for (stem, path) in dir_files(dir)? {
            let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            images.insert(stem, bytes);
        }
    }
    Ok((fonts, images))
}

fn load_hyphenation(res: &Resources) -> Result<HyphenationStore, String> {
    let mut store = HyphenationStore::new();
    for spec in &res.hyphenation {
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
        Cmd::Validate { doc, res, json } => {
            let (fonts, images) = load_resources(&res)?;
            let errors = inkoxide::validate(&read(&doc)?, format_of(&doc, &res), &fonts, &images);
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
            output,
            no_compress,
            creation_date,
            pdf_a,
            png_dir,
            dpi,
            threads,
        } => {
            let (fonts, images) = load_resources(&res)?;
            let input = read(&doc)?;
            let mut opts = RenderOptions::default()
                .with_compress(!no_compress)
                .with_threads(threads)
                .with_subset_fonts(!res.full_fonts)
                .with_pdf_a(pdf_a)
                .with_hyphenation(load_hyphenation(&res)?);
            opts.creation_date = creation_date;
            let start = Instant::now();
            let pdf = match inkoxide::render(&input, format_of(&doc, &res), &opts, &fonts, &images)
            {
                Ok(pdf) => pdf,
                Err(e) => {
                    eprintln!("{} {e}", e.code());
                    return Ok(ExitCode::from(1));
                }
            };
            let elapsed = start.elapsed();
            let out = output.unwrap_or_else(|| doc.with_extension("pdf"));
            fs::write(&out, &pdf).map_err(|e| format!("{}: {e}", out.display()))?;
            eprintln!(
                "{} ({} bytes) in {:.2} ms",
                out.display(),
                pdf.len(),
                ms(elapsed)
            );
            if let Some(dir) = png_dir {
                rasterise(&out, &dir, dpi)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Bench {
            doc,
            res,
            iterations,
            threads,
        } => {
            let input = read(&doc)?;
            let opts = RenderOptions::default()
                .with_threads(threads)
                .with_subset_fonts(!res.full_fonts)
                .with_hyphenation(load_hyphenation(&res)?);
            // Cold start: font registration plus the first render, which
            // also fills the process-wide caches.
            let cold = Instant::now();
            let (fonts, images) = load_resources(&res)?;
            inkoxide::render(&input, format_of(&doc, &res), &opts, &fonts, &images)
                .map_err(|e| format!("{} {e}", e.code()))?;
            let cold = cold.elapsed();
            let mut times = Vec::with_capacity(iterations as usize);
            let mut size = 0;
            for _ in 0..iterations.max(1) {
                let start = Instant::now();
                let pdf = inkoxide::render(&input, format_of(&doc, &res), &opts, &fonts, &images)
                    .map_err(|e| format!("{} {e}", e.code()))?;
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
fn format_of(doc: &Path, res: &Resources) -> InputFormat {
    match doc.extension().and_then(|e| e.to_str()) {
        _ if res.msgpack => InputFormat::MessagePack,
        Some("msgpack" | "mpk") => InputFormat::MessagePack,
        _ => InputFormat::Json,
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
