//! The global recording hotkey: works while Margin is in the background.
//!
//! The accelerator comes from `settings.keybinds["recording.toggle"]` in the UI registry's format
//! (`Mod+Shift+R`, where `Mod` is Cmd on macOS and Ctrl elsewhere). A missing entry means the
//! default; an empty string turns the hotkey off.

use margin_domain::{RecordingTrigger, Settings, ToastAction, ToastKind, ToastRequest};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::Shell;

/// Keybind registry id of "Start or stop recording".
pub const ACTION_ID: &str = "recording.toggle";
/// Default accelerator.
pub const DEFAULT: &str = "Mod+Shift+R";

/// The hotkey currently registered with the OS.
#[derive(Debug, Clone)]
pub struct Registered {
    pub accelerator: String,
    pub shortcut: Shortcut,
}

/// The accelerator the settings ask for, or `None` when the hotkey is turned off.
pub fn accelerator(settings: &Settings) -> Option<String> {
    match settings.keybinds.get(ACTION_ID) {
        Some(value) if value.trim().is_empty() => None,
        Some(value) => Some(value.trim().to_string()),
        None => Some(DEFAULT.to_string()),
    }
}

/// Parse a registry accelerator (`Mod+Shift+R`) into a global shortcut.
pub fn parse(accelerator: &str) -> Result<Shortcut, String> {
    let tokens: Vec<String> = accelerator
        .split('+')
        .map(|token| {
            let token = token.trim();
            match token.to_ascii_lowercase().as_str() {
                "mod" | "cmdorctrl" | "commandorcontrol" => "CommandOrControl".to_string(),
                "meta" | "cmd" | "command" | "super" | "win" => "Super".to_string(),
                "opt" | "option" | "alt" => "Alt".to_string(),
                "ctrl" | "control" => "Control".to_string(),
                "shift" => "Shift".to_string(),
                _ => token.to_string(),
            }
        })
        .collect();
    tokens.join("+").parse::<Shortcut>().map_err(|e| e.to_string())
}

/// How the accelerator reads in copy: `⌘⇧R` on macOS, `Ctrl+Shift+R` elsewhere.
pub fn display(accelerator: &str) -> String {
    let mac = cfg!(target_os = "macos");
    let parts: Vec<String> = accelerator
        .split('+')
        .map(|token| {
            let token = token.trim();
            let lower = token.to_ascii_lowercase();
            let symbol = match (lower.as_str(), mac) {
                ("mod" | "cmdorctrl" | "commandorcontrol", true) | ("meta" | "cmd" | "command" | "super", true) => "⌘",
                ("mod" | "cmdorctrl" | "commandorcontrol" | "ctrl" | "control", false) => "Ctrl",
                ("meta" | "cmd" | "command" | "super" | "win", false) => "Win",
                ("shift", true) => "⇧",
                ("shift", false) => "Shift",
                ("alt" | "opt" | "option", true) => "⌥",
                ("alt" | "opt" | "option", false) => "Alt",
                ("ctrl" | "control", true) => "⌃",
                _ => return token.strip_prefix("Key").unwrap_or(token).to_uppercase(),
            };
            symbol.to_string()
        })
        .collect();
    if mac { parts.concat() } else { parts.join("+") }
}

/// Make the registered hotkey match `settings`: unregister the old one, register the new one,
/// and raise a toast (never an error) when the OS refuses it.
pub fn sync(app: &AppHandle, settings: &Settings) {
    let shell = app.state::<Shell>();
    let wanted = accelerator(settings);
    let previous = shell.hotkey.lock().ok().and_then(|mut h| h.take());
    let shortcuts = app.global_shortcut();
    if let Some(previous) = previous
        && let Err(error) = shortcuts.unregister(previous.shortcut)
    {
        tracing::warn!(%error, accelerator = previous.accelerator, "could not unregister the recording hotkey");
    }
    let Some(accelerator) = wanted else { return };

    let shortcut = match parse(&accelerator) {
        Ok(shortcut) => shortcut,
        Err(error) => {
            tracing::warn!(%error, accelerator, "invalid recording hotkey");
            shell.toast(taken_toast(
                format!("{} is not a shortcut Margin can use", display(&accelerator)),
                "Choose a different shortcut for Start or stop recording in Settings.",
            ));
            return;
        }
    };
    let registered = shortcuts.on_shortcut(shortcut, |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            super::toggle_recording(app, RecordingTrigger::Hotkey);
        }
    });
    match registered {
        Ok(()) => {
            if let Ok(mut current) = shell.hotkey.lock() {
                *current = Some(Registered { accelerator, shortcut });
            }
        }
        Err(error) => {
            tracing::warn!(%error, accelerator, "could not register the recording hotkey");
            shell.toast(taken_toast(
                format!("{} is already in use", display(&accelerator)),
                "Another app is using this shortcut, so it will not start recording. Choose a different one in Settings.",
            ));
        }
    }
}

fn taken_toast(title: String, description: &str) -> ToastRequest {
    ToastRequest {
        kind: ToastKind::Error,
        title,
        description: Some(description.into()),
        action_label: Some("Open Settings".into()),
        action: Some(ToastAction::OpenSettings { page: "keybinds".into() }),
    }
}

#[cfg(test)]
mod tests {
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    use super::*;

    #[test]
    fn parses_registry_accelerators() {
        let mod_key = if cfg!(target_os = "macos") { Modifiers::SUPER } else { Modifiers::CONTROL };
        assert_eq!(parse("Mod+Shift+R").unwrap(), Shortcut::new(Some(mod_key | Modifiers::SHIFT), Code::KeyR));
        assert_eq!(parse("Alt+Shift+Space").unwrap(), Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space));
        assert_eq!(parse("Meta+KeyK").unwrap(), Shortcut::new(Some(Modifiers::SUPER), Code::KeyK));
        assert!(parse("Mod+Shift+").is_err());
        assert!(parse("Banana+R").is_err());
    }

    #[test]
    fn missing_means_default_and_empty_means_off() {
        let mut settings = Settings::default();
        assert_eq!(accelerator(&settings).as_deref(), Some(DEFAULT));
        settings.keybinds.insert(ACTION_ID.into(), "Mod+Alt+R".into());
        assert_eq!(accelerator(&settings).as_deref(), Some("Mod+Alt+R"));
        settings.keybinds.insert(ACTION_ID.into(), " ".into());
        assert_eq!(accelerator(&settings), None);
    }

    #[test]
    fn displays_for_people() {
        if cfg!(target_os = "macos") {
            assert_eq!(display("Mod+Shift+R"), "⌘⇧R");
        } else {
            assert_eq!(display("Mod+Shift+R"), "Ctrl+Shift+R");
        }
    }
}
