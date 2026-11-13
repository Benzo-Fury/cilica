use cilica_domain::{PermissionKind, PermissionStatus};

use crate::{AudioError, AudioResult, CaptureSource, RawStream, SourceKind};

/// System output capture. Not available on this platform.
#[derive(Debug, Clone, Default)]
pub struct SystemSource;

impl CaptureSource for SystemSource {
    fn kind(&self) -> SourceKind {
        SourceKind::System
    }
    fn open(&self) -> AudioResult<RawStream> {
        Err(AudioError::SystemUnsupported("this operating system is not supported".into()))
    }
}

pub(crate) fn permission_status(_kind: PermissionKind) -> PermissionStatus {
    PermissionStatus::NotNeeded
}
pub(crate) fn request_permission(kind: PermissionKind) -> PermissionStatus {
    permission_status(kind)
}
pub(crate) fn permission_settings_url(_kind: PermissionKind) -> Option<&'static str> {
    None
}
pub(crate) fn open_url(_url: &str) -> std::io::Result<()> {
    Err(std::io::Error::other("not supported"))
}
