//! macOS: Core Audio process objects tell us which processes are running audio input
//! (`kAudioProcessPropertyIsRunningInput`, macOS 14+). Browser meetings are recognised from
//! window titles through the Accessibility API, only when that permission is granted.

use std::ffi::{c_int, c_void};

use cidre::{ax, cf, core_audio as ca, ns};

use crate::MicUser;

unsafe extern "C" {
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
}

fn exe_path(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; 4096];
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
}

pub(crate) fn mic_users(read_window_titles: bool) -> Vec<MicUser> {
    let Ok(processes) = ca::System::processes() else { return Vec::new() };
    let mut out = Vec::new();
    for p in processes {
        if !p.is_running_input().unwrap_or(false) {
            continue;
        }
        let Ok(pid) = p.pid() else { continue };
        let id = p
            .bundle_id()
            .ok()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| exe_path(pid))
            .unwrap_or_default();
        let window_titles = if read_window_titles && is_browserish(&id) { browser_titles(&id) } else { Vec::new() };
        out.push(MicUser { pid: pid as u32, id, window_titles });
    }
    out
}

fn is_browserish(id: &str) -> bool {
    let id = id.to_lowercase();
    ["com.google.chrome", "com.apple.safari", "com.apple.webkit", "com.microsoft.edgemac", "company.thebrowser", "org.mozilla.firefox", "com.brave.browser", "com.vivaldi", "com.operasoftware"]
        .iter()
        .any(|p| id.starts_with(p))
}

/// Helpers and GPU processes hold the mic; windows belong to the main app.
fn main_bundle_ids(id: &str) -> Vec<String> {
    let lower = id.to_lowercase();
    if lower.starts_with("com.apple.webkit") || lower.starts_with("com.apple.safari") {
        return vec!["com.apple.Safari".into(), "com.apple.SafariTechnologyPreview".into()];
    }
    match id.find(".helper") {
        Some(i) => vec![id[..i].to_string()],
        None => vec![id.to_string()],
    }
}

fn browser_titles(id: &str) -> Vec<String> {
    let mut titles = Vec::new();
    for bundle in main_bundle_ids(id) {
        let apps = ns::RunningApp::with_bundle_id(&ns::String::with_str(&bundle));
        for app in apps.iter() {
            let el = ax::UiElement::with_app_pid(app.pid());
            let _ = el.set_messaging_timeout_secs(0.5);
            let Ok(windows) = el.attr_value(ax::attr::windows()) else { continue };
            if windows.get_type_id() != cf::Array::type_id() {
                continue;
            }
            let windows: &cf::ArrayOf<ax::UiElement> = unsafe { std::mem::transmute(&*windows) };
            for w in windows.iter() {
                if let Ok(t) = w.attr_value(ax::attr::title())
                    && let Some(s) = t.try_as_string()
                {
                    titles.push(s.to_string());
                }
            }
        }
    }
    titles
}
