//! Print who holds the microphone and what the detector would report.
//! `cargo run -p margin-detect --example detector`
fn main() {
    println!("window titles readable: {}", margin_detect::can_read_window_titles());
    for u in margin_detect::mic_users(margin_detect::can_read_window_titles()) {
        println!("mic in use: pid {} id {} -> {:?}", u.pid, u.id, margin_detect::identify(&u));
    }
    let allow = margin_domain::RecordingSettings::default().meeting_apps;
    println!("allow-listed meetings now: {:?}", margin_detect::current_meetings(&allow));
}
