//! What Setup does, as plain data: how it was started, the steps of each job and how much of the
//! bar each one fills, where Cilica goes, and the command lines for Tauri's NSIS installer and
//! uninstaller. No I/O, so all of it is tested on every platform.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

/// Cilica's product name: its install folder, Start menu entry and Installed apps entry.
pub const PRODUCT: &str = "Cilica";
/// Cilica's bundle identifier: the name of its data folders and its keychain service.
pub const IDENTIFIER: &str = "app.cilica.desktop";
/// Installed apps runs the copy of Setup next to Cilica with this to remove it.
pub const UNINSTALL_FLAG: &str = "--uninstall";
/// Cilica starts the Cilica Setup it downloaded with this, to install the update Setup carries
/// under Setup's window. Followed by a path, as Cilica 0.6 sent it to the copy of Setup next to
/// it: the downloaded installer to run instead.
pub const UPDATE_FLAG: &str = "--update";
/// Tauri's updater starts the package it downloaded with this (plus `/P` and `/R`), which is how
/// copies of Cilica from before 0.6, and Cilica when it can't start Setup itself, update.
pub const NSIS_UPDATE_FLAG: &str = "/UPDATE";
/// NSIS's silent switch. Cilica 0.6's copy of Setup runs the package Cilica downloaded with it,
/// thinking it is Tauri's NSIS installer; Setup passes the whole line on to the one it carries.
pub const NSIS_SILENT_FLAG: &str = "/S";
/// With [`UPDATE_FLAG`]: the version that installer holds.
pub const VERSION_FLAG: &str = "--version";
/// Set on the copy Setup starts from the temporary folder, so it can replace or delete Cilica's
/// folder (the copy next to Cilica is one of its files).
pub const RELAUNCHED_FLAG: &str = "--relaunched";
/// Asks the NSIS installer to leave out the desktop shortcut it makes in silent mode. Read by
/// the `NSIS_HOOK_POSTINSTALL` hook in `apps/desktop/src-tauri/windows/hooks.nsh`.
pub const NO_DESKTOP_FLAG: &str = "/NODESKTOP";

// ───────── How Setup was started ─────────

/// What this window is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Install,
    /// Cilica downloaded a new version and handed it over: install it at once, then reopen Cilica.
    Update,
    Uninstall,
}

/// The update to install in [`Mode::Update`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSource {
    /// A downloaded NSIS installer to run, already checked against its signature. `None`: the
    /// one this copy of Setup carries.
    pub installer: Option<PathBuf>,
    /// The version it installs, empty when nobody said.
    pub version: String,
}

/// The command line, read once at launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub mode: Mode,
    /// This is the temporary copy that replaces or removes Cilica, not the one inside its folder.
    pub relaunched: bool,
    /// Set in [`Mode::Update`].
    pub update: Option<UpdateSource>,
    /// Started like Tauri's NSIS installer in silent mode ([`NSIS_SILENT_FLAG`]): run the
    /// installer this copy carries with these arguments, without a window.
    pub silent: Option<Vec<String>>,
}

impl Launch {
    /// Read the arguments after the program name. A copy carrying Cilica answers the switches
    /// Tauri's NSIS installer takes, because that is how older copies of Cilica update. Then an
    /// update comes first; a copy of Setup without Cilica inside it can otherwise only remove
    /// Cilica, so it uninstalls whatever it was asked.
    pub fn parse(args: impl IntoIterator<Item = String>, has_payload: bool) -> Self {
        let args: Vec<String> = args.into_iter().collect();
        let has = |flag: &str| args.iter().any(|a| a.eq_ignore_ascii_case(flag));
        let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).filter(|v| !v.starts_with("--")).cloned();
        let relaunched = has(RELAUNCHED_FLAG);

        if has_payload && has(NSIS_SILENT_FLAG) {
            return Self { mode: Mode::Install, relaunched, update: None, silent: Some(args) };
        }

        let version = value(VERSION_FLAG).unwrap_or_default();
        let update = match value(UPDATE_FLAG).filter(|path| !path.trim().is_empty()) {
            Some(path) => Some(UpdateSource { installer: Some(PathBuf::from(path)), version }),
            None if has_payload && (has(UPDATE_FLAG) || has(NSIS_UPDATE_FLAG)) => Some(UpdateSource {
                installer: None,
                version: if version.is_empty() { env!("CARGO_PKG_VERSION").to_owned() } else { version },
            }),
            None => None,
        };
        let mode = if update.is_some() {
            Mode::Update
        } else if has(UNINSTALL_FLAG) || !has_payload {
            Mode::Uninstall
        } else {
            Mode::Install
        };
        Self { mode, relaunched, update, silent: None }
    }

    /// The arguments that start the same job from a copy of Setup in the temporary folder.
    pub fn relaunch_args(&self) -> Vec<String> {
        let mut args = match (&self.mode, &self.update) {
            (Mode::Update, Some(update)) => {
                let mut args = vec![UPDATE_FLAG.to_owned()];
                args.extend(update.installer.as_ref().map(|path| path.display().to_string()));
                args.extend([VERSION_FLAG.to_owned(), update.version.clone()]);
                args
            }
            _ => vec![UNINSTALL_FLAG.to_owned()],
        };
        args.push(RELAUNCHED_FLAG.to_owned());
        args
    }
}

// ───────── Choices ─────────

/// The Install screen's options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct InstallOptions {
    /// The folder Cilica goes in (already ending in `Cilica`).
    pub dir: String,
    pub desktop_shortcut: bool,
    pub open_at_sign_in: bool,
}

/// The Remove screen's one choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UninstallOptions {
    /// Also delete notes, recordings, settings and saved keys.
    pub delete_data: bool,
}

// ───────── Steps ─────────

/// One line of the progress screen. The UI owns the words for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Wait for Cilica to quit, so the update can replace its files.
    Close,
    /// Space on the drive and permission to write the folder.
    Check,
    /// Write the bundled installer to the temporary folder.
    Unpack,
    /// Run it: Cilica's files, the Start menu shortcut, the uninstaller.
    Copy,
    /// The desktop shortcut (made by the installer; listed so it can be seen).
    Shortcut,
    /// Open Cilica when you sign in.
    SignIn,
    /// Check Windows lists Cilica in Installed apps.
    Register,
    /// Run Cilica's uninstaller.
    RemoveFiles,
    /// Notes, recordings and settings.
    DeleteData,
    /// API keys in Windows Credential Manager.
    DeleteKeys,
    /// Check Windows no longer lists Cilica.
    Unregister,
}

impl Step {
    /// How much of the bar this step fills, relative to the others. Roughly its share of the time.
    pub fn weight(self) -> f32 {
        match self {
            Self::Close => 0.6,
            Self::Check => 0.4,
            Self::Unpack => 1.2,
            Self::Copy => 4.0,
            Self::Shortcut | Self::SignIn | Self::Register => 0.3,
            Self::RemoveFiles => 3.0,
            Self::DeleteData => 1.2,
            Self::DeleteKeys | Self::Unregister => 0.3,
        }
    }
}

/// The steps of an install, in order. An update keeps the shortcuts and login item it finds,
/// so it skips those steps.
pub fn install_steps(options: &InstallOptions, updating: bool) -> Vec<Step> {
    let mut steps = vec![Step::Check, Step::Unpack, Step::Copy];
    if options.desktop_shortcut && !updating {
        steps.push(Step::Shortcut);
    }
    if options.open_at_sign_in && !updating {
        steps.push(Step::SignIn);
    }
    steps.push(Step::Register);
    steps
}

/// The steps of an update Cilica handed over, in order. Its installer is already on disk, and
/// the shortcuts and login item stay as they are.
pub fn update_steps() -> Vec<Step> {
    vec![Step::Close, Step::Check, Step::Copy, Step::Register]
}

/// The steps of an uninstall, in order. Cilica's uninstaller closes Cilica if it is open.
pub fn uninstall_steps(options: UninstallOptions) -> Vec<Step> {
    let mut steps = vec![Step::RemoveFiles];
    if options.delete_data {
        steps.extend([Step::DeleteData, Step::DeleteKeys]);
    }
    steps.push(Step::Unregister);
    steps
}

/// The whole job's progress (0..1) when `step` is `within` (0..1) done. Steps before it count
/// in full; a step that is not in the list counts as nothing.
pub fn overall(steps: &[Step], step: Step, within: f32) -> f32 {
    let total: f32 = steps.iter().map(|s| s.weight()).sum();
    let Some(index) = steps.iter().position(|s| *s == step) else { return 0.0 };
    if total <= 0.0 {
        return 0.0;
    }
    let before: f32 = steps[..index].iter().map(|s| s.weight()).sum();
    ((before + step.weight() * within.clamp(0.0, 1.0)) / total).clamp(0.0, 1.0)
}

/// How far the copy is from the bytes the installer has written so far. Stops short of the end:
/// only the installer exiting finishes the step.
pub fn copy_fraction(written: u64, expected: u64) -> f32 {
    if expected == 0 {
        return 0.0;
    }
    (written as f64 / expected as f64).min(0.97) as f32
}

/// How many bytes an install writes. The release build records Cilica's size; a build without
/// it guesses from the compressed installer.
pub fn expected_install_bytes(app_bytes: Option<u64>, payload_len: u64) -> u64 {
    app_bytes.filter(|b| *b > 0).unwrap_or(payload_len.saturating_mul(5) / 2)
}

// ───────── Folders ─────────

/// The folder Cilica goes in when someone picks `picked`: a `Cilica` folder inside it, unless
/// they picked one already called that. Cilica's uninstaller deletes the folder, so it is never a
/// folder that holds other things.
pub fn install_dir_for(picked: &Path) -> PathBuf {
    let named_cilica = picked.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(PRODUCT));
    if named_cilica { picked.to_path_buf() } else { picked.join(PRODUCT) }
}

/// A registry path without the quotes NSIS writes around it (`InstallLocation`).
pub fn unquote(value: &str) -> &str {
    let v = value.trim();
    v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v)
}

/// Whole megabytes, rounded up, for sizes shown to people.
pub fn megabytes(bytes: u64) -> u32 {
    u32::try_from(bytes.div_ceil(1024 * 1024)).unwrap_or(u32::MAX)
}

// ───────── Command lines ─────────

/// Arguments for Tauri's NSIS installer: silent, and `/D=` last. A new install leaves out the
/// desktop shortcut unless asked; an update passes `/UPDATE`, as Tauri's updater does, so the
/// shortcuts stay as they are. NSIS reads `/D=` to the end of the line and does not accept quotes
/// around it, so these are passed raw, not quoted per argument.
pub fn installer_args(dir: &Path, desktop_shortcut: bool, updating: bool) -> Vec<String> {
    let mut args = vec!["/S".to_owned()];
    if updating {
        args.push("/UPDATE".to_owned());
    } else if !desktop_shortcut {
        args.push(NO_DESKTOP_FLAG.to_owned());
    }
    args.push(format!("/D={}", dir.display()));
    args
}

/// Arguments for Cilica's NSIS uninstaller: silent, and `_?=` so it runs in place and its exit
/// is the end of the uninstall (without it, NSIS copies itself to the temporary folder and
/// returns at once). Raw, for the same reason as [`installer_args`].
pub fn uninstaller_args(dir: &Path) -> Vec<String> {
    vec!["/S".to_owned(), format!("_?={}", dir.display())]
}

/// The `Run` value that opens Cilica when you sign in, in the form the autostart plugin writes,
/// so Cilica sees its own login item when it first starts.
pub fn run_value(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_copy_with_cilica_inside_installs() {
        let launch = Launch::parse(args(&[]), true);
        assert_eq!(launch, Launch { mode: Mode::Install, relaunched: false, update: None, silent: None });
    }

    #[test]
    fn an_update_handed_over_by_cilica_wins() {
        let update = UpdateSource { installer: Some(PathBuf::from("C:/T/a b.exe")), version: "0.6.0".into() };
        for payload in [true, false] {
            let launch = Launch::parse(args(&["--update", "C:/T/a b.exe", "--version", "0.6.0"]), payload);
            assert_eq!(launch, Launch { mode: Mode::Update, relaunched: false, update: Some(update.clone()), silent: None });
        }
        // Without a path, or without a version, it is still read sensibly.
        assert_eq!(Launch::parse(args(&["--update"]), false).mode, Mode::Uninstall);
        assert_eq!(Launch::parse(args(&["--update", "x.exe"]), false).update.unwrap().version, "");
    }

    #[test]
    fn a_copy_carrying_cilica_updates_with_what_it_carries() {
        let own = Some(UpdateSource { installer: None, version: env!("CARGO_PKG_VERSION").to_owned() });
        // Cilica, starting the Setup it downloaded.
        let launch = Launch::parse(args(&["--update", "--version", "9.9.9"]), true);
        assert_eq!(launch.mode, Mode::Update);
        assert_eq!(launch.update, Some(UpdateSource { installer: None, version: "9.9.9".into() }));
        // Tauri's updater, in copies of Cilica from before 0.6.
        let launch = Launch::parse(args(&["/P", "/UPDATE", "/R", "/ARGS"]), true);
        assert_eq!((launch.mode, launch.update, launch.silent), (Mode::Update, own, None));
    }

    #[test]
    fn a_silent_nsis_line_is_passed_on() {
        // Cilica 0.6's copy of Setup, running the downloaded package as if it were NSIS.
        let line = args(&["/S", "/UPDATE", "/D=C:\\Users\\Ana", "Lee\\AppData\\Local\\Cilica"]);
        let launch = Launch::parse(line.clone(), true);
        assert_eq!(launch.silent, Some(line.clone()));
        assert_eq!(launch.update, None);
        // A copy without Cilica inside has nothing to pass it to.
        assert_eq!(Launch::parse(line, false).silent, None);
    }

    #[test]
    fn a_relaunch_carries_the_same_job() {
        let update = Launch::parse(args(&["--update", "C:/T/a b.exe", "--version", "0.6.0"]), false);
        let relaunched = Launch::parse(update.relaunch_args(), false);
        assert_eq!(relaunched, Launch { relaunched: true, ..update });

        let own = Launch::parse(args(&["--update", "--version", "0.7.0"]), true);
        let relaunched = Launch::parse(own.relaunch_args(), true);
        assert_eq!(relaunched, Launch { relaunched: true, ..own });

        let uninstall = Launch::parse(args(&["--uninstall"]), false);
        assert_eq!(uninstall.relaunch_args(), args(&["--uninstall", "--relaunched"]));
    }

    #[test]
    fn uninstall_is_asked_for_or_the_only_choice() {
        assert_eq!(Launch::parse(args(&["--uninstall"]), true).mode, Mode::Uninstall);
        assert_eq!(Launch::parse(args(&[]), false).mode, Mode::Uninstall);
        assert!(Launch::parse(args(&["--uninstall", "--relaunched"]), false).relaunched);
    }

    #[test]
    fn install_steps_follow_the_options() {
        let mut options = InstallOptions { dir: "C:\\M".into(), desktop_shortcut: false, open_at_sign_in: false };
        assert_eq!(install_steps(&options, false), vec![Step::Check, Step::Unpack, Step::Copy, Step::Register]);
        options.desktop_shortcut = true;
        options.open_at_sign_in = true;
        assert_eq!(install_steps(&options, false), vec![Step::Check, Step::Unpack, Step::Copy, Step::Shortcut, Step::SignIn, Step::Register]);
        assert_eq!(install_steps(&options, true), vec![Step::Check, Step::Unpack, Step::Copy, Step::Register]);
    }

    #[test]
    fn an_update_waits_for_cilica_then_installs() {
        assert_eq!(update_steps(), vec![Step::Close, Step::Check, Step::Copy, Step::Register]);
    }

    #[test]
    fn uninstall_steps_follow_the_choice() {
        assert_eq!(uninstall_steps(UninstallOptions { delete_data: false }), vec![Step::RemoveFiles, Step::Unregister]);
        assert_eq!(
            uninstall_steps(UninstallOptions { delete_data: true }),
            vec![Step::RemoveFiles, Step::DeleteData, Step::DeleteKeys, Step::Unregister]
        );
    }

    #[test]
    fn overall_progress_counts_earlier_steps_in_full() {
        let steps = [Step::Check, Step::Copy];
        let total = Step::Check.weight() + Step::Copy.weight();
        assert_eq!(overall(&steps, Step::Check, 0.0), 0.0);
        assert!((overall(&steps, Step::Copy, 0.0) - Step::Check.weight() / total).abs() < 1e-6);
        assert!((overall(&steps, Step::Copy, 1.0) - 1.0).abs() < 1e-6);
        assert_eq!(overall(&steps, Step::Copy, 7.0), 1.0);
        assert_eq!(overall(&steps, Step::DeleteData, 0.5), 0.0);
    }

    #[test]
    fn the_copy_never_reports_done_before_the_installer_exits() {
        assert_eq!(copy_fraction(0, 100), 0.0);
        assert!((copy_fraction(50, 100) - 0.5).abs() < 1e-6);
        assert!((copy_fraction(500, 100) - 0.97).abs() < 1e-6);
        assert_eq!(copy_fraction(10, 0), 0.0);
    }

    #[test]
    fn expected_size_falls_back_to_the_payload() {
        assert_eq!(expected_install_bytes(Some(90), 10), 90);
        assert_eq!(expected_install_bytes(None, 10), 25);
        assert_eq!(expected_install_bytes(Some(0), 10), 25);
    }

    #[test]
    fn a_picked_folder_gets_its_own_cilica_folder() {
        assert_eq!(install_dir_for(Path::new("/apps")), PathBuf::from("/apps/Cilica"));
        assert_eq!(install_dir_for(Path::new("/apps/Cilica")), PathBuf::from("/apps/Cilica"));
        assert_eq!(install_dir_for(Path::new("/apps/cilica")), PathBuf::from("/apps/cilica"));
    }

    #[test]
    fn registry_paths_lose_their_quotes() {
        assert_eq!(unquote("\"C:\\Users\\a b\\Cilica\""), "C:\\Users\\a b\\Cilica");
        assert_eq!(unquote("C:\\Cilica"), "C:\\Cilica");
        assert_eq!(unquote("\""), "\"");
    }

    #[test]
    fn sizes_round_up_to_whole_megabytes() {
        assert_eq!(megabytes(0), 0);
        assert_eq!(megabytes(1), 1);
        assert_eq!(megabytes(1024 * 1024), 1);
        assert_eq!(megabytes(1024 * 1024 + 1), 2);
    }

    #[test]
    fn installer_arguments_end_with_the_unquoted_folder() {
        let dir = Path::new("C:/Users/a b/Cilica");
        assert_eq!(installer_args(dir, true, false), args(&["/S", "/D=C:/Users/a b/Cilica"]));
        assert_eq!(installer_args(dir, false, false), args(&["/S", "/NODESKTOP", "/D=C:/Users/a b/Cilica"]));
        assert_eq!(installer_args(dir, false, true), args(&["/S", "/UPDATE", "/D=C:/Users/a b/Cilica"]));
        assert_eq!(uninstaller_args(dir), args(&["/S", "_?=C:/Users/a b/Cilica"]));
    }

    #[test]
    fn the_flags_do_not_collide_with_tauris() {
        // Tauri's installer looks for these with GetOptions, which searches the whole command
        // line, ignoring case, for the text.
        for theirs in ["/P", "/NS", "/R", "/UPDATE", "/ARGS"] {
            assert!(!NO_DESKTOP_FLAG.to_uppercase().contains(theirs), "{NO_DESKTOP_FLAG} would read as {theirs}");
        }
    }

    #[test]
    fn the_login_item_is_the_quoted_exe() {
        assert_eq!(run_value(Path::new("C:/a b/Cilica.exe")), "\"C:/a b/Cilica.exe\"");
    }
}
