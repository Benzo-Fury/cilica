//! Parsing `pactl list source-outputs` (Linux detection). Kept platform-independent so it is
//! tested everywhere.

use crate::MicUser;

#[allow(dead_code)]
/// Parse `pactl list source-outputs`, skipping paused (corked) streams and monitor captures.
pub(crate) fn parse(text: &str) -> Vec<MicUser> {
    let mut out = Vec::new();
    for block in text.split("Source Output #").skip(1) {
        let prop = |key: &str| {
            block.lines().find_map(|l| {
                let l = l.trim();
                l.strip_prefix(key).and_then(|r| r.trim_start().strip_prefix('=')).map(|v| v.trim().trim_matches('"').to_string())
            })
        };
        let corked = block.lines().any(|l| l.trim() == "Corked: yes");
        let monitor = block.lines().any(|l| l.trim().starts_with("Source:") && l.contains(".monitor"));
        if corked || monitor {
            continue;
        }
        let pid = prop("application.process.id").and_then(|p| p.parse().ok()).unwrap_or(0);
        let binary = prop("application.process.binary");
        let name = prop("application.name");
        let id = match (binary, name) {
            (Some(b), Some(n)) if crate::apps::identify(&MicUser { pid, id: n.clone(), window_titles: vec![] }).is_some() => {
                let _ = b;
                n
            }
            (Some(b), _) => b,
            (None, Some(n)) => n,
            (None, None) => continue,
        };
        out.push(MicUser { pid, id, window_titles: Vec::new() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"Source Output #42
	Driver: PipeWire
	Owner Module: n/a
	Client: 77
	Source: 55
	Corked: no
	Properties:
		application.name = "ZOOM VoiceEngine"
		application.process.id = "4242"
		application.process.binary = "zoom"
Source Output #43
	Source: 56
	Corked: yes
	Properties:
		application.name = "Firefox"
		application.process.id = "777"
Source Output #44
	Source: alsa_output.pci.monitor
	Corked: no
	Properties:
		application.name = "OBS"
"#;

    #[test]
    fn parses_active_capture_streams() {
        let users = parse(SAMPLE);
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].pid, 4242);
        assert_eq!(crate::identify(&users[0]).unwrap().app_name, "Zoom");
    }
}
