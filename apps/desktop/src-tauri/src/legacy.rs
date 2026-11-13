//! One-off clean-up for copies updated from Margin (0.3.0 and earlier) to Cilica (0.4.0).
//!
//! Remove once every install has updated, together with the other Margin leftovers:
//! `keychain.rs` (`LEGACY_SERVICE`), `core.rs` (`move_legacy_data`) and
//! `NSIS_HOOK_PREINSTALL` in `windows/hooks.nsh` (uninstalling Margin before Cilica installs; the
//! post-install hook there is Cilica Setup's and stays).

/// The app's name before the rename: its `.app` bundle and its login item were named after it.
const LEGACY_NAME: &str = "Margin";

/// Runs before Tauri starts. On macOS, when the updater left the new app in `Margin.app`, renames
/// the bundle to `Cilica.app` and relaunches from there; this process then exits.
pub fn rename_bundle() {
    #[cfg(target_os = "macos")]
    if let Err(error) = macos::rename_bundle() {
        tracing::warn!(%error, "could not rename Margin.app to Cilica.app");
    }
}

/// Removes the "open at login" entry Margin left behind. Cilica writes its own under its new
/// name when Open at login is on.
pub fn remove_login_item() {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut builder = auto_launch::AutoLaunchBuilder::new();
    builder.set_app_name(LEGACY_NAME).set_app_path(&exe.display().to_string());
    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);
    let Ok(item) = builder.build() else { return };
    if item.is_enabled().unwrap_or(false)
        && let Err(error) = item.disable()
    {
        tracing::warn!(%error, "could not remove Margin's login item");
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::{Path, PathBuf};
    use std::process::{Command, exit};

    /// Renames `…/Margin.app` to `…/Cilica.app`, then relaunches and exits. Does nothing when the
    /// app runs from any other bundle (or none, as in development) or `Cilica.app` already exists.
    pub fn rename_bundle() -> std::io::Result<()> {
        let Some(bundle) = legacy_bundle(&std::env::current_exe()?) else { return Ok(()) };
        let target = bundle.with_file_name("Cilica.app");
        if target.exists() {
            return Ok(());
        }
        std::fs::rename(&bundle, &target)?;
        // Open the renamed app once this process has gone, so the single-instance check does not
        // hand the launch back to us.
        Command::new("/bin/sh")
            .args(["-c", r#"while kill -0 "$1" 2>/dev/null; do sleep 0.1; done; open "$2""#, "sh"])
            .arg(std::process::id().to_string())
            .arg(&target)
            .spawn()?;
        exit(0);
    }

    /// The `Margin.app` bundle that contains `exe`, if any.
    pub(super) fn legacy_bundle(exe: &Path) -> Option<PathBuf> {
        let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"))?;
        (bundle.file_stem()? == super::LEGACY_NAME).then(|| bundle.to_path_buf())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn finds_only_the_margin_bundle() {
            let exe = Path::new("/Applications/Margin.app/Contents/MacOS/cilica-desktop");
            assert_eq!(legacy_bundle(exe), Some(PathBuf::from("/Applications/Margin.app")));
            assert_eq!(legacy_bundle(Path::new("/Applications/Cilica.app/Contents/MacOS/cilica-desktop")), None);
            assert_eq!(legacy_bundle(Path::new("/Users/me/Cilica/target/debug/cilica-desktop")), None);
        }
    }
}
