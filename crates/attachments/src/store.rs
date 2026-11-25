//! The attachments folder: saving images into it by content hash, resolving the paths notes use,
//! copying one out for Save as…, and removing files nothing points at any more.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use cilica_domain::{AppError, AppResult, SavedImage};
use sha2::{Digest, Sha256};

use crate::fetch::WebFetcher;
use crate::format::{HeicConverter, MAX_INPUT_BYTES, Preparer, StoredFormat};

/// The folder name under the app data folder, and the prefix of every path a note uses.
pub const FOLDER: &str = "attachments";

/// Images this small (both sides) from the web are tracking pixels, not pictures.
const TRACKING_PIXEL_SIDE: u32 = 2;

/// Base64 that accepts padded and unpadded input alike.
const BASE64: GeneralPurpose =
    GeneralPurpose::new(&base64::alphabet::STANDARD, GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent));

/// Counter that keeps temporary file names unique within this process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------- Names ---------- //

/// A valid attachment file name: 32 lowercase hex characters (the start of the SHA-256 of the
/// stored bytes), a dot and a stored format's extension. Nothing else is ever opened or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttachmentName(String);

impl AttachmentName {
    /// The name for these stored bytes.
    pub fn for_bytes(bytes: &[u8], format: StoredFormat) -> Self {
        let digest = Sha256::digest(bytes);
        let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
        Self(format!("{hex}.{}", format.extension()))
    }

    /// Parse a bare file name (`3f2a…c1.png`).
    pub fn parse(name: &str) -> Option<Self> {
        let (hash, ext) = name.split_once('.')?;
        let hash_ok = hash.len() == 32 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        (hash_ok && StoredFormat::from_extension(ext).is_some()).then(|| Self(name.to_string()))
    }

    /// Parse the path a note uses (`attachments/3f2a…c1.png`), exactly.
    pub fn from_note_path(path: &str) -> Option<Self> {
        Self::parse(path.strip_prefix(FOLDER)?.strip_prefix('/')?)
    }

    /// The file name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path a note uses.
    pub fn note_path(&self) -> String {
        format!("{FOLDER}/{}", self.0)
    }
}

// ---------- Store ---------- //

/// How one image is saved.
#[derive(Debug, Clone, Copy, Default)]
pub struct SaveOptions {
    /// Shrink photos larger than 2560 px on their longest side (Settings › Writing).
    pub shrink_large: bool,
}

/// Owns the attachments folder. Cheap to clone; every write is atomic (temporary file, then rename)
/// and the same bytes always land in the same file.
#[derive(Clone)]
pub struct ImageStore {
    dir: Arc<PathBuf>,
    /// Turns HEIC into JPEG where the OS can (macOS).
    heic: Option<HeicConverter>,
    fetcher: WebFetcher,
}

impl ImageStore {
    /// A store for `dir` (normally `<app data>/attachments`). Call [`ImageStore::ensure_dir`] once.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: Arc::new(dir.into()), heic: None, fetcher: WebFetcher::default() }
    }

    /// Give the store a HEIC → JPEG converter.
    pub fn with_heic_converter(mut self, converter: Option<HeicConverter>) -> Self {
        self.heic = converter;
        self
    }

    /// Use another fetcher (tests reach a loopback server through one).
    pub fn with_fetcher(mut self, fetcher: WebFetcher) -> Self {
        self.fetcher = fetcher;
        self
    }

    /// The attachments folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Create the folder if it isn't there.
    pub fn ensure_dir(&self) -> AppResult<()> {
        std::fs::create_dir_all(self.dir.as_path())
            .map_err(|e| AppError::internal(format!("Cilica couldn't create its attachments folder: {e}. Check that you can write to its data folder.")))
    }

    // ── Saving ─────────────────────────────────────────────────────────

    /// Check, convert and save image bytes. `original_name` suggests the alt text.
    pub async fn save_bytes(&self, bytes: Vec<u8>, original_name: Option<&str>, options: SaveOptions) -> AppResult<SavedImage> {
        self.save(bytes, original_name, options, false).await
    }

    /// Save base64 image data (a pasted image), with or without a `data:…;base64,` prefix.
    pub async fn save_base64(&self, data: &str, original_name: Option<&str>, options: SaveOptions) -> AppResult<SavedImage> {
        let bytes = decode_base64(data).ok_or_else(|| AppError::invalid("That pasted image couldn't be read. Copy it again and paste."))?;
        self.save_bytes(bytes, original_name, options).await
    }

    /// Read a local file (dropped from Finder or picked) and save it.
    pub async fn save_file(&self, path: &Path, options: SaveOptions) -> AppResult<SavedImage> {
        // Check what it is and how big before reading any of it
        let meta = tokio::fs::metadata(path).await.map_err(|_| AppError::not_found("Cilica couldn't find that file. Check it still exists and try again."))?;
        if !meta.is_file() {
            return Err(AppError::invalid("That's a folder, not an image. Drop an image file instead."));
        }
        if meta.len() > MAX_INPUT_BYTES as u64 {
            return Err(AppError::invalid("That image is larger than 40 MB. Make it smaller and try again."));
        }
        let bytes = tokio::fs::read(path).await.map_err(|e| AppError::internal(format!("Cilica couldn't read that file: {e}. Check you can open it and try again.")))?;
        let name = path.file_stem().and_then(|s| s.to_str()).map(str::to_string);
        self.save_bytes(bytes, name.as_deref(), options).await
    }

    /// Download an image from a web link (see [`crate::fetch`] for the safety rules) and save it.
    /// Tracking pixels (2 × 2 or smaller) are refused.
    pub async fn fetch(&self, link: &str, options: SaveOptions) -> AppResult<SavedImage> {
        let downloaded = self.fetcher.get(link).await?;
        let name = downloaded.url.path_segments().and_then(|mut s| s.next_back()).map(percent_decoded).filter(|s| !s.is_empty());
        let stem = name.as_deref().map(|n| Path::new(n).file_stem().and_then(|s| s.to_str()).unwrap_or(n).to_string());
        self.save(downloaded.bytes, stem.as_deref(), options, true).await
    }

    /// Prepare off the async runtime, then write the file once.
    async fn save(&self, bytes: Vec<u8>, original_name: Option<&str>, options: SaveOptions, refuse_tiny: bool) -> AppResult<SavedImage> {
        let store = self.clone();
        let name = alt_from_name(original_name);
        tokio::task::spawn_blocking(move || {
            // Check, convert, maybe shrink
            let prepared = Preparer { shrink_large: options.shrink_large, heic: store.heic.as_ref() }.prepare(bytes)?;
            if refuse_tiny && prepared.width <= TRACKING_PIXEL_SIDE && prepared.height <= TRACKING_PIXEL_SIDE {
                return Err(AppError::invalid("That image is only a pixel or two, so Cilica didn't keep it. Links like that are usually trackers."));
            }

            // Write it under its content hash
            let file = AttachmentName::for_bytes(&prepared.bytes, prepared.format);
            store.write_once(&file, &prepared.bytes)?;
            Ok(SavedImage { path: file.note_path(), width: prepared.width, height: prepared.height, name })
        })
        .await
        .map_err(|e| AppError::internal(format!("Cilica stopped while saving the image: {e}. Try again.")))?
    }

    /// Write `bytes` as `name` unless that file exists (then only mark it as just used, so a
    /// clean-up running now doesn't take it). Temporary file plus rename, so a crash never leaves
    /// half an image under a real name.
    fn write_once(&self, name: &AttachmentName, bytes: &[u8]) -> AppResult<()> {
        let path = self.dir.join(name.as_str());
        let write_error = |e: std::io::Error| AppError::internal(format!("Cilica couldn't save the image: {e}. Check there's space on your disk and try again."));

        // Same bytes, same file: just touch it
        if path.is_file() {
            if let Ok(file) = std::fs::File::options().append(true).open(&path) {
                let _ = file.set_modified(SystemTime::now());
            }
            return Ok(());
        }

        // Write a temporary file next to it, then move it into place
        self.ensure_dir()?;
        let temp = self.dir.join(format!(".{}.{}-{}.tmp", name.as_str(), std::process::id(), TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)));
        let written = (|| {
            let mut file = std::fs::File::create(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            std::fs::rename(&temp, &path)
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temp);
            return Err(write_error(error));
        }
        Ok(())
    }

    // ── Reading ────────────────────────────────────────────────────────

    /// The file behind a note's `attachments/<name>` path. Anything else (other folders, `..`,
    /// absolute paths, other names) is refused.
    pub fn absolute(&self, path: &str) -> AppResult<PathBuf> {
        let name = AttachmentName::from_note_path(path).ok_or_else(|| AppError::invalid("That isn't an image in Cilica's attachments folder."))?;
        Ok(self.dir.join(name.as_str()))
    }

    /// An attachment prepared for a model to look at (see [`crate::format::for_model`]). The
    /// decoding runs off the async runtime.
    pub async fn read_for_model(&self, path: &str, max_side: u32) -> AppResult<crate::format::ModelImage> {
        let source = self.absolute(path)?;
        let bytes = tokio::fs::read(&source).await.map_err(|_| AppError::not_found("That image is missing from Cilica's attachments folder."))?;
        tokio::task::spawn_blocking(move || crate::format::for_model(&bytes, max_side))
            .await
            .map_err(|_| AppError::internal("Cilica couldn't read that image."))?
    }

    /// Copy an attachment to `dest` (Save as…).
    pub async fn copy_to(&self, path: &str, dest: &Path) -> AppResult<()> {
        let source = self.absolute(path)?;
        if !tokio::fs::try_exists(&source).await.unwrap_or(false) {
            return Err(AppError::not_found("That image is missing from Cilica's attachments folder."));
        }
        tokio::fs::copy(&source, dest).await.map(|_| ()).map_err(|e| {
            AppError::internal(format!("Cilica couldn't save {}: {e}. Check that the folder exists and you can write to it.", dest.display()))
        })
    }

    // ── Clean-up ───────────────────────────────────────────────────────

    /// Delete attachments no note, version or assistant record points at and that are older than
    /// `min_age` (so an image pasted moments ago survives until the note saves), plus leftover
    /// temporary files. Files that aren't attachment names are left alone. Returns how many went.
    pub async fn gc(&self, referenced: &HashSet<String>, min_age: Duration) -> AppResult<u32> {
        let dir = self.dir.clone();
        let referenced = referenced.clone();
        tokio::task::spawn_blocking(move || {
            let Ok(entries) = std::fs::read_dir(dir.as_path()) else { return Ok(0) };
            let mut removed = 0;
            for entry in entries.flatten() {
                // Only regular files old enough to be safe
                let Ok(meta) = entry.metadata() else { continue };
                let old_enough = meta.modified().ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= min_age);
                if !meta.is_file() || !old_enough {
                    continue;
                }

                // Unreferenced attachments and stray temporary files go
                let name = entry.file_name().to_string_lossy().to_string();
                let unused = AttachmentName::parse(&name).is_some() && !referenced.contains(&name);
                let stray = name.starts_with('.') && name.ends_with(".tmp");
                if (unused || stray) && std::fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
            }
            Ok(removed)
        })
        .await
        .map_err(|e| AppError::internal(format!("Cilica stopped while tidying attachments: {e}")))?
    }
}

/// The suggested alt text: the original name's stem, or "Image".
fn alt_from_name(original: Option<&str>) -> String {
    let stem = original.map(|n| Path::new(n.trim()).file_stem().and_then(|s| s.to_str()).unwrap_or(n).trim().to_string());
    stem.filter(|s| !s.is_empty()).unwrap_or_else(|| "Image".to_string())
}

/// Base64 (optionally a whole data URI) to bytes, ignoring whitespace.
pub(crate) fn decode_base64(data: &str) -> Option<Vec<u8>> {
    let payload = match data.trim().strip_prefix("data:") {
        Some(uri) => uri.split_once(',').filter(|(head, _)| head.ends_with(";base64")).map(|(_, b)| b)?,
        None => data,
    };
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64.decode(compact.as_bytes()).ok()
}

/// `%20`-style escapes in a URL path segment, decoded where they make UTF-8.
fn percent_decoded(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| segment.to_string())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use image::ImageFormat;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::fetch::FetchLimits;
    use crate::format::tests::encoded;

    /// A store in a fresh temporary folder.
    fn store() -> (tempfile::TempDir, ImageStore) {
        let temp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(temp.path().join(FOLDER));
        store.ensure_dir().unwrap();
        (temp, store)
    }

    #[test]
    fn names_are_strict() {
        let good = "0123456789abcdef0123456789abcdef.png";
        assert!(AttachmentName::parse(good).is_some());
        assert!(AttachmentName::from_note_path(&format!("attachments/{good}")).is_some());
        for bad in [
            "0123456789ABCDEF0123456789abcdef.png",
            "0123456789abcdef0123456789abcde.png",
            "0123456789abcdef0123456789abcdef.svg",
            "0123456789abcdef0123456789abcdef.png.png",
            "../0123456789abcdef0123456789abcdef.png",
        ] {
            assert!(AttachmentName::parse(bad).is_none(), "{bad}");
        }
        for bad in [
            good.to_string(),
            format!("/attachments/{good}"),
            format!("attachments/../{good}"),
            format!("attachments//{good}"),
            format!("attachments/sub/{good}"),
            "attachments/../../etc/passwd".to_string(),
        ] {
            assert!(AttachmentName::from_note_path(&bad).is_none(), "{bad}");
        }
    }

    #[tokio::test]
    async fn same_bytes_same_file() {
        let (_temp, store) = store();
        let png = encoded(5, 3, ImageFormat::Png);
        let a = store.save_bytes(png.clone(), Some("Screen Shot.png"), SaveOptions::default()).await.unwrap();
        let b = store.save_bytes(png.clone(), None, SaveOptions::default()).await.unwrap();
        assert_eq!(a.path, b.path);
        assert!(a.path.starts_with("attachments/") && a.path.ends_with(".png"));
        assert_eq!((a.width, a.height, a.name.as_str(), b.name.as_str()), (5, 3, "Screen Shot", "Image"));

        // One file, holding exactly the bytes, and no temporary files left over
        let files: Vec<_> = std::fs::read_dir(store.dir()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(std::fs::read(store.absolute(&a.path).unwrap()).unwrap(), png);
    }

    #[tokio::test]
    async fn saves_base64_and_files() {
        let (temp, store) = store();
        let jpeg = encoded(4, 4, ImageFormat::Jpeg);
        let data = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&jpeg));
        let saved = store.save_base64(&data, None, SaveOptions::default()).await.unwrap();
        assert!(saved.path.ends_with(".jpg"));

        // A file whose name lies about its type is stored as what it really is
        let lying = temp.path().join("holiday.gif");
        std::fs::write(&lying, &jpeg).unwrap();
        let from_file = store.save_file(&lying, SaveOptions::default()).await.unwrap();
        assert_eq!(from_file.path, saved.path);
        assert_eq!(from_file.name, "holiday");
        assert!(store.save_file(temp.path(), SaveOptions::default()).await.is_err());
    }

    #[tokio::test]
    async fn copies_out_and_refuses_paths_outside() {
        let (temp, store) = store();
        let saved = store.save_bytes(encoded(2, 2, ImageFormat::Png), None, SaveOptions::default()).await.unwrap();
        let dest = temp.path().join("copy.png");
        store.copy_to(&saved.path, &dest).await.unwrap();
        assert!(dest.is_file());
        assert!(store.absolute("attachments/../cilica.db").is_err());
        assert!(store.copy_to("attachments/0123456789abcdef0123456789abcdef.png", &dest).await.is_err());
    }

    #[tokio::test]
    async fn cleans_up_only_old_unreferenced_attachments() {
        let (_temp, store) = store();
        let kept = store.save_bytes(encoded(3, 3, ImageFormat::Png), None, SaveOptions::default()).await.unwrap();
        let unused = store.save_bytes(encoded(4, 4, ImageFormat::Png), None, SaveOptions::default()).await.unwrap();
        std::fs::write(store.dir().join("notes.txt"), b"not ours").unwrap();
        let referenced: HashSet<String> = [kept.path.trim_start_matches("attachments/").to_string()].into();

        // Too young to delete
        assert_eq!(store.gc(&referenced, Duration::from_secs(3600)).await.unwrap(), 0);

        // Old enough: only the unreferenced attachment goes
        assert_eq!(store.gc(&referenced, Duration::ZERO).await.unwrap(), 1);
        assert!(store.absolute(&kept.path).unwrap().is_file());
        assert!(!store.absolute(&unused.path).unwrap().exists());
        assert!(store.dir().join("notes.txt").is_file());
    }

    #[tokio::test]
    async fn fetches_images_and_refuses_tracking_pixels() {
        let (_temp, store) = store();

        // Serve a 3 × 3 PNG, then a 1 × 1 one, then an SVG
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec();
        let bodies = vec![encoded(3, 3, ImageFormat::Png), encoded(1, 1, ImageFormat::Png), svg];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            for body in bodies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 2048];
                let _ = socket.read(&mut buf).await;
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
            }
        });
        let store = store.with_fetcher(WebFetcher::allowing(FetchLimits::default(), |ip| ip == IpAddr::V4(Ipv4Addr::LOCALHOST)));

        let saved = store.fetch(&format!("http://127.0.0.1:{port}/img/My%20Chart.png?x=1"), SaveOptions::default()).await.unwrap();
        assert_eq!((saved.width, saved.height, saved.name.as_str()), (3, 3, "My Chart"));
        let tiny = store.fetch(&format!("http://127.0.0.1:{port}/pixel.gif"), SaveOptions::default()).await.unwrap_err();
        assert!(tiny.message.contains("pixel"), "{}", tiny.message);
        let svg = store.fetch(&format!("http://127.0.0.1:{port}/logo.png"), SaveOptions::default()).await.unwrap_err();
        assert!(svg.message.contains("SVG"), "{}", svg.message);
    }
}
