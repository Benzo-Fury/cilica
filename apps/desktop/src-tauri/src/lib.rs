//! The Tauri shell. It only wires crates together: commands, events, windows, tray.
//! Business logic lives in `crates/*`.

pub mod commands;
pub mod core;
pub mod events;
pub mod shell;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder};

/// The typed IPC surface. Adding a command: write it in its `commands/*` module, then list it here
/// (append to your own module's block) and run `bun run bindings`.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::notes::list_notes,
            commands::notes::get_home_glance,
            commands::notes::list_trash,
            commands::notes::get_note,
            commands::notes::create_note,
            commands::notes::rename_note,
            commands::notes::update_note_body,
            commands::notes::move_note,
            commands::notes::trash_note,
            commands::notes::restore_note,
            commands::notes::delete_note_forever,
            commands::notes::discard_empty_note,
            commands::notes::empty_trash,
            commands::notes::list_categories,
            commands::notes::create_category,
            commands::notes::rename_category,
            commands::notes::reorder_category,
            commands::notes::set_category_collapsed,
            commands::notes::delete_category,
            commands::notes::search_notes,
            commands::notes::near_words,
            commands::notes::list_note_versions,
            commands::notes::restore_note_version,
            commands::notes::export_note_markdown,
            commands::notes::get_link_graph,
            commands::notes::get_backlinks,
            commands::notes::link_mention,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::get_app_info,
            commands::settings::show_main_window,
            commands::settings::title_bar_double_click,
            commands::settings::show_island,
            commands::settings::hide_island,
            commands::settings::set_island_size,
            commands::settings::get_island_info,
            commands::settings::perform_haptic,
            commands::recording::get_recording_state,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::keep_recording,
            commands::recording::list_input_devices,
            commands::recording::get_permissions,
            commands::recording::request_permission,
            commands::recording::open_permission_settings,
            commands::recording::list_stt_models,
            commands::recording::download_stt_model,
            commands::recording::cancel_stt_model_download,
            commands::recording::delete_stt_model,
            commands::recording::get_transcript,
            commands::recording::get_audio_path,
            commands::recording::rename_speaker,
            commands::recording::link_speaker,
            commands::recording::save_speaker_as_person,
            commands::recording::list_speaker_suggestions,
            commands::recording::list_people,
            commands::recording::rename_person,
            commands::recording::set_person_color,
            commands::recording::merge_people,
            commands::recording::delete_person,
            commands::recording::regenerate_summary,
            commands::assistant::list_threads,
            commands::assistant::create_thread,
            commands::assistant::delete_thread,
            commands::assistant::get_thread_messages,
            commands::assistant::send_message,
            commands::assistant::cancel_turn,
            commands::assistant::get_prompt_chips,
            commands::assistant::get_week_line,
            commands::assistant::list_proposals,
            commands::assistant::get_proposal,
            commands::assistant::apply_proposal,
            commands::assistant::dismiss_proposal,
            commands::assistant::list_ai_actions,
            commands::assistant::undo_ai_action,
            commands::assistant::undo_turn,
            commands::assistant::get_provider_status,
            commands::assistant::set_provider_key,
            commands::assistant::clear_provider_key,
            commands::assistant::detect_claude_code,
            commands::ollama::get_ollama_status,
            commands::ollama::list_local_models,
            commands::ollama::set_up_ollama,
            commands::ollama::pull_local_model,
            commands::ollama::cancel_local_model_pull,
            commands::ollama::delete_local_model,
            commands::ollama::select_local_model,
            commands::tasks::list_tasks,
            commands::tasks::create_task,
            commands::tasks::update_task,
            commands::tasks::delete_task,
            commands::tasks::run_task_now,
            commands::tasks::list_task_runs,
            commands::tasks::get_task_run,
            commands::swears::get_swear_jar,
            commands::swears::list_swear_words,
            commands::swears::add_swear_coin,
            commands::swears::empty_swear_jar,
            commands::updates::check_for_update,
            commands::updates::install_update,
            commands::updates::restart_app,
            commands::import::import_detect,
            commands::import::import_detect_all,
            commands::import::import_save_key,
            commands::import::import_clear_key,
            commands::import::import_scan,
            commands::import::import_start,
            commands::import::import_cancel,
            commands::import::import_undo,
            commands::import::import_runs,
            commands::import::note_import_source
        ])
        .events(collect_events![events::BackendEvent])
}

/// Where the generated TypeScript bindings are written.
#[cfg(debug_assertions)]
fn bindings_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/bindings.ts")
}

#[cfg(debug_assertions)]
pub fn export_bindings() {
    use specta_typescript::Typescript;
    specta_builder()
        .export(
            Typescript::default().header("// @ts-nocheck\n/* biome-ignore-all lint: generated */"),
            bindings_path(),
        )
        .expect("failed to export TypeScript bindings");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    // Bindings are regenerated by `bun run bindings`, not at launch: writing them from the app would
    // touch the source tree (often under ~/Documents) and make macOS ask for folder access.
    let builder = specta_builder();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| shell::show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(
            // Size and position of the main window only; never visibility or decorations.
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::SIZE | tauri_plugin_window_state::StateFlags::POSITION)
                .with_denylist(&[shell::island::LABEL])
                .build(),
        )
        .invoke_handler(builder.invoke_handler())
        .on_window_event(shell::on_window_event);
    #[cfg(target_os = "macos")]
    let app = app.menu(shell::menu::build);
    app.setup(move |app| {
        builder.mount_events(app);
        let handle = app.handle().clone();
        let core = tauri::async_runtime::block_on(core::Core::init(&handle))?;
        commands::settings::setup(&handle, &core)?;
        commands::notes::setup(&handle, &core)?;
        commands::recording::setup(&handle, &core)?;
        commands::assistant::setup(&handle, &core)?;
        commands::ollama::setup(&handle, &core)?;
        commands::tasks::setup(&handle, &core)?;
        commands::import::setup(&handle, &core)?;
        commands::updates::setup(&handle);
        app.manage(core);
        Ok(())
    })
    .build(tauri::generate_context!())
    .expect("error while building Margin")
    .run(shell::on_run_event);
}

#[cfg(test)]
mod tests {
    /// `bun run bindings` runs this to regenerate `src/ipc/bindings.ts` without launching the app.
    #[test]
    fn export_bindings() {
        super::export_bindings();
    }
}
