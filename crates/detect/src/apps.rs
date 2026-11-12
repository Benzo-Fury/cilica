//! Who is holding the mic, in words a person recognises.

use crate::MicUser;

/// A recognised meeting app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identified {
    pub app_name: String,
    pub app_id: Option<String>,
}

/// (id or prefix, display name). Ids are bundle ids (macOS), lower-cased executable names
/// (Windows) and lower-cased `application.name` / binary (Linux).
const NATIVE: &[(&str, &str)] = &[
    // macOS
    ("us.zoom.xos", "Zoom"),
    ("us.zoom.", "Zoom"),
    ("com.microsoft.teams2", "Microsoft Teams"),
    ("com.microsoft.teams", "Microsoft Teams"),
    ("com.tinyspeck.slackmacgap", "Slack"),
    ("net.whatsapp.whatsapp", "WhatsApp"),
    ("desktop.whatsapp", "WhatsApp"),
    ("com.apple.facetime", "FaceTime"),
    ("com.apple.avconferenced", "FaceTime"),
    ("/usr/libexec/avconferenced", "FaceTime"),
    ("com.cisco.webexmeetingsapp", "Webex"),
    ("com.cisco.webex", "Webex"),
    ("cisco-systems.spark", "Webex"),
    ("com.hnc.discord", "Discord"),
    ("co.teamport.around", "Around"),
    ("com.skype.skype", "Skype"),
    // Windows executables
    ("zoom.exe", "Zoom"),
    ("ms-teams.exe", "Microsoft Teams"),
    ("teams.exe", "Microsoft Teams"),
    ("slack.exe", "Slack"),
    ("whatsapp.exe", "WhatsApp"),
    ("whatsapp.root.exe", "WhatsApp"),
    ("ciscocollabhost.exe", "Webex"),
    ("webexhost.exe", "Webex"),
    ("discord.exe", "Discord"),
    ("around.exe", "Around"),
    // Linux application.name / binary
    ("zoom", "Zoom"),
    ("zoom voiceengine", "Zoom"),
    ("teams-for-linux", "Microsoft Teams"),
    ("slack", "Slack"),
    ("discord", "Discord"),
    ("webex", "Webex"),
];

/// Browsers: (id prefix, name). Their meetings are recognised from window titles.
const BROWSERS: &[(&str, &str)] = &[
    ("com.google.chrome", "Chrome"),
    ("com.apple.safari", "Safari"),
    ("com.apple.webkit.gpu", "Safari"),
    ("com.microsoft.edgemac", "Edge"),
    ("company.thebrowser.browser", "Arc"),
    ("org.mozilla.firefox", "Firefox"),
    ("com.brave.browser", "Brave"),
    ("com.vivaldi.vivaldi", "Vivaldi"),
    ("com.operasoftware.opera", "Opera"),
    ("chrome.exe", "Chrome"),
    ("msedge.exe", "Edge"),
    ("firefox.exe", "Firefox"),
    ("brave.exe", "Brave"),
    ("google chrome", "Chrome"),
    ("chromium", "Chromium"),
    ("firefox", "Firefox"),
];

/// Window-title clues for meetings in a browser: (needle, display name).
const WEB_MEETINGS: &[(&str, &str)] = &[
    ("meet.google.com", "Google Meet"),
    ("google meet", "Google Meet"),
    ("meet -", "Google Meet"),
    ("microsoft teams", "Microsoft Teams"),
    ("teams.microsoft.com", "Microsoft Teams"),
    ("teams.live.com", "Microsoft Teams"),
    ("zoom meeting", "Zoom"),
    ("app.zoom.us", "Zoom"),
    ("webex", "Webex"),
    ("whereby", "Whereby"),
    ("slack huddle", "Slack"),
    ("huddle", "Slack"),
    ("discord", "Discord"),
    ("whatsapp", "WhatsApp"),
];

/// True for Margin's own processes (bundle id `app.margin.desktop`, executable `margin`).
pub fn is_margin(id: &str) -> bool {
    let id = id.to_lowercase();
    id.starts_with("app.margin") || id == "margin" || id == "margin.exe" || id == "margin-desktop" || id == "margin-desktop.exe"
}

fn lookup<'a>(table: &'a [(&'a str, &'a str)], id: &str) -> Option<&'a str> {
    table.iter().find(|(k, _)| id == *k || (k.ends_with('.') && id.starts_with(k)) || id.starts_with(&format!("{k}."))).map(|(_, v)| *v)
}

/// The meeting a mic user represents, if any.
pub fn identify(user: &MicUser) -> Option<Identified> {
    let id = user.id.to_lowercase();
    if let Some(name) = lookup(NATIVE, &id) {
        return Some(Identified { app_name: name.into(), app_id: Some(user.id.clone()) });
    }
    if lookup(BROWSERS, &id).is_some() || BROWSERS.iter().any(|(k, _)| id.starts_with(k)) {
        for title in &user.window_titles {
            let t = title.to_lowercase();
            if let Some((_, name)) = WEB_MEETINGS.iter().find(|(needle, _)| t.contains(needle)) {
                return Some(Identified { app_name: (*name).into(), app_id: Some(user.id.clone()) });
            }
        }
    }
    None
}

/// Allow-list entries are display names from Settings ("Zoom", "Google Meet", "Slack").
pub fn matches_allow_list(app_name: &str, allow: &[String]) -> bool {
    allow.iter().any(|a| a.eq_ignore_ascii_case(app_name) || (app_name == "Slack" && a.to_lowercase().starts_with("slack")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: &str, titles: &[&str]) -> MicUser {
        MicUser { pid: 1, id: id.into(), window_titles: titles.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn native_apps() {
        assert_eq!(identify(&user("us.zoom.xos", &[])).unwrap().app_name, "Zoom");
        assert_eq!(identify(&user("us.zoom.CptHost", &[])).unwrap().app_name, "Zoom");
        assert_eq!(identify(&user("com.microsoft.teams2", &[])).unwrap().app_name, "Microsoft Teams");
        assert_eq!(identify(&user("Zoom.exe", &[])).unwrap().app_name, "Zoom");
        assert_eq!(identify(&user("ZOOM VoiceEngine", &[])).unwrap().app_name, "Zoom");
        assert_eq!(identify(&user("com.apple.avconferenced", &[])).unwrap().app_name, "FaceTime");
        assert!(identify(&user("com.apple.VoiceMemos", &[])).is_none());
    }

    #[test]
    fn browsers_need_a_meeting_title() {
        assert!(identify(&user("com.google.Chrome.helper", &[])).is_none());
        assert!(identify(&user("com.google.Chrome", &["Inbox - Gmail"])).is_none());
        let meet = identify(&user("com.google.Chrome", &["Meet - abc-defg-hij"])).unwrap();
        assert_eq!(meet.app_name, "Google Meet");
        assert_eq!(identify(&user("com.apple.Safari", &["Microsoft Teams meeting | Microsoft Teams"])).unwrap().app_name, "Microsoft Teams");
    }

    #[test]
    fn allow_list_and_self() {
        let allow: Vec<String> = ["Zoom", "Google Meet", "Slack"].map(String::from).to_vec();
        assert!(matches_allow_list("zoom", &allow));
        assert!(matches_allow_list("Slack", &["Slack huddles".to_string()]));
        assert!(!matches_allow_list("Discord", &allow));
        assert!(is_margin("app.margin.desktop"));
        assert!(!is_margin("us.zoom.xos"));
    }
}
