//! Warnings are `tracing` events: a render reports a problem that is not an
//! error to whoever installed a subscriber. One-thread renders run on the
//! calling thread, so a subscriber for this thread sees all of them.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

/// Keeps the message of every warning.
struct Warnings {
    seen: Arc<Mutex<Vec<String>>>,
    next: AtomicU64,
}

struct Message<'a>(&'a mut String);

impl Visit for Message<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.0 = format!("{value:?}");
        }
    }
}

impl Subscriber for Warnings {
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        *meta.level() <= Level::WARN
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        if *event.metadata().level() == Level::WARN {
            let mut message = String::new();
            event.record(&mut Message(&mut message));
            self.seen.lock().unwrap().push(message);
        }
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

/// The warnings of rendering `doc`.
fn warnings(doc: &str) -> Vec<String> {
    let mut fonts = FontStore::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    fonts.register("Inter-Regular", bytes).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let subscriber = Warnings {
        seen: seen.clone(),
        next: AtomicU64::new(0),
    };
    let options = RenderOptions::default().with_threads(1);
    let pdf = tracing::subscriber::with_default(subscriber, || {
        inkoxide::render(
            doc.as_bytes(),
            InputFormat::Json,
            &options,
            &fonts,
            &ImageStore::new(),
        )
    });
    assert!(pdf.unwrap().starts_with(b"%PDF-"));
    let seen = seen.lock().unwrap().clone();
    seen
}

/// A document with one paragraph, with `meta` and the `extra` style and
/// run properties.
fn doc(meta: &str, style: &str, run: &str) -> String {
    format!(
        r#"{{"version":1,"meta":{{{meta}}},"fonts":[{{"id":"Inter-Regular","family":"Inter"}}],
        "pages":[{{"size":{{"width":595,"height":842}},"children":[
          {{"type":"text","style":{{{style}}},"runs":[
            {{"text":"documentation","font":"Inter-Regular","size":12{run}}}]}}]}}]}}"#
    )
}

fn hyphenation_warnings(doc: &str) -> usize {
    warnings(doc)
        .iter()
        .filter(|w| w.contains("`hyphenate` is on but the text has no language"))
        .count()
}

#[test]
fn hyphenate_without_a_language_warns() {
    let d = doc("", r#""hyphenate":true"#, "");
    assert_eq!(hyphenation_warnings(&d), 1);
}

#[test]
fn a_run_that_hyphenates_without_a_language_warns() {
    let d = doc("", "", r#","hyphenate":true"#);
    assert_eq!(hyphenation_warnings(&d), 1);
}

#[test]
fn hyphenate_with_a_language_does_not_warn() {
    let on_node = doc("", r#""hyphenate":true,"lang":"en""#, "");
    assert_eq!(hyphenation_warnings(&on_node), 0);
    let in_meta = doc(r#""lang":"en-US""#, r#""hyphenate":true"#, "");
    assert_eq!(hyphenation_warnings(&in_meta), 0);
}

#[test]
fn text_that_does_not_hyphenate_does_not_warn() {
    assert_eq!(hyphenation_warnings(&doc("", "", "")), 0);
    let off = doc("", r#""hyphenate":false"#, "");
    assert_eq!(hyphenation_warnings(&off), 0);
}

#[test]
fn an_axis_the_font_lacks_warns() {
    let d = r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter","variations":{"wght":700}}],
        "pages":[{"size":{"width":595,"height":842},"children":[
          {"type":"text","runs":[{"text":"x","font":"Inter-Regular","size":12}]}]}]}"#;
    let seen = warnings(d);
    assert_eq!(
        seen,
        ["font `Inter-Regular`: `Inter-Regular` has no `wght` axis; the value is ignored"]
    );
}
