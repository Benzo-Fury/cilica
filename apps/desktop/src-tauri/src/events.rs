//! Bridges `margin_domain::AppEvent` to the webview as one typed tauri-specta event, and fans each
//! event out to in-process listeners in the shell (tray label, quit guard, Dynamic Island).
//!
//! Crates keep calling `core.events.emit(...)` as before; listeners are added with
//! [`crate::core::Core::on_event`] and never change how anything else emits.

use std::sync::{Arc, RwLock};

use margin_domain::{AppEvent, EventSink};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

/// The single backend → UI event. The UI listens once and switches on `payload.type`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct BackendEvent(pub AppEvent);

/// Something in the shell that wants to see every backend event. Called on the emitting thread,
/// so it must be quick and must not block on the main thread's event loop.
pub type Listener = Arc<dyn Fn(&AppEvent) + Send + Sync>;

/// The [`EventSink`] handed to every crate.
pub struct TauriSink {
    app: AppHandle,
    listeners: RwLock<Vec<Listener>>,
}

impl TauriSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app, listeners: RwLock::new(Vec::new()) }
    }

    /// Add an in-process listener. It sees events after the webview does.
    pub fn subscribe(&self, listener: impl Fn(&AppEvent) + Send + Sync + 'static) {
        if let Ok(mut listeners) = self.listeners.write() {
            listeners.push(Arc::new(listener));
        }
    }
}

impl EventSink for TauriSink {
    fn emit(&self, event: AppEvent) {
        // Copy the list so a listener can subscribe or emit without deadlocking.
        let listeners: Vec<Listener> = self.listeners.read().map(|l| l.clone()).unwrap_or_default();
        let for_listeners = (!listeners.is_empty()).then(|| event.clone());
        if let Err(error) = BackendEvent(event).emit(&self.app) {
            tracing::warn!(%error, "failed to emit backend event");
        }
        if let Some(event) = for_listeners {
            for listener in &listeners {
                listener(&event);
            }
        }
    }
}
