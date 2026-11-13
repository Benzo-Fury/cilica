//! The WebSocket side shared by the streaming providers: connect with an auth header, send audio
//! in 100 ms frames, keep the connection alive through silence, end the audio the provider's way
//! and wait for its last results. A dropped connection is reopened once; the provider's times
//! restart at zero on the new connection, so each session carries the offset of its first audio.
//!
//! TLS names its crypto backend (aws-lc-rs) instead of using rustls' process default: the app
//! links two backends (another dependency brings `ring`), and with two the default cannot be
//! chosen and rustls panics on the first connection.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, sleep_until};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::Connector;

use super::link::{Audio, Emitter, pcm_bytes};
use crate::{SAMPLE_RATE, SttError, SttResult};

/// Audio frame sent to providers: 100 ms.
const FRAME: usize = SAMPLE_RATE as usize / 10;
/// Shortest frame any provider accepts (AssemblyAI: 20 ms).
const MIN_FRAME: usize = SAMPLE_RATE as usize / 50;
/// Keepalive after this much time without audio.
const KEEPALIVE: Duration = Duration::from_secs(5);
/// How long to wait for the last results after the audio ended.
const DRAIN: Duration = Duration::from_secs(8);

/// One provider's streaming protocol.
pub(crate) trait Protocol: Send {
    fn name(&self) -> &str;
    /// URL and headers for a new connection.
    fn request(&self) -> SttResult<(String, Vec<(&'static str, String)>)>;
    /// Text frames to send right after connecting (configuration).
    fn start(&self) -> Vec<Message> {
        Vec::new()
    }
    /// How to say the audio has ended.
    fn end(&mut self) -> Vec<Message>;
    fn keepalive(&self) -> Option<Message> {
        None
    }
    /// A text frame from the provider. `offset_ms` is where this connection's time zero sits on
    /// the stream. Returns true when the provider says it has sent everything.
    fn on_text(&mut self, text: &str, offset_ms: u32, out: &Emitter) -> SttResult<bool>;
    /// The connection is gone: turn whatever is still pending into finals.
    fn flush(&mut self, out: &Emitter);
    /// A close frame from the provider; returns the error it stands for, if any.
    fn on_close(&self, _code: CloseCode, _reason: &str) -> Option<SttError> {
        None
    }
}

/// TLS for a provider: this computer's trusted certificates, with an explicit crypto backend.
pub(crate) fn client_config(roots: rustls::RootCertStore) -> SttResult<Arc<rustls::ClientConfig>> {
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| SttError::Provider(format!("Cilica could not set up a secure connection: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// [`client_config`] with the system's root certificates, loaded once.
pub(crate) fn native_tls() -> SttResult<Arc<rustls::ClientConfig>> {
    static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let loaded = rustls_native_certs::load_native_certs();
            if !loaded.errors.is_empty() {
                tracing::warn!(errors = ?loaded.errors, "some system certificates could not be read");
            }
            let mut roots = rustls::RootCertStore::empty();
            let (added, _) = roots.add_parsable_certificates(loaded.certs);
            if added == 0 {
                return Err("Cilica could not read this computer's trusted certificates, so it cannot reach the transcription service securely.".to_string());
            }
            client_config(roots).map_err(|e| e.to_string())
        })
        .clone()
        .map_err(SttError::Provider)
}

/// Run a provider until the audio ends, reconnecting once if the connection drops.
pub(crate) async fn run<P: Protocol>(protocol: P, audio: UnboundedReceiver<Audio>, out: Emitter) -> SttResult<()> {
    let tls = native_tls()?;
    run_with(protocol, audio, out, tls).await
}

/// [`run`] with the given TLS settings (tests trust their own certificate).
pub(crate) async fn run_with<P: Protocol>(mut protocol: P, mut audio: UnboundedReceiver<Audio>, out: Emitter, tls: Arc<rustls::ClientConfig>) -> SttResult<()> {
    let mut sent = 0u64; // samples already sent, across connections
    let mut pending: Vec<i16> = Vec::new();
    let mut ended = false;
    for attempt in 0..2 {
        let offset_ms = (sent * 1000 / SAMPLE_RATE as u64) as u32;
        match session(&mut protocol, &mut audio, &out, &tls, &mut sent, &mut pending, &mut ended, offset_ms).await {
            Ok(()) => return Ok(()),
            Err(SttError::Network(name)) if attempt == 0 && !ended => {
                tracing::warn!(provider = %name, "the transcription connection dropped; reconnecting");
                protocol.flush(&out);
                continue;
            }
            Err(e) => {
                protocol.flush(&out);
                return Err(e);
            }
        }
    }
    Err(SttError::Network(protocol.name().to_string()))
}

#[allow(clippy::too_many_arguments)]
async fn session<P: Protocol>(
    protocol: &mut P,
    audio: &mut UnboundedReceiver<Audio>,
    out: &Emitter,
    tls: &Arc<rustls::ClientConfig>,
    sent: &mut u64,
    pending: &mut Vec<i16>,
    ended: &mut bool,
    offset_ms: u32,
) -> SttResult<()> {
    let name = protocol.name().to_string();
    let (url, headers) = protocol.request()?;
    let mut request = url.as_str().into_client_request().map_err(|e| SttError::Provider(format!("{name}: {e}")))?;
    for (key, value) in headers {
        let value = HeaderValue::from_str(&value).map_err(|_| SttError::Auth(name.clone()))?;
        request.headers_mut().insert(key, value);
    }
    let connector = Connector::Rustls(tls.clone());
    let (ws, _) = tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
        .await
        .map_err(|e| handshake_error(&name, e))?;
    let (mut tx, mut rx) = ws.split();
    for m in protocol.start() {
        tx.send(m).await.map_err(|_| SttError::Network(name.clone()))?;
    }

    let mut last_audio = Instant::now();
    let mut drain_until: Option<Instant> = None;
    if *ended {
        // The audio ended while we were reconnecting: send what is left and finish.
        send_frames(&mut tx, pending, true, sent, &name).await?;
        for m in protocol.end() {
            tx.send(m).await.map_err(|_| SttError::Network(name.clone()))?;
        }
        drain_until = Some(Instant::now() + DRAIN);
    }
    loop {
        let wake = drain_until.unwrap_or(last_audio + KEEPALIVE);
        tokio::select! {
            item = audio.recv(), if drain_until.is_none() => {
                match item {
                    Some(Audio::Pcm(pcm)) => {
                        pending.extend_from_slice(&pcm);
                        send_frames(&mut tx, pending, false, sent, &name).await?;
                        last_audio = Instant::now();
                    }
                    Some(Audio::End) | None => {
                        *ended = true;
                        send_frames(&mut tx, pending, true, sent, &name).await?;
                        for m in protocol.end() {
                            tx.send(m).await.map_err(|_| SttError::Network(name.clone()))?;
                        }
                        drain_until = Some(Instant::now() + DRAIN);
                    }
                }
            }
            message = rx.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if protocol.on_text(text.as_str(), offset_ms, out)? {
                            protocol.flush(out);
                            let _ = tx.send(Message::Close(None)).await;
                            return Ok(());
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        if let Some(f) = &frame
                            && let Some(error) = protocol.on_close(f.code, f.reason.as_str())
                        {
                            return Err(error);
                        }
                        if *ended {
                            protocol.flush(out);
                            return Ok(());
                        }
                        return Err(SttError::Network(name));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => {
                        if *ended {
                            protocol.flush(out);
                            return Ok(());
                        }
                        return Err(SttError::Network(name));
                    }
                }
            }
            _ = sleep_until(wake) => {
                if drain_until.is_some() {
                    // The provider did not confirm the end in time: keep what arrived.
                    protocol.flush(out);
                    return Ok(());
                }
                if let Some(m) = protocol.keepalive() {
                    tx.send(m).await.map_err(|_| SttError::Network(name.clone()))?;
                }
                last_audio = Instant::now();
            }
        }
    }
}

/// Send whole 100 ms frames from `pending`; at the end, the remainder too (padded to the
/// shortest frame providers accept).
async fn send_frames<S>(tx: &mut S, pending: &mut Vec<i16>, last: bool, sent: &mut u64, name: &str) -> SttResult<()>
where
    S: futures::Sink<Message> + Unpin,
{
    let mut start = 0;
    while pending.len() - start >= FRAME {
        tx.send(Message::Binary(pcm_bytes(&pending[start..start + FRAME]).into())).await.map_err(|_| SttError::Network(name.to_string()))?;
        start += FRAME;
        *sent += FRAME as u64;
    }
    pending.drain(..start);
    if last && !pending.is_empty() {
        let mut frame = std::mem::take(pending);
        if frame.len() < MIN_FRAME {
            frame.resize(MIN_FRAME, 0);
        }
        *sent += frame.len() as u64;
        tx.send(Message::Binary(pcm_bytes(&frame).into())).await.map_err(|_| SttError::Network(name.to_string()))?;
    }
    Ok(())
}

/// A refused connection: 401 is the key, 403 a permission the key lacks, anything else the
/// network or the service.
fn handshake_error(name: &str, error: WsError) -> SttError {
    match error {
        WsError::Http(response) => match response.status().as_u16() {
            401 => SttError::Auth(name.to_string()),
            403 => super::not_allowed(name),
            402 => SttError::Provider(format!("{name} says the account is out of credit. Top it up, then start recording again.")),
            429 => SttError::Provider(format!("{name} is limiting connections right now. Try again in a minute.")),
            400 => SttError::Provider(format!("{name} did not accept Cilica's settings. Try another language or service.")),
            status => SttError::Provider(format!("{name} answered with an error ({status}).")),
        },
        _ => SttError::Network(name.to_string()),
    }
}

/// Seconds (as providers send them) to milliseconds.
pub(crate) fn secs_ms(secs: f64) -> u32 {
    (secs.max(0.0) * 1000.0).round() as u32
}
