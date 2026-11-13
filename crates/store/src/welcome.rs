//! The welcome note Cilica opens on after first-run setup. It carries what the old setup dialog's
//! last page did: the three shortcuts that get you most of the way. No tables of its own.

use cilica_domain::Note;

use crate::notes::NewNote;
use crate::{Db, StoreResult};

/// Title of the note written when setup finishes.
pub const WELCOME_TITLE: &str = "Welcome to Cilica";

/// Shortcut glyphs differ by platform; the store never asks which one it runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutStyle {
    /// ⌘⇧R
    Mac,
    /// Ctrl+Shift+R
    Other,
}

impl ShortcutStyle {
    /// Format a chord of modifiers plus a key, e.g. `(true, "R")` → "⌘⇧R" or "Ctrl+Shift+R".
    fn chord(self, shift: bool, key: &str) -> String {
        match (self, shift) {
            (Self::Mac, true) => format!("⌘⇧{key}"),
            (Self::Mac, false) => format!("⌘{key}"),
            (Self::Other, true) => format!("Ctrl+Shift+{key}"),
            (Self::Other, false) => format!("Ctrl+{key}"),
        }
    }
}

/// The welcome note's Markdown body, with the default shortcuts for this platform.
pub fn welcome_body(style: ShortcutStyle) -> String {
    let record = style.chord(true, "R");
    let search = style.chord(false, "K");
    let ask = style.chord(false, "J");
    format!(
        "Cilica is a notes app with an AI assistant and a meeting summariser. This note covers the basics. Delete it whenever you like.\n\n\
         ## Write\n\n\
         Notes are plain Markdown. Type `#` for a heading, `-` for a list and `[[` to link to another note. Shortcuts can be changed in Settings › Keybinds.\n\n\
         ## Ask Cilica\n\n\
         Press **{ask}** to open the assistant. Ask it to sort notes into categories, summarise a meeting or tidy up this note. Every change can be undone.\n\n\
         ## Record a meeting\n\n\
         Press **{record}** from anywhere, or click Record in the sidebar. Cilica hears your microphone and the call, transcribes it on this computer, and writes a summary with decisions and action items when you stop.\n\n\
         ## Find anything\n\n\
         Press **{search}** to search every note and transcript, or to ask Cilica a question about them.\n"
    )
}

impl Db {
    /// Write the welcome note at the top of Notes.
    pub async fn create_welcome_note(&self, style: ShortcutStyle) -> StoreResult<Note> {
        self.create_note(NewNote { title: WELCOME_TITLE.into(), body_md: welcome_body(style), ..Default::default() }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_the_welcome_note_with_platform_shortcuts() {
        let db = Db::open_in_memory().await.unwrap();
        let note = db.create_welcome_note(ShortcutStyle::Mac).await.unwrap();
        assert_eq!(note.title, WELCOME_TITLE);
        assert!(note.body_md.contains("**⌘⇧R**"));
        assert!(note.body_md.contains("**⌘K**"));
        assert!(!note.body_md.contains("Ctrl"));
        assert_eq!(db.get_note(note.id).await.unwrap().title, WELCOME_TITLE);
    }

    #[test]
    fn spells_out_shortcuts_off_the_mac() {
        let body = welcome_body(ShortcutStyle::Other);
        assert!(body.contains("**Ctrl+Shift+R**"));
        assert!(body.contains("**Ctrl+J**"));
        assert!(!body.contains('⌘'));
    }
}
