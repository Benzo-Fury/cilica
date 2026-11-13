//! Everything Setup does to the computer. `windows.rs` does it for real; `simulated.rs` plays the
//! same steps with pauses on macOS and Linux, so the window can be built and checked anywhere
//! with `cargo run -p cilica-setup`. Both offer the same functions; the shared ones live here.

#[cfg(not(windows))]
mod simulated;
#[cfg(windows)]
mod windows;

use std::path::{Path, PathBuf};

use cilica_domain::{AppError, AppResult};
#[cfg(not(windows))]
pub use simulated::*;
#[cfg(windows)]
pub use windows::*;

use crate::plan::megabytes;

/// Cilica, already on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Existing {
    pub version: String,
    pub dir: PathBuf,
}

/// This run's own temporary folder: the WebView2 profile, the unpacked installer and, when
/// removing Cilica, the copy of Setup that does it. Deleted after Setup exits.
pub fn session_dir(relaunched: bool) -> PathBuf {
    if relaunched
        && let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        return dir;
    }
    std::env::temp_dir().join(format!("cilica-setup-{}", std::process::id()))
}

/// Free bytes on the drive that holds `dir` (or would, once it exists).
pub fn free_space(dir: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let dir = dir.to_string_lossy().to_lowercase();
    disks
        .list()
        .iter()
        .filter(|disk| dir.starts_with(&disk.mount_point().to_string_lossy().to_lowercase()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space())
}

/// Check Cilica can go in `dir`: there is room for `needed` bytes and the folder can be written.
/// Creates the folder; the installer would anyway.
pub fn check_folder(dir: &Path, needed: u64) -> AppResult<()> {
    if let Some(free) = free_space(dir)
        && free < needed + needed / 10
    {
        return Err(AppError::invalid(format!(
            "There isn't enough space for Cilica. It needs {} MB and {} MB is free. Free up some space or choose another drive.",
            megabytes(needed),
            megabytes(free)
        )));
    }
    let probe = dir.join(".cilica-setup-check");
    std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&probe, b""))
        .and_then(|()| std::fs::remove_file(&probe))
        .map_err(|e| {
            AppError::new(
                cilica_domain::ErrorCode::PermissionDenied,
                format!("Cilica can't be installed in {}: {e}. Choose a folder in your user folder.", dir.display()),
            )
        })
}

/// Total size of the files under `dir`, 0 when it is gone.
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_sizes_add_up_and_a_missing_folder_is_empty() {
        let root = std::env::temp_dir().join(format!("cilica-setup-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("inner")).unwrap();
        std::fs::write(root.join("a"), [0u8; 10]).unwrap();
        std::fs::write(root.join("inner/b"), [0u8; 5]).unwrap();
        assert_eq!(dir_size(&root), 15);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(dir_size(&root), 0);
    }

    #[test]
    fn a_writable_folder_passes_the_check() {
        let root = std::env::temp_dir().join(format!("cilica-setup-check-{}", std::process::id()));
        check_folder(&root.join("Cilica"), 1).unwrap();
        assert!(root.join("Cilica").is_dir());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_session_folder_is_per_run() {
        let dir = session_dir(false);
        assert!(dir.ends_with(format!("cilica-setup-{}", std::process::id())));
    }
}
