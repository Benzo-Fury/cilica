//! Finding text in note images: a background queue that reads every image a note points at once,
//! with the OS's on-device recognizer ([`TextRecognizer`]), and records the text so search and the
//! assistant can find it (`cilica_store`'s `attachment_text` and `image_text_fts`).
//!
//! One image at a time, low priority. It sleeps until woken (an image saved, a note body written,
//! an import finished), stops while the setting is off, waits while a recording is in progress, and
//! reports where it stands as [`AppEvent::ImageTextProgress`] (at most a few times a second).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cilica_domain::{AppEvent, AppResult, Events, ImageTextStatus, TextRecognizer};
use cilica_store::Db;
use tokio::sync::Notify;

use crate::store::ImageStore;

/// Wait this long after a wake before looking, so a burst of saves while typing is one look.
const SETTLE: Duration = Duration::from_millis(1500);

/// Pause between two images, so reading a large library never hogs the machine.
const BETWEEN_IMAGES: Duration = Duration::from_millis(150);

/// Progress events are sent at most this often while images are being read.
const EMIT_EVERY: Duration = Duration::from_millis(250);

// ---------- State ---------- //

/// What the queue knows right now. Behind a mutex; never held across an await.
#[derive(Debug, Default)]
struct Progress {
    /// Settings › Writing › Find text in images.
    enabled: bool,
    /// A recording is in progress (anything but idle).
    recording: bool,
    running: bool,
    done: u32,
    total: u32,
    /// When the last event went out and what it said, to throttle and skip repeats.
    last_emit: Option<Instant>,
    last_status: Option<ImageTextStatus>,
}

/// The parts shared by every clone of the queue.
struct Inner {
    db: Db,
    store: ImageStore,
    /// `None` where the OS can't read text in images.
    recognizer: Option<Arc<dyn TextRecognizer>>,
    events: Events,
    wake: Notify,
    progress: Mutex<Progress>,
}

// ---------- Queue ---------- //

/// The background queue that reads text in note images. Cheap to clone; every clone drives the
/// same queue. The app spawns [`ImageTextQueue::run`] once and calls the setters as things change.
#[derive(Clone)]
pub struct ImageTextQueue {
    inner: Arc<Inner>,
}

impl ImageTextQueue {
    /// A queue over `db`'s notes and `store`'s files. `recognizer` is `None` where the OS has
    /// none; the queue then never reads anything and reports `supported: false`.
    pub fn new(db: Db, store: ImageStore, recognizer: Option<Arc<dyn TextRecognizer>>, events: Events, enabled: bool) -> Self {
        let progress = Mutex::new(Progress { enabled, ..Progress::default() });
        Self { inner: Arc::new(Inner { db, store, recognizer, events, wake: Notify::new(), progress }) }
    }

    // ── Control ────────────────────────────────────────────────────────

    /// Look for unread images soon (after a short settle).
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Turn reading on or off (the setting). Off stops after the current image and keeps the text
    /// already read; on looks again at once.
    pub fn set_enabled(&self, enabled: bool) {
        if self.update(|p| std::mem::replace(&mut p.enabled, enabled) != enabled) {
            self.emit(true);
            if enabled {
                self.wake();
            }
        }
    }

    /// Hold reading while a recording is in progress; carry on once it ends.
    pub fn set_recording(&self, recording: bool) {
        if self.update(|p| std::mem::replace(&mut p.recording, recording) != recording) {
            self.emit(true);
            if !recording {
                self.wake();
            }
        }
    }

    /// Where reading stands. Counts are read from the database unless a pass is keeping them.
    pub async fn status(&self) -> AppResult<ImageTextStatus> {
        if !self.update(|p| p.running) {
            let backlog = self.inner.db.image_text_backlog().await?;
            self.update(|p| {
                if !p.running {
                    p.total = backlog.total;
                    p.done = backlog.total - backlog.pending.len() as u32;
                }
            });
        }
        Ok(self.snapshot())
    }

    // ── Running ────────────────────────────────────────────────────────

    /// Read images until stopped: one pass now, then one after every wake. Never returns.
    pub async fn run(self) {
        loop {
            if let Err(error) = self.pass().await {
                tracing::warn!(%error, "could not read text in images");
                self.update(|p| p.running = false);
                self.emit(true);
            }
            self.inner.wake.notified().await;
            tokio::time::sleep(SETTLE).await;
        }
    }

    /// Read every referenced image that has no text recorded yet, one at a time, until done,
    /// turned off or a recording starts. Returns how many images were read.
    pub async fn pass(&self) -> AppResult<u32> {
        let Some(recognizer) = self.inner.recognizer.clone() else {
            return Ok(0);
        };

        // What is waiting
        let backlog = self.inner.db.image_text_backlog().await?;
        let held = self.update(|p| {
            p.total = backlog.total;
            p.done = backlog.total - backlog.pending.len() as u32;
            !p.enabled || p.recording
        });
        if held || backlog.pending.is_empty() {
            self.emit(true);
            return Ok(0);
        }
        self.update(|p| p.running = true);
        self.emit(true);

        // One image at a time, checking between images whether to stop
        let mut read = 0;
        for name in &backlog.pending {
            if self.update(|p| !p.enabled || p.recording) {
                break;
            }
            let (text, error) = self.recognize(&recognizer, name).await;
            self.inner.db.set_attachment_text(name, &text, error.as_deref()).await?;
            read += 1;
            self.update(|p| p.done = (p.done + 1).min(p.total));
            self.emit(false);
            tokio::time::sleep(BETWEEN_IMAGES).await;
        }

        // Finished or stopped
        self.update(|p| p.running = false);
        self.emit(true);
        Ok(read)
    }

    /// Read one image on a blocking thread. Returns its text, or empty text and why it failed.
    async fn recognize(&self, recognizer: &Arc<dyn TextRecognizer>, name: &str) -> (String, Option<String>) {
        let path = self.inner.store.dir().join(name);
        if !path.is_file() {
            return (String::new(), Some("The image file is missing.".into()));
        }
        let recognizer = recognizer.clone();
        match tokio::task::spawn_blocking(move || recognizer.recognize(&path)).await {
            Ok(Ok(text)) => (text, None),
            Ok(Err(reason)) => {
                tracing::debug!(name, %reason, "could not read text in an image");
                (String::new(), Some(reason))
            }
            Err(_) => (String::new(), Some("Reading the image stopped unexpectedly.".into())),
        }
    }

    // ── Reporting ──────────────────────────────────────────────────────

    /// Run `f` on the progress under its lock.
    fn update<T>(&self, f: impl FnOnce(&mut Progress) -> T) -> T {
        let mut progress = self.inner.progress.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut progress)
    }

    /// The status as it stands, without touching the database.
    fn snapshot(&self) -> ImageTextStatus {
        let supported = self.inner.recognizer.is_some();
        self.update(|p| ImageTextStatus {
            supported,
            enabled: p.enabled,
            done: p.done,
            total: p.total,
            running: p.running,
            paused: supported && p.enabled && p.recording && p.done < p.total,
        })
    }

    /// Send the status to the UI when it changed. Unless `force`d, at most every [`EMIT_EVERY`].
    fn emit(&self, force: bool) {
        let status = self.snapshot();
        let send = self.update(|p| {
            let due = force || p.last_emit.is_none_or(|t| t.elapsed() >= EMIT_EVERY);
            if !due || p.last_status == Some(status) {
                return false;
            }
            p.last_emit = Some(Instant::now());
            p.last_status = Some(status);
            true
        });
        if send {
            self.inner.events.emit(AppEvent::ImageTextProgress { status });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    use cilica_domain::{RecordingSink, SearchSource};
    use cilica_store::NewNote;

    use super::*;
    use crate::store::FOLDER;

    const A: &str = "0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210";
    const C: &str = "00000000000000000000000000000000";

    /// Says what the file holds as its text; "fail" files can't be read. Counts calls.
    #[derive(Default)]
    struct FakeRecognizer {
        calls: AtomicU32,
    }

    impl TextRecognizer for FakeRecognizer {
        fn recognize(&self, path: &Path) -> Result<String, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            if text == "fail" { Err("unreadable".into()) } else { Ok(text) }
        }
    }

    /// A database, a folder with three "images" and a note pointing at them.
    async fn setup() -> (tempfile::TempDir, Db, ImageStore) {
        let temp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(temp.path().join(FOLDER));
        store.ensure_dir().unwrap();
        std::fs::write(store.dir().join(format!("{A}.png")), "Invoice 2041\nDue Friday").unwrap();
        std::fs::write(store.dir().join(format!("{B}.jpg")), "fail").unwrap();
        let db = Db::open_in_memory().await.unwrap();
        let body = format!("![a](attachments/{A}.png)\n![b](attachments/{B}.jpg)\n![c](attachments/{C}.png)\n![a again](attachments/{A}.png)");
        db.create_note(NewNote { title: "Bills".into(), body_md: body, ..Default::default() }).await.unwrap();
        (temp, db, store)
    }

    /// Every referenced image is read once, failures and missing files included, and the text is
    /// searchable afterwards.
    #[tokio::test]
    async fn reads_each_image_once() {
        let (_temp, db, store) = setup().await;
        let fake = Arc::new(FakeRecognizer::default());
        let sink = Arc::new(RecordingSink::default());
        let queue = ImageTextQueue::new(db.clone(), store, Some(fake.clone()), sink.clone(), true);

        // Three distinct images: one read, one unreadable, one missing (not passed to the recognizer)
        assert_eq!(queue.status().await.unwrap(), ImageTextStatus { supported: true, enabled: true, done: 0, total: 3, running: false, paused: false });
        assert_eq!(queue.pass().await.unwrap(), 3);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
        assert_eq!(queue.status().await.unwrap(), ImageTextStatus { supported: true, enabled: true, done: 3, total: 3, running: false, paused: false });

        // A second pass has nothing to do
        assert_eq!(queue.pass().await.unwrap(), 0);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 2);

        // Search finds the text
        let hits = db.search_notes("invoice", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, SearchSource::Image);
        assert_eq!(hits[0].image.as_deref(), Some(format!("attachments/{A}.png").as_str()));

        // The UI heard it start and finish
        let events = sink.0.lock().unwrap();
        let statuses: Vec<ImageTextStatus> = events.iter().filter_map(|e| if let AppEvent::ImageTextProgress { status } = e { Some(*status) } else { None }).collect();
        assert!(statuses.first().is_some_and(|s| s.running && s.done == 0));
        assert!(statuses.last().is_some_and(|s| !s.running && s.done == 3));
    }

    /// Nothing is read while the setting is off or a recording is in progress.
    #[tokio::test]
    async fn waits_while_off_or_recording() {
        let (_temp, db, store) = setup().await;
        let fake = Arc::new(FakeRecognizer::default());
        let queue = ImageTextQueue::new(db, store, Some(fake.clone()), Arc::new(RecordingSink::default()), false);

        // Off
        assert_eq!(queue.pass().await.unwrap(), 0);
        assert!(!queue.status().await.unwrap().enabled);

        // On, but recording: paused
        queue.set_enabled(true);
        queue.set_recording(true);
        assert_eq!(queue.pass().await.unwrap(), 0);
        let status = queue.status().await.unwrap();
        assert!(status.paused && !status.running && status.done == 0);

        // Recording over: reads everything
        queue.set_recording(false);
        assert_eq!(queue.pass().await.unwrap(), 3);
        assert!(!queue.status().await.unwrap().paused);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    }

    /// Without a recognizer the queue says it isn't supported and reads nothing.
    #[tokio::test]
    async fn unsupported_reads_nothing() {
        let (_temp, db, store) = setup().await;
        let queue = ImageTextQueue::new(db, store, None, Arc::new(RecordingSink::default()), true);
        assert_eq!(queue.pass().await.unwrap(), 0);
        let status = queue.status().await.unwrap();
        assert!(!status.supported && status.total == 3 && status.done == 0);
    }
}
