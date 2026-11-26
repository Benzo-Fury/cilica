//! Note images for models that can see them (the `view_images` tool).
//!
//! Tool results carry image *references* (`attachments/<name>`); the bytes are loaded here just
//! before each request: read from the attachments folder, fitted inside 1568 px and re-encoded
//! (which also drops any metadata), then kept in a small in-memory cache so a turn's later steps
//! and the next turn don't decode the same file again. The same file always gives the same bytes,
//! so the provider's prompt cache holds across requests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cilica_attachments::{ImageStore, MODEL_MAX_SIDE};
use cilica_domain::{AppError, AppResult};
use cilica_llm::{ChatMessage, ContentBlock};

/// Images kept decoded in memory (each is at most a few hundred KB of base64).
const CACHE_ENTRIES: usize = 24;

/// One image ready for a request.
#[derive(Debug, Clone)]
pub struct LoadedImage {
    pub media_type: &'static str,
    /// Base64 of the re-encoded bytes.
    pub data: Arc<str>,
    pub width: u32,
    pub height: u32,
}

#[derive(Default)]
struct Inner {
    store: Mutex<Option<ImageStore>>,
    /// Most recently used last.
    cache: Mutex<VecDeque<(String, LoadedImage)>>,
}

/// The attachments folder as models see it. Cheap to clone; clones share the cache.
#[derive(Clone, Default)]
pub struct ModelImages {
    inner: Arc<Inner>,
}

impl ModelImages {
    /// Images from `store`, or none (tests, and builds without an attachments folder).
    pub fn new(store: Option<ImageStore>) -> Self {
        let this = Self::default();
        *this.inner.store.lock().unwrap() = store;
        this
    }

    /// Point at the attachments folder (the app does this once at startup).
    pub fn set_store(&self, store: ImageStore) {
        *self.inner.store.lock().unwrap() = Some(store);
        self.inner.cache.lock().unwrap().clear();
    }

    /// Whether there is a folder to read images from.
    pub fn available(&self) -> bool {
        self.inner.store.lock().unwrap().is_some()
    }

    /// The image at a note path, from the cache or the folder.
    pub async fn load(&self, path: &str) -> AppResult<LoadedImage> {
        // From the cache, moved to the back as most recently used
        {
            let mut cache = self.inner.cache.lock().unwrap();
            if let Some(at) = cache.iter().position(|(p, _)| p == path) {
                let entry = cache.remove(at).expect("position is in range");
                let image = entry.1.clone();
                cache.push_back(entry);
                return Ok(image);
            }
        }

        // From the folder, then remembered
        let store = self.inner.store.lock().unwrap().clone().ok_or_else(|| AppError::unavailable("Cilica can't open note images here."))?;
        let prepared = store.read_for_model(path, MODEL_MAX_SIDE).await?;
        let image = LoadedImage { media_type: prepared.media_type, data: prepared.to_base64().into(), width: prepared.width, height: prepared.height };
        let mut cache = self.inner.cache.lock().unwrap();
        cache.retain(|(p, _)| p != path);
        cache.push_back((path.to_string(), image.clone()));
        while cache.len() > CACHE_ENTRIES {
            cache.pop_front();
        }
        Ok(image)
    }

    /// Fill in the bytes of every image in `messages` that has none. An image that can't be
    /// loaded (deleted since, say) stays empty, and providers send a short note in its place.
    pub async fn hydrate(&self, messages: &mut [ChatMessage]) {
        for message in messages.iter_mut() {
            for block in message.content.iter_mut() {
                let ContentBlock::ToolResult { images, .. } = block else { continue };
                for image in images.iter_mut().filter(|i| i.data.is_none()) {
                    match self.load(&image.path).await {
                        Ok(loaded) => {
                            image.media_type = loaded.media_type.to_string();
                            image.data = Some(loaded.data);
                        }
                        Err(error) => tracing::debug!(path = %image.path, message = %error.message, "could not load an image for the model"),
                    }
                }
            }
        }
    }
}

/// Remove every image from `messages` (for a model that can't see them), keeping the text.
pub(crate) fn strip_images(messages: &mut [ChatMessage]) {
    for message in messages.iter_mut() {
        for block in message.content.iter_mut() {
            if let ContentBlock::ToolResult { images, .. } = block {
                images.clear();
            }
        }
    }
}
