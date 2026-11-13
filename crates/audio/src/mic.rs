//! The microphone, through cpal on every platform.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use cilica_domain::AudioDevice;

use crate::source::CallbackSink;
use crate::{AudioError, AudioResult, CaptureSource, RawStream, SourceKind};

/// The microphone. `device_id` is an id from [`list_input_devices`]; `None` follows the system
/// default input (the capture session reopens it when the default changes).
#[derive(Debug, Clone, Default)]
pub struct MicSource {
    pub device_id: Option<String>,
}

impl MicSource {
    pub fn new(device_id: Option<String>) -> Self {
        Self { device_id }
    }
}

/// The id of the current default input device, if there is one.
pub fn default_input_id() -> Option<String> {
    cpal::default_host().default_input_device().and_then(|d| d.id().ok()).map(|id| id.to_string())
}

fn device_name(device: &cpal::Device) -> String {
    device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| device.to_string())
}

/// Every input device, the default first.
pub fn list_input_devices() -> AudioResult<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let default_id = default_input_id();
    let devices = host.input_devices().map_err(|e| AudioError::Device(e.to_string()))?;
    let mut out: Vec<AudioDevice> = devices
        .filter_map(|d| {
            let id = d.id().ok()?.to_string();
            Some(AudioDevice { is_default: Some(&id) == default_id.as_ref(), name: device_name(&d), id })
        })
        .collect();
    out.sort_by_key(|d| !d.is_default);
    Ok(out)
}

fn find_device(device_id: Option<&str>) -> AudioResult<cpal::Device> {
    let host = cpal::default_host();
    match device_id {
        Some(id) => {
            let parsed: Option<cpal::DeviceId> = id.parse().ok();
            parsed
                .and_then(|pid| host.device_by_id(&pid))
                .or_else(|| {
                    // Ids from older builds or other hosts: fall back to matching the string form.
                    host.input_devices().ok()?.find(|d| d.id().map(|i| i.to_string() == id).unwrap_or(false))
                })
                .ok_or_else(|| AudioError::DeviceNotFound(id.to_string()))
        }
        None => host.default_input_device().ok_or(AudioError::NoInputDevice),
    }
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, mut sink: CallbackSink) -> AudioResult<cpal::Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let status = sink.status.clone();
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| sink.push_interleaved(data, channels, <f32 as FromSample<T>>::from_sample_),
            move |err: cpal::Error| {
                if err.kind() != cpal::ErrorKind::Xrun {
                    status.fail(err.to_string());
                }
            },
            Some(std::time::Duration::from_secs(5)),
        )
        .map_err(map_cpal_error)
}

fn map_cpal_error(e: cpal::Error) -> AudioError {
    match e.kind() {
        cpal::ErrorKind::PermissionDenied => AudioError::MicPermissionDenied,
        cpal::ErrorKind::DeviceNotAvailable => AudioError::Device("the microphone was disconnected".into()),
        _ => AudioError::Device(e.to_string()),
    }
}

impl CaptureSource for MicSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Mic
    }

    fn open(&self) -> AudioResult<RawStream> {
        let device = find_device(self.device_id.as_deref())?;
        let supported = device.default_input_config().map_err(map_cpal_error)?;
        let config = supported.config();
        let (sink, consumer, status) = CallbackSink::new();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, sink)?,
            SampleFormat::I16 => build::<i16>(&device, config, sink)?,
            SampleFormat::I32 => build::<i32>(&device, config, sink)?,
            SampleFormat::U16 => build::<u16>(&device, config, sink)?,
            SampleFormat::F64 => build::<f64>(&device, config, sink)?,
            SampleFormat::I8 => build::<i8>(&device, config, sink)?,
            SampleFormat::U8 => build::<u8>(&device, config, sink)?,
            other => return Err(AudioError::Device(format!("the microphone uses an unsupported format ({other:?})"))),
        };
        stream.play().map_err(map_cpal_error)?;
        Ok(RawStream {
            sample_rate: config.sample_rate,
            consumer,
            status,
            device_name: device_name(&device),
            _guard: Box::new(stream),
            rate_probe: None,
        })
    }
}
