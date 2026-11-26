//! Images kept as files next to the database (`<app data>/attachments/<name>`) instead of inline in
//! note bodies. Notes point at them as `attachments/<name>` in Markdown.

use serde::{Deserialize, Serialize};
use specta::Type;

/// An image Cilica saved into its attachments folder, ready to be written into a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SavedImage {
    /// Where the note points: `attachments/<32 hex>.<ext>`, relative to the app data folder.
    /// The same bytes always give the same path.
    pub path: String,
    /// Size in pixels as shown (after any EXIF rotation and any shrinking).
    pub width: u32,
    pub height: u32,
    /// A suggested alt text: the original file's name without its extension, or "Image".
    pub name: String,
}

// ---------- Text in images ---------- //

/// Reads the text in an image file on this device (Apple Vision on macOS). Blocking: callers run
/// it on a blocking thread. The app passes `None` instead of a recognizer where the OS has none,
/// which is how "not supported" is said.
pub trait TextRecognizer: Send + Sync {
    /// The text found in the image at `path`, lines in reading order separated by `\n`. Empty when
    /// the image has no text. `Err` carries a short reason for the logs.
    fn recognize(&self, path: &std::path::Path) -> Result<String, String>;
}

/// Where finding text in images stands (Settings › Writing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
pub struct ImageTextStatus {
    /// This device can read text in images.
    pub supported: bool,
    /// Settings › Writing › Find text in images is on.
    pub enabled: bool,
    /// Images referenced by notes that have been read (text found, none found, or unreadable).
    pub done: u32,
    /// Images referenced by notes.
    pub total: u32,
    /// Images are being read right now.
    pub running: bool,
    /// Waiting for a recording to end before reading more.
    pub paused: bool,
}
