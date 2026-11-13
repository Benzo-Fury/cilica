//! Print who holds the microphone and what the detector would report.
//! `cargo run -p cilica-detect --example detector`
fn main() {
    println!("window titles readable: {}", cilica_detect::can_read_window_titles());
    for u in cilica_detect::mic_users(cilica_detect::can_read_window_titles()) {
        println!("mic in use: pid {} id {} -> {:?}", u.pid, u.id, cilica_detect::identify(&u));
    }
    let allow = cilica_domain::RecordingSettings::default().meeting_apps;
    println!("allow-listed meetings now: {:?}", cilica_detect::current_meetings(&allow));
}
