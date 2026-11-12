//! Linux: system audio through a PipeWire capture stream on the monitor of the default sink.
//! Written against pipewire-rs 0.10 but not compiled here (needs libpipewire headers); treat as
//! unverified until it is built and run on a Linux machine.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use margin_domain::{PermissionKind, PermissionStatus};
use pipewire as pw;
use pw::spa;
use pw::spa::pod::Pod;

use crate::source::CallbackSink;
use crate::{AudioError, AudioResult, CaptureSource, RawStream, SourceKind};

/// Everything the desktop plays on the default sink.
#[derive(Debug, Clone, Default)]
pub struct SystemSource;

const RATE: u32 = 48_000;
const CHANNELS: u32 = 2;

struct PipewireRun {
    quit: Option<pw::channel::Sender<()>>,
    stopped: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for PipewireRun {
    fn drop(&mut self) {
        if let Some(tx) = self.quit.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.stopped.store(true, Ordering::SeqCst);
    }
}

fn run(sink: CallbackSink, quit_rx: pw::channel::Receiver<()>, ready: mpsc::Sender<Result<(), String>>) {
    let status = sink.status.clone();
    let result = (|| -> Result<(), pw::Error> {
        pw::init();
        let mainloop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&mainloop, None)?;
        let core = context.connect_rc(None)?;

        let mut props = pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Communication",
            *pw::keys::APP_NAME => "Margin",
        };
        // Capture the monitor of the default sink instead of a source.
        props.insert("stream.capture.sink", "true");
        let stream = pw::stream::StreamBox::new(&core, "margin-system-audio", props)?;

        let _listener = stream
            .add_local_listener_with_user_data(sink)
            .process(|stream, sink| {
                if let Some(mut buffer) = stream.dequeue_buffer() {
                    let datas = buffer.datas_mut();
                    if let Some(data) = datas.first_mut() {
                        let size = data.chunk().size() as usize;
                        if let Some(bytes) = data.data() {
                            let bytes = &bytes[..size.min(bytes.len())];
                            // F32LE interleaved, as negotiated below.
                            let samples: &[f32] = unsafe {
                                std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / 4)
                            };
                            sink.push_interleaved(samples, CHANNELS as usize, |v| v);
                        }
                    }
                }
            })
            .state_changed(|_, sink, _, new| {
                if let pw::stream::StreamState::Error(e) = new {
                    sink.status.fail(e);
                }
            })
            .register()?;

        let mut info = spa::param::audio::AudioInfoRaw::new();
        info.set_format(spa::param::audio::AudioFormat::F32LE);
        info.set_rate(RATE);
        info.set_channels(CHANNELS);
        let obj = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: spa::param::ParamType::EnumFormat.as_raw(),
            properties: info.into(),
        };
        let values: Vec<u8> =
            spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(obj))
                .map_err(|_| pw::Error::CreationFailed)?
                .0
                .into_inner();
        let mut params = [Pod::from_bytes(&values).ok_or(pw::Error::CreationFailed)?];
        stream.connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )?;

        let ml = mainloop.clone();
        let _quit = quit_rx.attach(mainloop.loop_(), move |_| ml.quit());
        let _ = ready.send(Ok(()));
        mainloop.run();
        Ok(())
    })();
    if let Err(e) = result {
        status.fail(e.to_string());
        let _ = ready.send(Err(e.to_string()));
    }
}

impl CaptureSource for SystemSource {
    fn kind(&self) -> SourceKind {
        SourceKind::System
    }

    fn open(&self) -> AudioResult<RawStream> {
        let (sink, consumer, status) = CallbackSink::new();
        let (quit_tx, quit_rx) = pw::channel::channel::<()>();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("margin-pipewire".into())
            .spawn(move || run(sink, quit_rx, tx))
            .map_err(AudioError::Io)?;
        let guard = PipewireRun { quit: Some(quit_tx), stopped: Arc::new(AtomicBool::new(false)), handle: Some(handle) };
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(RawStream {
                sample_rate: RATE,
                consumer,
                status,
                device_name: "System audio".into(),
                _guard: Box::new(guard),
                rate_probe: None,
            }),
            Ok(Err(e)) => Err(AudioError::SystemUnsupported(format!("PipeWire is not available ({e})"))),
            Err(_) => Err(AudioError::SystemUnsupported("PipeWire did not answer".into())),
        }
    }
}

pub(crate) fn permission_status(_kind: PermissionKind) -> PermissionStatus {
    PermissionStatus::NotNeeded
}

pub(crate) fn request_permission(kind: PermissionKind) -> PermissionStatus {
    permission_status(kind)
}

pub(crate) fn permission_settings_url(_kind: PermissionKind) -> Option<&'static str> {
    None
}

pub(crate) fn open_url(url: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open").arg(url).spawn().map(|_| ())
}
