//! Setup on Windows. Cilica is installed and removed by Tauri's own NSIS installer and
//! uninstaller, run silently, so the files, Start menu shortcut, Installed apps entry and the
//! updater's expectations stay exactly as Tauri makes them. This file runs them (a new install,
//! an update Cilica handed over, a removal), measures them for the progress bar and does the few
//! things they cannot: the login item, deleting Cilica's
//! data and saved keys, and checking the result.
//!
//! Registry layout (written by Tauri's installer, `installMode: currentUser`):
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Cilica` holds `DisplayVersion`,
//! `InstallLocation` (quoted) and `MainBinaryName`.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::Duration;

use cilica_domain::{AppError, AppResult};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

use super::{Existing, check_folder, dir_size};
use crate::job::Reporter;
use crate::payload;
use crate::plan::{
    IDENTIFIER, InstallOptions, Launch, Mode, PRODUCT, Step, UninstallOptions, UpdateSource, copy_fraction, expected_install_bytes,
    installer_args, run_value, uninstaller_args, unquote,
};

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Cilica";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Where Settings › Apps › Startup records a login item switched off.
const STARTUP_APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const WEBVIEW2_CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
/// Setup's own name for the copy next to Cilica (`bundle.resources` in tauri.setup.conf.json).
const SETUP_EXE: &str = "cilica-setup.exe";
const DEFAULT_MAIN_EXE: &str = "Cilica.exe";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const POLL: Duration = Duration::from_millis(120);
/// How long an update waits for Cilica to quit before the installer closes it instead.
const CLOSE_WAIT: Duration = Duration::from_secs(10);

// ───────── What is on this PC ─────────

/// Whether WebView2, which draws Setup's window, is installed. Checked the way Tauri's
/// installer does: the Evergreen runtime's version under EdgeUpdate.
pub fn webview_ready() -> bool {
    let keys = [
        (LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\{WEBVIEW2_CLIENT}")),
        (LOCAL_MACHINE, format!(r"SOFTWARE\{WEBVIEW2_CLIENT}")),
        (CURRENT_USER, format!(r"Software\{WEBVIEW2_CLIENT}")),
    ];
    keys.iter().any(|(root, path)| {
        root.open(path).and_then(|key| key.get_string("pv")).is_ok_and(|v| !v.is_empty() && v != "0.0.0.0")
    })
}

/// Cilica as Installed apps lists it, if it is installed for this user.
pub fn existing() -> Option<Existing> {
    let key = CURRENT_USER.open(UNINSTALL_KEY).ok()?;
    let dir = key.get_string("InstallLocation").ok().map(|v| PathBuf::from(unquote(&v))).filter(|d| !d.as_os_str().is_empty())?;
    Some(Existing { version: key.get_string("DisplayVersion").unwrap_or_default(), dir })
}

/// Where Tauri's per-user installer puts Cilica: `%LOCALAPPDATA%\Cilica`.
pub fn default_install_dir() -> PathBuf {
    known_dir("LOCALAPPDATA").join(PRODUCT)
}

/// Cilica's notes, recordings and settings: `%APPDATA%\app.cilica.desktop`.
pub fn data_dir() -> PathBuf {
    known_dir("APPDATA").join(IDENTIFIER)
}

/// Cilica's caches and WebView2 profile: `%LOCALAPPDATA%\app.cilica.desktop`.
fn local_data_dir() -> PathBuf {
    known_dir("LOCALAPPDATA").join(IDENTIFIER)
}

fn known_dir(var: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// Cilica's own program file in `dir`, by the name its installer recorded.
fn main_exe(dir: &Path) -> PathBuf {
    let name = CURRENT_USER.open(UNINSTALL_KEY).and_then(|key| key.get_string("MainBinaryName")).unwrap_or_else(|_| DEFAULT_MAIN_EXE.to_owned());
    dir.join(name)
}

/// Whether Cilica is open from `dir` (any copy of it, when `dir` is unknown).
pub fn cilica_running(dir: Option<&Path>) -> bool {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
    let exe = dir.map(main_exe);
    system.processes().values().any(|process| match (&exe, process.exe()) {
        (Some(wanted), Some(path)) => path.as_os_str().eq_ignore_ascii_case(wanted.as_os_str()),
        _ => process.name().eq_ignore_ascii_case(DEFAULT_MAIN_EXE),
    })
}

// ───────── Install ─────────

/// Install Cilica into `options.dir`, step by step. Returns Cilica's program file.
/// `updating`: Cilica is already in `options.dir`; its shortcuts and login item stay as they are.
pub fn install(options: &InstallOptions, updating: bool, session: &Path, report: &Reporter) -> AppResult<PathBuf> {
    let dir = PathBuf::from(&options.dir);
    let expected = expected_install_bytes(payload::app_bytes(), payload::INSTALLER.len() as u64);

    report.start(Step::Check);
    check_folder(&dir, expected)?;
    report.done(Step::Check);

    report.start(Step::Unpack);
    let installer = unpack(session, |within| report.progress(Step::Unpack, within))?;
    report.done(Step::Unpack);

    report.start(Step::Copy);
    let copied = run_installer(&installer, &installer_args(&dir, options.desktop_shortcut, updating), expected, report);
    let _ = std::fs::remove_file(&installer);
    copied?;
    let exe = main_exe(&dir);
    if !exe.is_file() {
        return Err(AppError::internal(format!("Cilica's installer finished, but {} is missing. Run Setup again.", exe.display())));
    }
    report.done(Step::Copy);

    // The installer made the shortcut (or left it out for /NODESKTOP); this only shows it.
    if report.steps().contains(&Step::Shortcut) {
        report.start(Step::Shortcut);
        report.done(Step::Shortcut);
    }

    if report.steps().contains(&Step::SignIn) {
        report.start(Step::SignIn);
        open_at_sign_in(&exe)?;
        report.done(Step::SignIn);
    }

    report.start(Step::Register);
    check_listed(env!("CARGO_PKG_VERSION"))?;
    report.done(Step::Register);
    Ok(exe)
}

/// Install an update into `dir`, where Cilica already is, step by step: the installer this copy
/// carries, unpacked into `session`, or one Cilica 0.6 downloaded. Returns Cilica's program file.
/// The shortcuts and login item stay as they are.
pub fn update(dir: &Path, source: &UpdateSource, session: &Path, report: &Reporter) -> AppResult<PathBuf> {
    // Cilica quits right after starting Setup; give it a moment to finish saving.
    report.start(Step::Close);
    let started = std::time::Instant::now();
    while cilica_running(Some(dir)) && started.elapsed() < CLOSE_WAIT {
        report.progress(Step::Close, started.elapsed().as_secs_f32() / CLOSE_WAIT.as_secs_f32());
        std::thread::sleep(POLL);
    }
    // Still open: the installer closes it, as it does for any silent install.
    report.done(Step::Close);

    report.start(Step::Check);
    let installer = match &source.installer {
        Some(path) => path.clone(),
        None => unpack(session, |_| {})?,
    };
    let size = std::fs::metadata(&installer).map(|m| m.len()).map_err(|_| {
        AppError::not_found("The downloaded update is missing from the temporary folder. Open Cilica and update again from Settings › General.")
    })?;
    // What the installer writes is about what is there now; a fresh folder falls back to a guess.
    let expected = match dir_size(dir) {
        0 => expected_install_bytes(None, size),
        bytes => bytes,
    };
    check_folder(dir, size)?;
    report.done(Step::Check);

    report.start(Step::Copy);
    let copied = run_installer(&installer, &installer_args(dir, false, true), expected, report);
    if source.installer.is_none() {
        let _ = std::fs::remove_file(&installer);
    }
    copied?;
    let exe = main_exe(dir);
    if !exe.is_file() {
        return Err(AppError::internal(format!(
            "The update finished, but {} is missing. Download Cilica Setup from GitHub and run it to repair Cilica.",
            exe.display()
        )));
    }
    report.done(Step::Copy);

    report.start(Step::Register);
    let wanted = if source.version.is_empty() { None } else { Some(source.version.as_str()) };
    if let Some(version) = wanted {
        check_listed(version)?;
    }
    report.done(Step::Register);

    // Done with a download Cilica 0.6 handed over. It keeps it in a folder of its own; remove
    // that too if empty.
    if let Some(download) = &source.installer {
        let _ = std::fs::remove_file(download);
        if let Some(folder) = download.parent() {
            let _ = std::fs::remove_dir(folder);
        }
    }
    Ok(exe)
}

/// Started like Tauri's NSIS installer in silent mode: run the installer this copy carries with
/// the same `args`, without a window, and return its exit code. Cilica 0.6's copy of Setup does
/// this to the Cilica Setup it downloaded, and checks the result itself.
pub fn run_carried_installer(args: &[String], session: &Path) -> i32 {
    let installer = match unpack(session, |_| {}) {
        Ok(installer) => installer,
        Err(error) => {
            tracing::error!(error = %error.message, "could not unpack Cilica's installer");
            return 1;
        }
    };
    // NSIS reads its own command line: hand it on as it came, `/D=` (spaces and all) last.
    let status = run_raw(&installer, args, CREATE_NO_WINDOW).and_then(|mut child| child.wait());
    let _ = std::fs::remove_file(&installer);
    match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            tracing::error!(%error, "Cilica's installer didn't run");
            1
        }
    }
}

/// Run Tauri's NSIS installer silently and wait for it, moving the Copy step by the bytes it writes.
fn run_installer(installer: &Path, args: &[String], expected: u64, report: &Reporter) -> AppResult<()> {
    let child = run_raw(installer, args, CREATE_NO_WINDOW).map_err(|e| AppError::internal(format!("Cilica's installer didn't start: {e}. Try again.")))?;
    let status = watch(child, |pid, system| {
        let written = system.process(pid).map(|p| p.disk_usage().total_written_bytes).unwrap_or(0);
        report.progress(Step::Copy, copy_fraction(written, expected));
    })?;
    if status.success() { Ok(()) } else { Err(installer_stopped(status)) }
}

/// Check Installed apps lists Cilica at `version`, which the installer writes last.
fn check_listed(version: &str) -> AppResult<()> {
    let listed = CURRENT_USER.open(UNINSTALL_KEY).and_then(|key| key.get_string("DisplayVersion")).ok();
    if listed.as_deref() != Some(version) {
        return Err(AppError::internal("Cilica was installed, but Windows doesn't list it in Installed apps. Run Setup again."));
    }
    Ok(())
}

/// Write the bundled installer to `session`, a chunk at a time for the bar.
fn unpack(session: &Path, progress: impl Fn(f32)) -> AppResult<PathBuf> {
    use std::io::Write;
    let fail = |e: std::io::Error| AppError::internal(format!("Setup couldn't unpack Cilica into the temporary folder: {e}. Free up some space and try again."));
    std::fs::create_dir_all(session).map_err(fail)?;
    let path = session.join("cilica-installer.exe");
    let mut file = std::fs::File::create(&path).map_err(fail)?;
    let total = payload::INSTALLER.len().max(1);
    for (i, chunk) in payload::INSTALLER.chunks(1 << 20).enumerate() {
        file.write_all(chunk).map_err(fail)?;
        progress(((i + 1) << 20).min(total) as f32 / total as f32);
    }
    file.sync_all().map_err(fail)?;
    Ok(path)
}

/// Turn on the login item the way Cilica's autostart plugin does, and clear a "switched off"
/// mark Settings › Apps › Startup may have left from an earlier install. Cilica adopts the item
/// as its Open at login setting when it first starts.
fn open_at_sign_in(exe: &Path) -> AppResult<()> {
    CURRENT_USER
        .create(RUN_KEY)
        .and_then(|key| key.set_string(PRODUCT, run_value(exe)))
        .map_err(|e| AppError::internal(format!("Cilica couldn't be set to open when you sign in: {e}. Turn on Open at login in Cilica's settings.")))?;
    if let Ok(key) = CURRENT_USER.options().read().write().open(STARTUP_APPROVED_KEY) {
        let _ = key.remove_value(PRODUCT);
    }
    Ok(())
}

fn installer_stopped(status: ExitStatus) -> AppError {
    match status.code() {
        // NSIS exits with 2 when the script aborts, which in silent mode means Cilica was open
        // and would not close, or a newer Cilica is already installed.
        Some(2) => AppError::internal("Cilica's installer stopped. If Cilica is open, quit it from the tray and try again."),
        code => AppError::internal(format!(
            "Cilica's installer stopped before it finished (code {}). Try again.",
            code.map_or_else(|| "unknown".to_owned(), |c| c.to_string())
        )),
    }
}

// ───────── Uninstall ─────────

/// Remove Cilica from `dir`, and its data and saved keys when asked.
pub fn uninstall(dir: &Path, options: UninstallOptions, report: &Reporter) -> AppResult<()> {
    report.start(Step::RemoveFiles);
    let uninstaller = dir.join("uninstall.exe");
    if !uninstaller.is_file() {
        return Err(AppError::not_found(format!("Cilica's uninstaller is missing from {}. Delete the folder yourself in File Explorer.", dir.display())));
    }
    let before = dir_size(dir).max(1);
    let child = run_raw(&uninstaller, &uninstaller_args(dir), CREATE_NO_WINDOW)
        .map_err(|e| AppError::internal(format!("Cilica's uninstaller didn't start: {e}. Try again.")))?;
    let status = watch(child, |_, _| {
        let removed = before.saturating_sub(dir_size(dir));
        report.progress(Step::RemoveFiles, copy_fraction(removed, before));
    })?;
    if !status.success() {
        return Err(AppError::internal(format!(
            "Cilica's uninstaller stopped before it finished (code {}). If Cilica is open, quit it from the tray and try again.",
            status.code().unwrap_or(-1)
        )));
    }
    // Run with `_?=`, the uninstaller can't delete itself; Setup's copy is a resource it already removed.
    let _ = std::fs::remove_file(&uninstaller);
    let _ = std::fs::remove_file(dir.join(SETUP_EXE));
    let _ = std::fs::remove_dir(dir);
    report.done(Step::RemoveFiles);

    if options.delete_data {
        report.start(Step::DeleteData);
        remove_dir(&data_dir())?;
        report.progress(Step::DeleteData, 0.6);
        remove_dir(&local_data_dir())?;
        report.done(Step::DeleteData);

        report.start(Step::DeleteKeys);
        delete_saved_keys()?;
        report.done(Step::DeleteKeys);
    }

    report.start(Step::Unregister);
    if CURRENT_USER.open(UNINSTALL_KEY).is_ok() {
        return Err(AppError::internal("Cilica's files were removed, but Installed apps still lists it. Run Setup again to finish."));
    }
    report.done(Step::Unregister);
    Ok(())
}

fn remove_dir(dir: &Path) -> AppResult<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::internal(format!("Cilica couldn't delete {}: {e}. Delete it yourself in File Explorer.", dir.display()))),
    }
}

/// Delete every key Cilica saved in Windows Credential Manager. The keyring crate names them
/// `<account>.<service>` (`anthropic.app.cilica.desktop`, `stt.soniox.app.cilica.desktop`).
fn delete_saved_keys() -> AppResult<()> {
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{CREDENTIALW, CRED_TYPE_GENERIC, CredDeleteW, CredEnumerateW, CredFree};

    let filter = wide(&format!("*.{IDENTIFIER}"));
    let mut count = 0u32;
    let mut list: *mut *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `filter` is NUL-terminated and outlives the call; Windows fills `count` and `list`.
    let found = unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut list) } != 0;
    if !found {
        // SAFETY: reads this thread's last error, set by the call above.
        let error = unsafe { GetLastError() };
        return if error == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(AppError::internal(format!("Cilica couldn't read its saved keys (error {error}). Remove them in Credential Manager.")))
        };
    }
    let mut failed = 0;
    for i in 0..count as usize {
        // SAFETY: `list` holds `count` valid credential pointers until `CredFree` below.
        let credential = unsafe { &**list.add(i) };
        if credential.Type != CRED_TYPE_GENERIC {
            continue;
        }
        // SAFETY: `TargetName` is a NUL-terminated string owned by `list`.
        if unsafe { CredDeleteW(credential.TargetName, CRED_TYPE_GENERIC, 0) } == 0 {
            failed += 1;
        }
    }
    // SAFETY: `list` came from `CredEnumerateW` and is freed once.
    unsafe { CredFree(list.cast()) };
    if failed > 0 {
        return Err(AppError::internal(format!("Cilica couldn't remove {failed} saved key(s). Remove them in Credential Manager.")));
    }
    Ok(())
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Installed apps (to remove Cilica) and Cilica (to update itself) start the copy of Setup inside
/// Cilica's folder, which the uninstaller deletes and the installer replaces. Copy it to
/// `session` and start that copy with `args` instead; this one exits.
pub fn relaunch_from_temp(session: &Path, args: &[String]) -> AppResult<()> {
    let fail = |e: std::io::Error| AppError::internal(format!("Setup couldn't start from the temporary folder: {e}."));
    let me = std::env::current_exe().map_err(fail)?;
    std::fs::create_dir_all(session).map_err(fail)?;
    let copy = session.join("Cilica Setup.exe");
    std::fs::copy(&me, &copy).map_err(fail)?;
    Command::new(&copy).args(args).spawn().map_err(fail)?;
    Ok(())
}

/// Whether this copy of Setup sits inside an installed Cilica's folder.
pub fn runs_from_install_dir() -> bool {
    let Some(existing) = existing() else { return false };
    std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)).is_some_and(|dir| dir.as_os_str().eq_ignore_ascii_case(existing.dir.as_os_str()))
}

// ───────── Without WebView2 ─────────

/// Setup's window needs WebView2. Without it, hand over to Tauri's own wizard, which installs
/// WebView2 itself, and wait for it. Returns the exit code for Setup to exit with.
pub fn fall_back_to_wizard(launch: &Launch, session: &Path) -> i32 {
    let result = match (launch.mode, &launch.update) {
        (Mode::Install, _) => unpack(session, |_| {}).map_err(|e| e.message).and_then(|installer| {
            let status = Command::new(&installer).status().map_err(|e| e.to_string());
            let _ = std::fs::remove_file(&installer);
            status
        }),
        // What Tauri's updater would have done: the installer's own progress window, then Cilica.
        (Mode::Update, Some(update)) => match &update.installer {
            Some(installer) => Command::new(installer).args(["/P", "/UPDATE", "/R"]).status().map_err(|e| e.to_string()),
            None => unpack(session, |_| {}).map_err(|e| e.message).and_then(|installer| {
                let status = Command::new(&installer).args(["/P", "/UPDATE", "/R"]).status().map_err(|e| e.to_string());
                let _ = std::fs::remove_file(&installer);
                status
            }),
        },
        (Mode::Update, None) => Err("no update to install".to_owned()),
        (Mode::Uninstall, _) => match existing() {
            Some(existing) => Command::new(existing.dir.join("uninstall.exe")).status().map_err(|e| e.to_string()),
            None => Err("Cilica isn't installed".to_owned()),
        },
    };
    match result {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            tracing::error!(%error, "could not start the fallback wizard");
            1
        }
    }
}

// ───────── Processes ─────────

/// Open Cilica, on its own: it outlives Setup.
pub fn open(exe: &Path) -> AppResult<()> {
    let mut command = Command::new(exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .map(drop)
        .map_err(|e| AppError::internal(format!("Cilica didn't open: {e}. Open it from the Start menu.")))
}

/// After Setup exits, delete `session` (its WebView2 profile, and the copy of Setup that removed
/// Cilica, which can't delete itself while running).
pub fn clean_up_after_exit(session: &Path) {
    if !session.exists() {
        return;
    }
    let script = format!("/C ping -n 4 127.0.0.1 >nul & rmdir /S /Q \"{}\"", session.display());
    if let Err(error) = Command::new("cmd.exe").raw_arg(script).creation_flags(CREATE_NO_WINDOW).spawn() {
        tracing::warn!(%error, "could not schedule the temporary folder's removal");
    }
}

/// Start `exe` with `args` passed as written (NSIS reads its own command line).
fn run_raw(exe: &Path, args: &[String], flags: u32) -> std::io::Result<Child> {
    let mut command = Command::new(exe);
    for arg in args {
        command.raw_arg(arg);
    }
    command.creation_flags(flags).spawn()
}

/// Wait for `child`, calling `tick` with its pid and a refreshed process list as it runs.
fn watch(mut child: Child, mut tick: impl FnMut(Pid, &System)) -> AppResult<ExitStatus> {
    let pid = Pid::from_u32(child.id());
    let mut system = System::new();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => return Err(AppError::internal(format!("Setup lost track of Cilica's installer: {e}. Try again."))),
        }
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, ProcessRefreshKind::nothing().with_disk_usage());
        tick(pid, &system);
        std::thread::sleep(POLL);
    }
}
