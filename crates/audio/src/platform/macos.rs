//! macOS: system audio through a Core Audio process tap (macOS 14.4+) wrapped in a private
//! aggregate device, via cidre; permissions through AVFoundation, TCC and Accessibility.

use std::ffi::{CString, c_char, c_int, c_void};
use std::sync::mpsc;
use std::time::Duration;

use cidre::core_audio::aggregate_device_keys as agg_keys;
use cidre::{arc, av, blocks, cat, cf, core_audio as ca, ns, os};
use margin_domain::{PermissionKind, PermissionStatus};

use crate::source::CallbackSink;
use crate::{AudioError, AudioResult, CaptureSource, RawStream, SourceKind};

// ── System audio ─────────────────────────────────────────────────────────

/// Everything the Mac plays, minus Margin's own process, as one mono stream.
#[derive(Debug, Clone, Default)]
pub struct SystemSource;

struct TapCtx {
    sink: CallbackSink,
    is_float: bool,
    bits: u32,
}

/// Owns the running tap. Field order is drop order: stop the device, then free the context the
/// IO proc points at, then destroy the tap.
struct TapRun {
    _device: ca::hardware::StartedDevice<ca::AggregateDevice>,
    _ctx: Box<TapCtx>,
    _tap: ca::TapGuard,
}

extern "C" fn io_proc(
    _device: ca::Device,
    _now: &cat::AudioTimeStamp,
    input: &cat::AudioBufList<1>,
    _input_time: &cat::AudioTimeStamp,
    _output: &mut cat::AudioBufList<1>,
    _output_time: &cat::AudioTimeStamp,
    ctx: Option<&mut TapCtx>,
) -> os::Status {
    let Some(ctx) = ctx else { return os::Status::NO_ERR };
    if input.number_buffers == 0 {
        return os::Status::NO_ERR;
    }
    let buf = &input.buffers[0];
    if buf.data.is_null() || buf.data_bytes_size == 0 {
        return os::Status::NO_ERR;
    }
    let channels = buf.number_channels.max(1) as usize;
    let bytes = buf.data_bytes_size as usize;
    // Core Audio buffers are aligned for their sample type.
    unsafe {
        match (ctx.is_float, ctx.bits) {
            (true, 32) => {
                let s = std::slice::from_raw_parts(buf.data as *const f32, bytes / 4);
                ctx.sink.push_interleaved(s, channels, |v| v);
            }
            (true, 64) => {
                let s = std::slice::from_raw_parts(buf.data as *const f64, bytes / 8);
                ctx.sink.push_interleaved(s, channels, |v| v as f32);
            }
            (false, 16) => {
                let s = std::slice::from_raw_parts(buf.data as *const i16, bytes / 2);
                ctx.sink.push_interleaved(s, channels, |v| v as f32 / 32_768.0);
            }
            (false, 32) => {
                let s = std::slice::from_raw_parts(buf.data as *const i32, bytes / 4);
                ctx.sink.push_interleaved(s, channels, |v| v as f32 / 2_147_483_648.0);
            }
            _ => {}
        }
    }
    os::Status::NO_ERR
}

fn ca_err(what: &str, e: os::Error) -> AudioError {
    AudioError::Device(format!("{what} failed ({e:?})"))
}

/// Process objects to leave out of the tap: Margin itself, so its sounds are never recorded.
fn excluded_processes() -> arc::R<ns::Array<ns::Number>> {
    let pid = std::process::id() as i32;
    match ca::Process::with_pid(pid) {
        Ok(p) if p.0 != ca::Obj::UNKNOWN => {
            let n = ns::Number::with_u32(p.0.0);
            ns::Array::from_slice(&[n.as_ref()])
        }
        _ => ns::Array::new(),
    }
}

/// A started tap: the run guard, its sample rate, and the ring the IO proc fills.
struct StartedTap {
    run: TapRun,
    rate: u32,
    consumer: ringbuf::HeapCons<f32>,
    status: std::sync::Arc<crate::SourceStatus>,
}

fn start_tap() -> AudioResult<StartedTap> {
    let mut desc = ca::TapDesc::with_mono_global_tap_excluding_processes(&excluded_processes());
    desc.set_private(true);
    desc.set_name(Some(ns::str!(c"Margin system audio")));
    let tap = desc.create_process_tap().map_err(|e| {
        AudioError::SystemUnsupported(format!(
            "the system audio tap could not be created ({e:?}); it needs macOS 14.4 or later"
        ))
    })?;
    let uid = tap.uid().map_err(|e| ca_err("Reading the tap id", e))?;
    let asbd = tap.asbd().map_err(|e| ca_err("Reading the tap format", e))?;

    let sub_tap = cf::DictionaryOf::with_keys_values(&[ca::sub_device_keys::uid()], &[uid.as_type_ref()]);
    let agg_uid = cf::Uuid::new().to_cf_string();
    let agg_desc = cf::DictionaryOf::with_keys_values(
        &[agg_keys::is_private(), agg_keys::tap_auto_start(), agg_keys::name(), agg_keys::uid(), agg_keys::tap_list()],
        &[
            cf::Boolean::value_true().as_type_ref(),
            cf::Boolean::value_false(),
            cf::String::from_str("Margin system audio").as_ref(),
            &agg_uid,
            &cf::ArrayOf::from_slice(&[sub_tap.as_ref()]),
        ],
    );
    let device = ca::AggregateDevice::with_desc(&agg_desc).map_err(|e| ca_err("Creating the aggregate device", e))?;

    let (sink, consumer, status) = CallbackSink::new();
    let is_float = asbd.format_flags.0 & cat::audio::FormatFlags::IS_FLOAT.0 != 0;
    let mut ctx = Box::new(TapCtx { sink, is_float, bits: asbd.bits_per_channel });
    let proc_id = device
        .create_io_proc_id(io_proc, Some(&mut *ctx))
        .map_err(|e| ca_err("Attaching to the system audio tap", e))?;
    let started = ca::device_start(device, Some(proc_id)).map_err(|e| ca_err("Starting the system audio tap", e))?;
    let rate = asbd.sample_rate.round() as u32;
    let run = TapRun { _device: started, _ctx: ctx, _tap: tap };
    Ok(StartedTap { run, rate, consumer, status })
}

impl CaptureSource for SystemSource {
    fn kind(&self) -> SourceKind {
        SourceKind::System
    }

    fn open(&self) -> AudioResult<RawStream> {
        if system_audio_status() == PermissionStatus::Denied {
            return Err(AudioError::SystemPermissionDenied);
        }
        let t = start_tap()?;
        let tap_obj: &ca::Obj = &t.run._tap;
        let tap_id = tap_obj.0;
        let probe = move || -> Option<u32> {
            let asbd: cat::AudioStreamBasicDesc = ca::Obj(tap_id).prop(&ca::PropSelector::TAP_FORMAT.global_addr()).ok()?;
            Some(asbd.sample_rate.round() as u32)
        };
        Ok(RawStream {
            sample_rate: t.rate,
            consumer: t.consumer,
            status: t.status,
            device_name: "System audio".into(),
            _guard: Box::new(t.run),
            rate_probe: Some(Box::new(probe)),
        })
    }
}

// ── Permissions ──────────────────────────────────────────────────────────

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_NOW: c_int = 2;

type TccPreflight = unsafe extern "C" fn(service: *const c_void, options: *const c_void) -> c_int;

/// The System Audio Recording permission has no public query API. Like Apple's own sample code
/// and other tap-based recorders, ask TCC directly (private `TCCAccessPreflight`): 0 granted,
/// 1 denied, anything else not decided yet. Returns `NotDetermined` if TCC cannot be loaded.
fn system_audio_status() -> PermissionStatus {
    unsafe {
        let path = CString::new("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC").unwrap();
        let handle = dlopen(path.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            return PermissionStatus::NotDetermined;
        }
        let sym = CString::new("TCCAccessPreflight").unwrap();
        let f = dlsym(handle, sym.as_ptr());
        if f.is_null() {
            return PermissionStatus::NotDetermined;
        }
        let preflight: TccPreflight = std::mem::transmute(f);
        let service = cf::String::from_str("kTCCServiceAudioCapture");
        let service_ptr = service.as_ref() as *const cf::String as *const c_void;
        match preflight(service_ptr, std::ptr::null()) {
            0 => PermissionStatus::Granted,
            1 => PermissionStatus::Denied,
            _ => PermissionStatus::NotDetermined,
        }
    }
}

fn mic_status() -> PermissionStatus {
    match av::CaptureDevice::authorization_status_for_media_type(av::MediaType::audio()) {
        Ok(av::AuthorizationStatus::Authorized) => PermissionStatus::Granted,
        Ok(av::AuthorizationStatus::Denied) | Ok(av::AuthorizationStatus::Restricted) => PermissionStatus::Denied,
        Ok(av::AuthorizationStatus::NotDetermined) => PermissionStatus::NotDetermined,
        Err(_) => PermissionStatus::NotDetermined,
    }
}

pub(crate) fn permission_status(kind: PermissionKind) -> PermissionStatus {
    match kind {
        PermissionKind::Microphone => mic_status(),
        PermissionKind::SystemAudio => system_audio_status(),
        PermissionKind::Accessibility => {
            if cidre::ax::is_process_trusted() {
                PermissionStatus::Granted
            } else {
                // macOS does not distinguish "never asked" from "denied" here.
                PermissionStatus::NotDetermined
            }
        }
    }
}

/// Show the OS prompt where one exists and wait (up to two minutes) for the answer.
pub(crate) fn request_permission(kind: PermissionKind) -> PermissionStatus {
    match kind {
        PermissionKind::Microphone => {
            if mic_status() != PermissionStatus::NotDetermined {
                return mic_status();
            }
            let (tx, rx) = mpsc::channel::<bool>();
            let mut block = blocks::SendBlock::<fn(bool)>::new1(move |granted: bool| {
                let _ = tx.send(granted);
            });
            if av::CaptureDevice::request_access_for_media_type_ch(av::MediaType::audio(), &mut block).is_err() {
                return mic_status();
            }
            match rx.recv_timeout(Duration::from_secs(120)) {
                Ok(true) => PermissionStatus::Granted,
                Ok(false) => PermissionStatus::Denied,
                Err(_) => mic_status(),
            }
        }
        PermissionKind::SystemAudio => {
            // The prompt appears the first time a tap starts delivering audio. Run one briefly.
            if system_audio_status() == PermissionStatus::NotDetermined
                && let Ok(tap) = start_tap()
            {
                std::thread::sleep(Duration::from_millis(500));
                drop(tap);
            }
            system_audio_status()
        }
        PermissionKind::Accessibility => {
            if cidre::ax::is_process_trusted_with_prompt(true) {
                PermissionStatus::Granted
            } else {
                PermissionStatus::NotDetermined
            }
        }
    }
}

pub(crate) fn permission_settings_url(kind: PermissionKind) -> Option<&'static str> {
    Some(match kind {
        PermissionKind::Microphone => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        PermissionKind::SystemAudio => "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
        PermissionKind::Accessibility => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    })
}

pub(crate) fn open_url(url: &str) -> std::io::Result<()> {
    std::process::Command::new("/usr/bin/open").arg(url).status().and_then(|s| {
        if s.success() { Ok(()) } else { Err(std::io::Error::other(format!("open exited with {s}"))) }
    })
}
