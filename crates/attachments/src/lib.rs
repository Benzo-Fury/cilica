//! Image attachments for Cilica: images live as files in `<app data>/attachments/` and notes point
//! at them as `attachments/<name>` in Markdown, instead of carrying `data:` URIs in their bodies.
//!
//! - [`ImageStore`] owns the folder: it saves images (sniffed from their bytes, checked by decoding,
//!   converted or shrunk as needed, named by content hash), downloads them from the web safely
//!   ([`fetch`]), resolves note paths and removes files nothing uses.
//! - [`markdown`] finds references and rewrites inline images in a text.
//! - [`NoteImages`] does both across the whole database.
//! - [`ImageTextQueue`] reads the text in note images in the background, for search.
//!
//! No tauri here; the app wires these to commands.

pub mod fetch;
pub mod format;
mod image_text;
mod library;
pub mod markdown;
mod store;

pub use fetch::{FetchLimits, WebFetcher, is_public};
pub use format::{HeicConverter, MODEL_MAX_SIDE, ModelImage, StoredFormat};
pub use image_text::ImageTextQueue;
pub use library::{ExternalizeReport, NoteImages};
pub use markdown::{Externalized, NoteImageRef, externalize_inline_images, note_images, referenced_names};
pub use store::{AttachmentName, FOLDER, ImageStore, SaveOptions};
