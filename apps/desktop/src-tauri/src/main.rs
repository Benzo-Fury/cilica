// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--mcp`: serve the assistant's tools over stdio for the Claude Code bridge, then exit.
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        std::process::exit(cilica_desktop_lib::commands::assistant::run_mcp_server());
    }
    cilica_desktop_lib::run()
}
