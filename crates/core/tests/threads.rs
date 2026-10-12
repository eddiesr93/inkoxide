//! `threads: 1` renders on the calling thread: concurrent one-thread
//! renders run side by side, each on its own thread, instead of queueing
//! for one shared worker.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

use inkoxide::{FontStore, ImageStore, InputFormat, RenderOptions};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

const RENDERS: usize = 4;

const DOC: &str = r#"{"version":1,"fonts":[{"id":"Inter-Regular","family":"Inter"}],
    "pages":[{"size":{"width":595,"height":842},"children":[
      {"type":"text","runs":[{"text":"Hello","font":"Inter-Regular","size":12}]}]}]}"#;

/// Counts renders that have reached layout; each waits there for the others.
#[derive(Default)]
struct Gate {
    arrived: Mutex<usize>,
    all: Condvar,
}

impl Gate {
    /// Arrives, then waits for every render to arrive; false after 10 s
    /// without them.
    fn arrive(&self) -> bool {
        let mut n = self.arrived.lock().unwrap();
        *n += 1;
        self.all.notify_all();
        let (n, wait) = self
            .all
            .wait_timeout_while(n, Duration::from_secs(10), |n| *n < RENDERS)
            .unwrap();
        drop(n);
        !wait.timed_out()
    }
}

/// A subscriber for one thread: records the thread that lays out each page
/// and whether every render was in layout at the same time. Spans entered
/// on other threads (a pool's workers) never reach it.
struct Recorder {
    gate: Arc<Gate>,
    seen: Arc<Mutex<Vec<(ThreadId, bool)>>>,
    next: AtomicU64,
}

impl Subscriber for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, span: &Attributes<'_>) -> Id {
        if span.metadata().name() == "layout+paginate" {
            let together = self.gate.arrive();
            self.seen
                .lock()
                .unwrap()
                .push((thread::current().id(), together));
        }
        Id::from_u64(self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, _: &Event<'_>) {}
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

#[test]
fn one_thread_renders_run_concurrently_on_their_calling_threads() {
    let mut fonts = FontStore::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fonts/Inter-Regular.ttf"
    ))
    .unwrap();
    fonts.register("Inter-Regular", bytes).unwrap();
    let options = RenderOptions::default().with_threads(1);
    let gate = Arc::new(Gate::default());

    thread::scope(|s| {
        let workers: Vec<_> = (0..RENDERS)
            .map(|_| {
                let (fonts, options, gate) = (&fonts, &options, gate.clone());
                s.spawn(move || {
                    let seen = Arc::new(Mutex::new(Vec::new()));
                    let recorder = Recorder {
                        gate,
                        seen: seen.clone(),
                        next: AtomicU64::new(0),
                    };
                    let pdf = tracing::subscriber::with_default(recorder, || {
                        inkoxide::render(
                            DOC.as_bytes(),
                            InputFormat::Json,
                            options,
                            fonts,
                            &ImageStore::new(),
                        )
                    });
                    assert!(pdf.unwrap().starts_with(b"%PDF-"));
                    let seen = seen.lock().unwrap().clone();
                    (thread::current().id(), seen)
                })
            })
            .collect();
        for w in workers {
            let (caller, seen) = w.join().unwrap();
            // One page, laid out on the calling thread while every other
            // render was laying out its own.
            assert_eq!(seen, [(caller, true)]);
        }
    });
}
