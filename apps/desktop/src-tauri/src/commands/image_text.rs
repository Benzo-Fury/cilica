//! Finding text in note images (Settings › Writing › Find text in images). The queue lives in
//! `cilica-attachments` ([`ImageTextQueue`]); this module gives it the OS's recognizer (Apple
//! Vision on macOS, none elsewhere), keeps it in step with the setting and with recordings, wakes
//! it when images or notes change, and answers `image_text_status`.

use std::sync::Arc;
use std::time::Duration;

use cilica_attachments::{ImageTextQueue, NoteImages};
use cilica_domain::*;
use tauri::{AppHandle, Manager, State};

use crate::commands::recording::RecordingArea;
use crate::core::Core;

/// Wait this long after launch before the first look, so startup stays quick.
const STARTUP_DELAY: Duration = Duration::from_secs(5);

/// Build the queue, manage it, follow settings, recordings, imports and assistant writes, and start
/// it. Runs after the attachments and recording setups.
pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    // The queue, with the setting and the recording state as they are now
    let store = app.state::<NoteImages>().store().clone();
    let enabled = tauri::async_runtime::block_on(core.db.get_settings())?.writing.find_text_in_images;
    let queue = ImageTextQueue::new(core.db.clone(), store, recognizer(), core.events.clone(), enabled);
    if let Some(area) = app.try_state::<RecordingArea>() {
        queue.set_recording(area.service.state().phase != RecordingPhase::Idle);
    }
    app.manage(queue.clone());

    // Follow what happens elsewhere in the app
    let listener = queue.clone();
    core.on_event(move |event| match event {
        AppEvent::SettingsChanged { settings } => listener.set_enabled(settings.writing.find_text_in_images),
        AppEvent::RecordingStateChanged { state } => listener.set_recording(state.phase != RecordingPhase::Idle),
        AppEvent::ImportProgress { progress } if progress.phase == ImportPhase::Done => listener.wake(),
        AppEvent::DataChanged { scopes, .. } if scopes.contains(&DataScope::Notes) => listener.wake(),
        _ => {}
    });

    // Start after launch has settled
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        queue.run().await;
    });
    Ok(())
}

/// Look for unread images soon. Commands that save images or note bodies call this; does nothing
/// before setup.
pub fn wake(app: &AppHandle) {
    if let Some(queue) = app.try_state::<ImageTextQueue>() {
        queue.wake();
    }
}

// ---------- Commands ---------- //

/// Where finding text in images stands: supported here, on, how many images are read, and whether
/// it is reading now or waiting for a recording to end.
#[tauri::command]
#[specta::specta]
pub async fn image_text_status(queue: State<'_, ImageTextQueue>) -> AppResult<ImageTextStatus> {
    queue.status().await
}

// ---------- Recognizer ---------- //

/// The OS's on-device text recognizer: Apple Vision on macOS, none elsewhere.
fn recognizer() -> Option<Arc<dyn TextRecognizer>> {
    #[cfg(target_os = "macos")]
    {
        Some(Arc::new(vision::VisionRecognizer))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Text recognition through Apple Vision (`VNRecognizeTextRequest`), entirely on the device.
#[cfg(target_os = "macos")]
pub(crate) mod vision {
    use std::path::Path;

    use cilica_domain::TextRecognizer;
    use objc2::AnyThread;
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSArray, NSDictionary, NSLocale, NSString, NSURL};
    use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest, VNRequestTextRecognitionLevel};

    /// Reads text with Vision's accurate recogniser, language correction on, in the user's
    /// preferred languages where Vision knows them.
    pub struct VisionRecognizer;

    impl TextRecognizer for VisionRecognizer {
        fn recognize(&self, path: &Path) -> Result<String, String> {
            autoreleasepool(|_| recognize(path))
        }
    }

    /// One line of text Vision found, with where it sits (normalised, origin bottom left).
    struct Found {
        text: String,
        x: f64,
        y: f64,
        height: f64,
    }

    /// Run one request over the file and join what it found in reading order.
    fn recognize(path: &Path) -> Result<String, String> {
        // The request
        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(true);
        request.setAutomaticallyDetectsLanguage(true);
        // SAFETY: a plain property setter on a request we own.
        unsafe { request.setPreferBackgroundProcessing(true) };
        if let Some(languages) = languages(&request) {
            request.setRecognitionLanguages(&languages);
        }

        // The image, by file URL
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let options = NSDictionary::<NSString, AnyObject>::new();
        // SAFETY: the options dictionary is empty, so its value type can't be wrong.
        let handler = unsafe { VNImageRequestHandler::initWithURL_options(VNImageRequestHandler::alloc(), &url, &options) };
        let as_request: &VNRequest = &request;
        handler.performRequests_error(&NSArray::from_slice(&[as_request])).map_err(|e| e.localizedDescription().to_string())?;

        // Each observation's best candidate, with its position
        let mut found = Vec::new();
        for observation in request.results().unwrap_or_default().iter() {
            found.extend(top_candidate(&observation));
        }
        Ok(reading_order(found))
    }

    /// The best reading of one observation, if it has any text.
    fn top_candidate(observation: &VNRecognizedTextObservation) -> Option<Found> {
        let candidate = observation.topCandidates(1).firstObject()?;
        let text = candidate.string().to_string().trim().to_string();
        if text.is_empty() {
            return None;
        }
        // SAFETY: reading a struct property of a finished observation.
        let rect = unsafe { observation.boundingBox() };
        Some(Found { text, x: rect.origin.x, y: rect.origin.y, height: rect.size.height })
    }

    /// Top to bottom, then left to right. Pieces whose middles sit on the same row join with a
    /// space; rows are separated by `\n`.
    fn reading_order(mut found: Vec<Found>) -> String {
        found.sort_by(|a, b| (b.y + b.height).total_cmp(&(a.y + a.height)));
        let mut rows: Vec<Vec<Found>> = Vec::new();
        for piece in found {
            let middle = piece.y + piece.height / 2.0;
            match rows.last_mut() {
                // Same row as the row's first piece
                Some(row) if middle >= row[0].y && middle <= row[0].y + row[0].height => row.push(piece),
                _ => rows.push(vec![piece]),
            }
        }
        rows.into_iter()
            .map(|mut row| {
                row.sort_by(|a, b| a.x.total_cmp(&b.x));
                row.into_iter().map(|p| p.text).collect::<Vec<_>>().join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The user's preferred languages that Vision can read, in the user's order (matched exactly
    /// or by language, so `en-GB` picks `en-US`). `None` keeps Vision's default.
    fn languages(request: &VNRecognizeTextRequest) -> Option<Retained<NSArray<NSString>>> {
        // SAFETY: a query on a configured request; errors come back as `Err`.
        let supported: Vec<String> = unsafe { request.supportedRecognitionLanguagesAndReturnError() }.ok()?.iter().map(|l| l.to_string()).collect();
        let preferred: Vec<String> = NSLocale::preferredLanguages().iter().map(|l| l.to_string()).collect();
        let picked = pick_languages(&preferred, &supported);
        if picked.is_empty() {
            return None;
        }
        let strings: Vec<Retained<NSString>> = picked.iter().map(|l| NSString::from_str(l)).collect();
        Some(NSArray::from_retained_slice(&strings))
    }

    /// For each preferred language, the supported one that equals it, or else the first that
    /// shares its language code. Each at most once.
    pub(super) fn pick_languages(preferred: &[String], supported: &[String]) -> Vec<String> {
        let code = |tag: &str| tag.split(['-', '_']).next().unwrap_or(tag).to_ascii_lowercase();
        let mut picked: Vec<String> = Vec::new();
        for want in preferred {
            let exact = supported.iter().find(|s| s.eq_ignore_ascii_case(want));
            let by_code = || supported.iter().find(|s| code(s) == code(want));
            if let Some(found) = exact.or_else(by_code)
                && !picked.contains(found)
            {
                picked.push(found.clone());
            }
        }
        picked
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn strings(list: &[&str]) -> Vec<String> {
            list.iter().map(|s| s.to_string()).collect()
        }

        #[test]
        fn picks_preferred_languages_vision_knows() {
            let supported = strings(&["en-US", "fr-FR", "de-DE", "zh-Hans", "pt-BR"]);
            assert_eq!(pick_languages(&strings(&["en-GB", "sl-SI", "fr-CA", "en-US"]), &supported), strings(&["en-US", "fr-FR"]));
            assert_eq!(pick_languages(&strings(&["zh-Hans-CN"]), &supported), strings(&["zh-Hans"]));
            assert!(pick_languages(&strings(&["sl-SI"]), &supported).is_empty());
        }

        /// Reads a generated PNG with text in it with the real Vision recogniser.
        #[test]
        fn reads_text_in_a_real_image() {
            let dir = std::env::temp_dir().join(format!("cilica-vision-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let png = dir.join("text.png");
            render_text_png(&png, "Invoice 2041 due Friday");

            let started = std::time::Instant::now();
            let text = VisionRecognizer.recognize(&png).unwrap();
            eprintln!("Vision read {text:?} in {:?}", started.elapsed());
            let warm = std::time::Instant::now();
            assert_eq!(VisionRecognizer.recognize(&png).unwrap(), text);
            eprintln!("…and again in {:?}", warm.elapsed());
            let _ = std::fs::remove_dir_all(&dir);
            assert!(text.to_lowercase().contains("invoice"), "{text}");
            assert!(text.contains("2041"), "{text}");
        }

        /// Draw `text` black on white into a PNG with macOS's own tools (no font crate needed):
        /// an SVG converted by `sips`.
        fn render_text_png(png: &Path, text: &str) {
            let svg = png.with_extension("svg");
            std::fs::write(
                &svg,
                format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="900" height="160"><rect width="900" height="160" fill="white"/><text x="30" y="100" font-family="Helvetica" font-size="56" fill="black">{text}</text></svg>"#),
            )
            .unwrap();
            let status = std::process::Command::new("/usr/bin/sips")
                .args(["-s", "format", "png"])
                .arg(&svg)
                .arg("--out")
                .arg(png)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success() && png.is_file(), "sips could not render the test image");
        }
    }
}
