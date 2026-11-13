//! Cilica Setup: Cilica's installer and uninstaller window on Windows.
//!
//! One small program, and the only thing Cilica ships on Windows (docs/architecture.md › Windows
//! installer):
//! - **Install.** `Cilica_<version>_x64_Setup.exe` carries Tauri's NSIS installer (src/payload.rs)
//!   and runs it silently under a frameless window drawn by the desktop frontend
//!   (`index.html?window=installer`, src/features/installer).
//! - **Update.** The same file is the updater's package. Cilica downloads and verifies it, then
//!   starts it with `--update --version <v>` (apps/desktop/src-tauri/src/commands/updates); it
//!   waits for Cilica to quit, runs the installer it carries silently, then opens Cilica again.
//!   Older copies of Cilica start it the ways they knew: Tauri's updater with `/P /UPDATE /R`
//!   (the same window), and Cilica 0.6 through its own copy of Setup with NSIS's silent line,
//!   which is passed on to the carried installer as it came.
//! - **Uninstall.** A copy without the installer sits next to Cilica as `cilica-setup.exe`;
//!   Installed apps starts it with `--uninstall` (apps/desktop/src-tauri/windows/hooks.nsh). It
//!   moves itself to the temporary folder, then runs Cilica's own uninstaller silently.
//!
//! Without WebView2 it hands over to Tauri's NSIS wizard, which installs WebView2 itself.

pub mod commands;
pub mod job;
pub mod payload;
pub mod plan;
pub mod system;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::commands::Setup;
use crate::plan::{Launch, Mode};

/// The one window's label.
pub const WINDOW: &str = "installer";
/// Logical size. Fixed: the art and the three screens are laid out for it.
const SIZE: (f64, f64) = (600.0, 480.0);

/// The typed IPC surface. Adding a command: write it in `commands.rs`, list it here, then run
/// `bun run bindings`.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::get_setup_info,
            commands::show_window,
            commands::choose_folder,
            commands::install,
            commands::update,
            commands::uninstall,
            commands::open_cilica,
            commands::quit,
        ])
        .events(collect_events![job::SetupEvent])
}

#[cfg(debug_assertions)]
pub fn export_bindings() {
    use specta_typescript::Typescript;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../desktop/src/ipc/installer/bindings.ts");
    specta_builder()
        .export(Typescript::default().header("// @ts-nocheck\n/* biome-ignore-all lint: generated */"), path)
        .expect("failed to export the installer's TypeScript bindings");
}

/// Run Setup. Returns the process exit code.
pub fn run() -> i32 {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let launch = Launch::parse(std::env::args().skip(1), payload::present());
    let session = system::session_dir(launch.relaunched);

    // Asked to be Tauri's NSIS installer, silently: be exactly that, with no window.
    if let Some(args) = &launch.silent {
        let code = system::run_carried_installer(args, &session);
        system::clean_up_after_exit(&session);
        return code;
    }

    // Removing or updating Cilica deletes or replaces the copy of Setup in its folder, which is
    // this one: continue from a copy elsewhere.
    if launch.mode != Mode::Install && !launch.relaunched && system::runs_from_install_dir() {
        match system::relaunch_from_temp(&session, &launch.relaunch_args()) {
            Ok(()) => return 0,
            Err(error) => tracing::warn!(error = %error.message, "working from Cilica's own folder instead"),
        }
    }
    if !system::webview_ready() {
        let code = system::fall_back_to_wizard(&launch, &session);
        system::clean_up_after_exit(&session);
        return code;
    }

    let builder = specta_builder();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .manage(Setup::new(launch.clone(), session.clone()))
        .on_window_event(|window, event| {
            // Stopping the installer halfway leaves Cilica broken: the close button waits.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.state::<Setup>().busy.get()
            {
                api.prevent_close();
            }
        })
        .setup(move |app| {
            builder.mount_events(app);
            let title = match launch.mode {
                Mode::Install | Mode::Update => "Cilica Setup",
                Mode::Uninstall => "Uninstall Cilica",
            };
            WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html?window=installer".into()))
                .title(title)
                .inner_size(SIZE.0, SIZE.1)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .shadow(true)
                .center()
                .visible(false)
                // A profile of its own in the temporary folder, deleted after Setup exits, so
                // Setup leaves nothing in AppData.
                .data_directory(session.join("webview"))
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            tracing::error!(%error, "could not start Setup's window");
            return 1;
        }
    };
    let cleanup = app.state::<Setup>().session.clone();
    let download = (launch.mode == Mode::Update).then(downloaded_from).flatten();
    app.run(move |_, event| {
        if let RunEvent::Exit = event {
            system::clean_up_after_exit(&cleanup);
            // An update runs from the folder Cilica downloaded it to, which it can't delete
            // while it runs.
            if let Some(dir) = &download {
                system::clean_up_after_exit(dir);
            }
        }
    });
    0
}

/// The folder Cilica downloads updates to (`STAGING_DIR` in
/// apps/desktop/src-tauri/src/commands/updates/windows.rs), when this copy runs from it.
fn downloaded_from() -> Option<std::path::PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    (dir.file_name()? == "cilica-update").then_some(dir)
}

#[cfg(test)]
mod tests {
    /// `bun run bindings` runs this to regenerate the installer's bindings without opening Setup.
    #[test]
    fn export_bindings() {
        super::export_bindings();
    }
}
