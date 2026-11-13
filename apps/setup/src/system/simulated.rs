//! Setup off Windows: the same steps with pauses and nothing written, so the window can be run
//! and checked on macOS and Linux (`cargo run -p cilica-setup`, add `-- --uninstall` for the
//! other flow). Set `CILICA_SETUP_FAIL=copy` (any step's name) to see a step fail.

use std::path::{Path, PathBuf};
use std::time::Duration;

use cilica_domain::{AppError, AppResult};

use super::Existing;
use crate::job::Reporter;
use crate::plan::{IDENTIFIER, InstallOptions, PRODUCT, UninstallOptions};

pub fn webview_ready() -> bool {
    true
}

/// A pretend earlier install, so the Remove screen has something to show.
pub fn existing() -> Option<Existing> {
    Some(Existing { version: env!("CARGO_PKG_VERSION").to_owned(), dir: default_install_dir() })
}

pub fn default_install_dir() -> PathBuf {
    preview_root().join("AppData").join("Local").join(PRODUCT)
}

pub fn data_dir() -> PathBuf {
    preview_root().join("AppData").join("Roaming").join(IDENTIFIER)
}

fn preview_root() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

pub fn cilica_running(_dir: Option<&Path>) -> bool {
    false
}

pub fn install(options: &InstallOptions, _updating: bool, _session: &Path, report: &Reporter) -> AppResult<PathBuf> {
    play(report)?;
    Ok(PathBuf::from(&options.dir).join("Cilica.exe"))
}

pub fn uninstall(_dir: &Path, _options: UninstallOptions, report: &Reporter) -> AppResult<()> {
    play(report)
}

/// Walk the steps over a few seconds, the long ones in small moves.
fn play(report: &Reporter) -> AppResult<()> {
    let fail = std::env::var("CILICA_SETUP_FAIL").ok();
    for &step in report.steps() {
        report.start(step);
        let ticks = (step.weight() * 6.0).round().max(1.0) as u32;
        for tick in 1..=ticks {
            std::thread::sleep(Duration::from_millis(110));
            report.progress(step, tick as f32 / ticks as f32);
        }
        if fail.as_deref().is_some_and(|name| serde_json::to_value(step).ok().and_then(|v| v.as_str().map(|s| s == name)).unwrap_or(false)) {
            return Err(AppError::internal("This is a pretend failure, set by CILICA_SETUP_FAIL. Try again."));
        }
        report.done(step);
    }
    Ok(())
}

pub fn relaunch_from_temp(_session: &Path) -> AppResult<()> {
    Ok(())
}

pub fn runs_from_install_dir() -> bool {
    false
}

pub fn fall_back_to_wizard(_mode: crate::plan::Mode, _session: &Path) -> i32 {
    0
}

pub fn open(exe: &Path) -> AppResult<()> {
    tracing::info!(exe = %exe.display(), "would open Cilica");
    Ok(())
}

pub fn clean_up_after_exit(session: &Path) {
    let _ = std::fs::remove_dir_all(session);
}
