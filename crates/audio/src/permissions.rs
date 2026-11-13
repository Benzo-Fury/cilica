//! Microphone, system-audio and accessibility permissions, per OS.

use cilica_domain::{PermissionKind, PermissionStatus, Permissions};

use crate::{AudioError, AudioResult, platform};

/// The current state of one permission. Cheap; never prompts.
pub fn status(kind: PermissionKind) -> PermissionStatus {
    platform::permission_status(kind)
}

/// All three at once.
pub fn all() -> Permissions {
    Permissions {
        microphone: status(PermissionKind::Microphone),
        system_audio: status(PermissionKind::SystemAudio),
        accessibility: status(PermissionKind::Accessibility),
    }
}

/// Show the OS prompt where one exists. Blocks until answered (up to two minutes), so call it
/// from a blocking thread.
pub fn request(kind: PermissionKind) -> PermissionStatus {
    platform::request_permission(kind)
}

/// Open the matching pane of the OS privacy settings.
pub fn open_settings(kind: PermissionKind) -> AudioResult<()> {
    let url = platform::permission_settings_url(kind).ok_or_else(|| {
        AudioError::SystemUnsupported("this system has no settings page for that permission; nothing needs to be allowed".into())
    })?;
    platform::open_url(url).map_err(AudioError::Io)
}
