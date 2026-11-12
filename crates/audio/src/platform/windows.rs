//! Windows: system audio through WASAPI loopback on the default render device; the microphone
//! privacy switch read from the registry. Type-checked with
//! `cargo check -p margin-audio --target x86_64-pc-windows-msvc`; not yet run on Windows.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use margin_domain::{PermissionKind, PermissionStatus};
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

use crate::source::CallbackSink;
use crate::{AudioError, AudioResult, CaptureSource, RawStream, SourceKind};

/// Everything the PC plays on the default output device.
#[derive(Debug, Clone, Default)]
pub struct SystemSource;

const RATE: u32 = 48_000;
const CHANNELS: usize = 2;

/// Stops and joins the capture thread on drop.
struct LoopbackRun {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for LoopbackRun {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run_loopback(mut sink: CallbackSink, stop: Arc<AtomicBool>, ready: mpsc::Sender<Result<(), String>>) {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let _ = wasapi::initialize_mta();
        let enumerator = DeviceEnumerator::new()?;
        let device = enumerator.get_default_device(Direction::Render)?;
        let mut client = device.get_iaudioclient()?;
        let format = WaveFormat::new(32, 32, SampleType::Float, RATE as usize, CHANNELS, None);
        let (_, min_period) = client.get_device_period()?;
        let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: min_period };
        // A capture stream on a render device is a loopback stream.
        client.initialize_client(&format, Direction::Capture, mode, None)?;
        let event = client.set_get_eventhandle()?;
        let capture = client.get_audiocaptureclient()?;
        client.start_stream()?;
        let _ = ready.send(Ok(()));
        let mut queue: VecDeque<u8> = VecDeque::with_capacity(RATE as usize * CHANNELS * 4);
        let mut samples: Vec<f32> = Vec::with_capacity(RATE as usize * CHANNELS);
        while !stop.load(Ordering::SeqCst) {
            // Loopback streams get no event while nothing plays; time out and poll.
            let _ = event.wait_for_event(100);
            capture.read_from_device_to_deque(&mut queue)?;
            samples.clear();
            let (a, b) = queue.as_slices();
            let bytes: Vec<u8> = a.iter().chain(b.iter()).copied().collect();
            let whole = bytes.len() / 4 * 4;
            for c in bytes[..whole].chunks_exact(4) {
                samples.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
            }
            queue.drain(..whole);
            sink.push_interleaved(&samples, CHANNELS, |v| v);
        }
        client.stop_stream()?;
        Ok(())
    })();
    if let Err(e) = result {
        sink.status.fail(e.to_string());
        let _ = ready.send(Err(e.to_string()));
    }
}

impl CaptureSource for SystemSource {
    fn kind(&self) -> SourceKind {
        SourceKind::System
    }

    fn open(&self) -> AudioResult<RawStream> {
        let (sink, consumer, status) = CallbackSink::new();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let thread_stop = stop.clone();
        let handle = std::thread::Builder::new()
            .name("margin-loopback".into())
            .spawn(move || run_loopback(sink, thread_stop, tx))
            .map_err(AudioError::Io)?;
        let run = LoopbackRun { stop, handle: Some(handle) };
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(RawStream {
                sample_rate: RATE,
                consumer,
                status,
                device_name: "System audio".into(),
                _guard: Box::new(run),
                rate_probe: None,
            }),
            Ok(Err(e)) => Err(AudioError::Device(format!("system audio loopback: {e}"))),
            Err(_) => Err(AudioError::Device("system audio loopback did not start".into())),
        }
    }
}

// ── Permissions ──────────────────────────────────────────────────────────

/// The microphone privacy switches (Settings › Privacy & security › Microphone), as stored in
/// the consent store: the global switch and the "Let desktop apps access" switch.
fn mic_status() -> PermissionStatus {
    use ::windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    use ::windows::core::{HSTRING, PCWSTR};

    fn read(root: ::windows::Win32::System::Registry::HKEY, key: &str) -> Option<String> {
        let key = HSTRING::from(key);
        let value = HSTRING::from("Value");
        let mut buf = [0u16; 32];
        let mut len = (buf.len() * 2) as u32;
        let res = unsafe {
            RegGetValueW(
                root,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        if res.is_err() {
            return None;
        }
        let chars = (len as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())]))
    }

    const BASE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    let machine = read(HKEY_LOCAL_MACHINE, BASE);
    let user = read(HKEY_CURRENT_USER, BASE);
    let desktop = read(HKEY_CURRENT_USER, &format!(r"{BASE}\NonPackaged"));
    if [machine, user, desktop].iter().any(|v| v.as_deref() == Some("Deny")) {
        PermissionStatus::Denied
    } else {
        PermissionStatus::Granted
    }
}

pub(crate) fn permission_status(kind: PermissionKind) -> PermissionStatus {
    match kind {
        PermissionKind::Microphone => mic_status(),
        PermissionKind::SystemAudio | PermissionKind::Accessibility => PermissionStatus::NotNeeded,
    }
}

/// Windows shows no prompt for desktop apps; the status is all there is.
pub(crate) fn request_permission(kind: PermissionKind) -> PermissionStatus {
    permission_status(kind)
}

pub(crate) fn permission_settings_url(kind: PermissionKind) -> Option<&'static str> {
    match kind {
        PermissionKind::Microphone => Some("ms-settings:privacy-microphone"),
        PermissionKind::SystemAudio | PermissionKind::Accessibility => None,
    }
}

pub(crate) fn open_url(url: &str) -> std::io::Result<()> {
    // explorer.exe opens ms-settings: URIs without flashing a console window.
    std::process::Command::new("explorer.exe").arg(url).spawn().map(|_| ())
}
