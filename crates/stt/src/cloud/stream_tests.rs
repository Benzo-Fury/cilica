//! The streaming adapters against a local WebSocket server that checks what Cilica sends and
//! replays provider messages, so no key or network is needed.

use std::sync::{Arc, Mutex};

use futures::{SinkExt, StreamExt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;

use super::link::LinkStream;
use super::ws::{self, Protocol};
use super::{assemblyai, deepgram, soniox};
use crate::{SttError, SttEvent, TranscriptStream, Utterance};

/// What the fake provider saw.
#[derive(Default, Debug)]
struct Seen {
    uri: String,
    auth: String,
    texts: Vec<String>,
    audio_bytes: usize,
}

/// A one-connection server: records the request and messages; once `is_end` matches a text
/// frame, sends `replies` and closes. With `reject`, refuses the handshake with that status.
async fn serve(replies: Vec<String>, is_end: fn(&str) -> bool, reject: Option<u16>) -> (String, Arc<Mutex<Seen>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = seen.clone();
    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        replay(socket, replies, is_end, reject, record).await;
    });
    (url, seen)
}

/// [`serve`] over TLS, as the real providers are, with a certificate for `localhost` made for
/// the test. Returns the URL, what the server saw, and the certificate to trust.
async fn serve_tls(replies: Vec<String>, is_end: fn(&str) -> bool) -> (String, Arc<Mutex<Seen>>, CertificateDer<'static>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let root = cert.cert.der().clone();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![root.clone()], key)
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("wss://localhost:{}/v1/listen", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = seen.clone();
    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let Ok(tls) = acceptor.accept(socket).await else { return };
        replay(tls, replies, is_end, None, record).await;
    });
    (url, seen, root)
}

/// Accept the WebSocket on `socket`, record what arrives and replay `replies` at the end.
async fn replay<S>(socket: S, replies: Vec<String>, is_end: fn(&str) -> bool, reject: Option<u16>, record: Arc<Mutex<Seen>>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    // The handshake callback's signature is fixed by tungstenite.
    #[allow(clippy::result_large_err)]
    let callback = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let mut s = record.lock().unwrap();
        s.uri = req.uri().to_string();
        s.auth = req.headers().get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
        match reject {
            Some(code) => {
                let mut e = ErrorResponse::new(None);
                *e.status_mut() = StatusCode::from_u16(code).unwrap();
                Err(e)
            }
            None => Ok(resp),
        }
    };
    let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(socket, callback).await else { return };
    while let Some(Ok(m)) = ws.next().await {
        match m {
            Message::Binary(b) => record.lock().unwrap().audio_bytes += b.len(),
            Message::Text(t) => {
                let t = t.to_string();
                record.lock().unwrap().texts.push(t.clone());
                if is_end(&t) {
                    for r in &replies {
                        ws.send(Message::Text(r.clone().into())).await.unwrap();
                    }
                    let _ = ws.close(None).await;
                    return;
                }
            }
            _ => {}
        }
    }
}

/// Run a protocol over one second of audio that starts 1 s into the recording.
fn transcribe<P: Protocol + 'static>(protocol: P) -> Result<Vec<SttEvent>, SttError> {
    let mut stream = LinkStream::spawn("test", move |audio, emitter| async move { ws::run(protocol, audio, emitter).await }).unwrap();
    let mut events = stream.push(16_000, &[0.1; 16_000])?;
    events.extend(stream.finish()?);
    Ok(events)
}

fn finals(events: &[SttEvent]) -> Vec<Utterance> {
    events.iter().filter_map(|e| if let SttEvent::Final(u) = e { Some(u.clone()) } else { None }).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn soniox_tokens_become_partials_and_finals() {
    let replies = vec![
        r#"{"tokens":[{"text":"Hel","start_ms":0,"end_ms":200,"is_final":true},{"text":"lo","start_ms":200,"end_ms":400,"is_final":false}]}"#.into(),
        r#"{"tokens":[{"text":"lo there.","start_ms":200,"end_ms":900,"is_final":true},{"text":"<end>","is_final":true}]}"#.into(),
        r#"{"tokens":[],"finished":true}"#.into(),
    ];
    let (url, seen) = serve(replies, |t| t.is_empty(), None).await;
    let mut p = soniox::Soniox::new("sk-test".into(), Some("en".into()), false);
    p.url = url;
    let events = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap();

    assert!(events.iter().any(|e| matches!(e, SttEvent::Partial(u) if u.text == "Hello" && u.id == 1)));
    assert_eq!(finals(&events), vec![Utterance { id: 1, start_ms: 1_000, end_ms: 1_900, text: "Hello there.".into(), speaker: None }]);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.auth, "Bearer sk-test");
    let config: serde_json::Value = serde_json::from_str(&seen.texts[0]).unwrap();
    assert_eq!(config["model"], "stt-rt-v6");
    assert_eq!(config["sample_rate"], 16000);
    assert_eq!(config["language_hints"][0], "en");
    assert_eq!(seen.audio_bytes, 32_000, "one second of 16-bit audio");
}

#[tokio::test(flavor = "multi_thread")]
async fn deepgram_results_close_at_speech_final() {
    let replies = vec![
        r#"{"type":"Results","start":0.1,"duration":0.5,"is_final":false,"speech_final":false,"channel":{"alternatives":[{"transcript":"hello"}]}}"#.into(),
        r#"{"type":"Results","start":0.1,"duration":0.8,"is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"Hello there."}]}}"#.into(),
        r#"{"type":"Metadata","request_id":"x","duration":1.0}"#.into(),
    ];
    let (url, seen) = serve(replies, |t| t.contains("CloseStream"), None).await;
    let mut p = deepgram::Deepgram::new("dg-test".into(), None, false);
    p.url = url;
    let events = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap();

    assert!(events.iter().any(|e| matches!(e, SttEvent::Partial(u) if u.text == "hello")));
    assert_eq!(finals(&events), vec![Utterance { id: 1, start_ms: 1_100, end_ms: 1_900, text: "Hello there.".into(), speaker: None }]);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.auth, "Token dg-test");
    assert!(seen.uri.contains("model=nova-3") && seen.uri.contains("language=multi") && seen.uri.contains("sample_rate=16000"), "{}", seen.uri);
}

#[tokio::test(flavor = "multi_thread")]
async fn assemblyai_turns_finish_when_formatted() {
    let replies = vec![
        r#"{"type":"Begin","id":"s1"}"#.into(),
        r#"{"type":"Turn","turn_order":0,"transcript":"hello there","end_of_turn":false,"turn_is_formatted":false,"words":[{"start":100,"end":400},{"start":450,"end":800}]}"#.into(),
        r#"{"type":"Turn","turn_order":0,"transcript":"hello there","end_of_turn":true,"turn_is_formatted":false,"words":[{"start":100,"end":400},{"start":450,"end":800}]}"#.into(),
        r#"{"type":"Turn","turn_order":0,"transcript":"Hello there.","end_of_turn":true,"turn_is_formatted":true,"words":[{"start":100,"end":400},{"start":450,"end":800}]}"#.into(),
        r#"{"type":"Termination","audio_duration_seconds":1}"#.into(),
    ];
    let (url, seen) = serve(replies, |t| t.contains("Terminate"), None).await;
    let mut p = assemblyai::AssemblyAi::new("aai-test".into(), Some("en".into()), false);
    p.url = url;
    let events = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap();

    assert_eq!(finals(&events), vec![Utterance { id: 1, start_ms: 1_100, end_ms: 1_800, text: "Hello there.".into(), speaker: None }]);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.auth, "aai-test", "raw key, no Bearer");
    assert!(seen.uri.contains("speech_model=universal-streaming-english"), "{}", seen.uri);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_key_says_so_without_the_key() {
    let (url, _) = serve(Vec::new(), |_| false, Some(401)).await;
    let mut p = deepgram::Deepgram::new("dg-secret".into(), Some("en".into()), false);
    p.url = url;
    let error = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap_err();
    assert!(matches!(error, SttError::Auth(_)), "{error:?}");
    assert!(!error.to_string().contains("dg-secret"));
    assert!(error.to_string().contains("Deepgram"));
}

#[tokio::test(flavor = "multi_thread")]
async fn assemblyai_error_frame_maps_to_auth() {
    let replies = vec![r#"{"type":"Error","error_code":1008,"error":"Unauthorized Connection: Invalid API key"}"#.into()];
    let (url, _) = serve(replies, |t| t.contains("Terminate"), None).await;
    let mut p = assemblyai::AssemblyAi::new("bad".into(), None, false);
    p.url = url;
    let error = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap_err();
    assert!(matches!(error, SttError::Auth(_)), "{error:?}");
}

/// The real providers are reached over TLS. The app has two rustls crypto backends compiled in
/// (this crate's test build adds the second one too), so the connection must name its own
/// rather than rely on a process default.
#[tokio::test(flavor = "multi_thread")]
async fn streams_over_tls() {
    let replies = vec![
        r#"{"tokens":[{"text":"Secure hello.","start_ms":0,"end_ms":500,"is_final":true},{"text":"<end>","is_final":true}]}"#.into(),
        r#"{"tokens":[],"finished":true}"#.into(),
    ];
    let (url, seen, root) = serve_tls(replies, |t| t.is_empty()).await;
    let mut p = soniox::Soniox::new("sk-test".into(), None, false);
    p.url = url;
    let events = tokio::task::spawn_blocking(move || {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(root).unwrap();
        let tls = ws::client_config(roots).unwrap();
        let mut stream = LinkStream::spawn("test", move |audio, emitter| async move { ws::run_with(p, audio, emitter, tls).await }).unwrap();
        let mut events = stream.push(16_000, &[0.1; 16_000])?;
        events.extend(stream.finish()?);
        Ok::<_, SttError>(events)
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(finals(&events), vec![Utterance { id: 1, start_ms: 1_000, end_ms: 1_500, text: "Secure hello.".into(), speaker: None }]);
    assert_eq!(seen.lock().unwrap().auth, "Bearer sk-test");
}

/// The real Soniox endpoint, over the system's certificates, with a key it will refuse: proves
/// TLS works on this computer without spending anything. Needs the network.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn reaches_soniox_over_tls() {
    let p = soniox::Soniox::new("not-a-real-key".into(), None, false);
    let result = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap();
    assert!(matches!(result, Err(SttError::Auth(_))), "{result:?}");
    let check = soniox::verify_realtime("not-a-real-key").await;
    assert!(matches!(check, Err(SttError::Auth(_))), "{check:?}");
}

/// A Soniox key without real-time access says what to change, not that the key is wrong.
#[tokio::test(flavor = "multi_thread")]
async fn soniox_key_without_realtime_access_says_so() {
    let replies = vec![r#"{"tokens":[],"error_code":403,"error_type":"permission_denied","error_message":"Not allowed."}"#.into()];
    let (url, _seen) = serve(replies, |t| t.is_empty(), None).await;
    let mut p = soniox::Soniox::new("sk-test".into(), None, false);
    p.url = url;
    let err = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap_err();
    assert!(matches!(err, SttError::NotAllowed(ref m) if m.contains("Speech-to-text, real-time")), "{err:?}");
}

/// Saving a Soniox key opens a stream with no audio: a refusal rejects the key, a clean end or
/// anything unclear keeps it.
#[tokio::test(flavor = "multi_thread")]
async fn soniox_key_check_covers_realtime_access() {
    let tls = || ws::client_config(rustls::RootCertStore::empty()).unwrap();

    let (url, seen) = serve(vec![r#"{"tokens":[],"finished":true}"#.into()], |t| t.is_empty(), None).await;
    soniox::verify_realtime_at(&url, "sk-good", tls()).await.unwrap();
    assert_eq!(seen.lock().unwrap().audio_bytes, 0, "no audio is sent while checking a key");

    let refused = vec![r#"{"tokens":[],"error_code":403,"error_type":"permission_denied","error_message":"Not allowed."}"#.into()];
    let (url, _) = serve(refused, |t| t.is_empty(), None).await;
    let err = soniox::verify_realtime_at(&url, "sk-tts-only", tls()).await.unwrap_err();
    assert!(matches!(err, SttError::NotAllowed(_)), "{err:?}");

    let (url, _) = serve(Vec::new(), |_| false, Some(401)).await;
    let err = soniox::verify_realtime_at(&url, "sk-wrong", tls()).await.unwrap_err();
    assert!(matches!(err, SttError::Auth(_)), "{err:?}");

    let (url, _) = serve(vec![r#"{"tokens":[],"error_code":503,"error_message":"Busy."}"#.into()], |t| t.is_empty(), None).await;
    soniox::verify_realtime_at(&url, "sk-good", tls()).await.unwrap();
}

fn speakers_of(events: &[SttEvent]) -> Vec<(String, Option<String>)> {
    finals(events).into_iter().map(|u| (u.text, u.speaker)).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn soniox_splits_lines_by_speaker() {
    let replies = vec![
        r#"{"tokens":[{"text":"Hello","start_ms":0,"end_ms":300,"is_final":true,"speaker":"1"},{"text":" there.","start_ms":300,"end_ms":600,"is_final":true,"speaker":"1"},{"text":" Hi","start_ms":700,"end_ms":900,"is_final":false,"speaker":"2"}]}"#.into(),
        r#"{"tokens":[{"text":" Hi!","start_ms":700,"end_ms":900,"is_final":true,"speaker":"2"},{"text":"<end>","is_final":true}]}"#.into(),
        r#"{"tokens":[],"finished":true}"#.into(),
    ];
    let (url, seen) = serve(replies, |t| t.is_empty(), None).await;
    let mut p = soniox::Soniox::new("sk-test".into(), None, true);
    p.url = url;
    let events = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap();

    assert_eq!(speakers_of(&events), vec![("Hello there.".into(), Some("1".into())), ("Hi!".into(), Some("2".into()))]);
    let config: serde_json::Value = serde_json::from_str(&seen.lock().unwrap().texts[0]).unwrap();
    assert_eq!(config["enable_speaker_diarization"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn soniox_asks_for_speakers_only_when_told() {
    let (url, seen) = serve(vec![r#"{"tokens":[],"finished":true}"#.into()], |t| t.is_empty(), None).await;
    let mut p = soniox::Soniox::new("sk-test".into(), None, false);
    p.url = url;
    let _ = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap();
    let config: serde_json::Value = serde_json::from_str(&seen.lock().unwrap().texts[0]).unwrap();
    assert!(config.get("enable_speaker_diarization").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn deepgram_splits_settled_words_by_speaker() {
    let replies = vec![
        r#"{"type":"Results","start":0.0,"duration":1.0,"is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"hello there hi","words":[{"word":"hello","punctuated_word":"Hello","start":0.1,"end":0.3,"speaker":0},{"word":"there","punctuated_word":"there.","start":0.3,"end":0.5,"speaker":0},{"word":"hi","punctuated_word":"Hi.","start":0.6,"end":0.9,"speaker":1}]}]}}"#.into(),
        r#"{"type":"Metadata","request_id":"x","duration":1.0}"#.into(),
    ];
    let (url, seen) = serve(replies, |t| t.contains("CloseStream"), None).await;
    let mut p = deepgram::Deepgram::new("dg-test".into(), None, true);
    p.url = url;
    let events = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap();

    assert_eq!(speakers_of(&events), vec![("Hello there.".into(), Some("0".into())), ("Hi.".into(), Some("1".into()))]);
    let f = finals(&events);
    assert_eq!((f[0].start_ms, f[0].end_ms, f[1].start_ms), (1_100, 1_500, 1_600));
    assert!(seen.lock().unwrap().uri.contains("diarize=true"));
}

#[tokio::test(flavor = "multi_thread")]
async fn assemblyai_turns_carry_their_speaker() {
    let replies = vec![
        r#"{"type":"Begin","id":"s1"}"#.into(),
        r#"{"type":"Turn","turn_order":0,"transcript":"Hello there.","end_of_turn":true,"turn_is_formatted":true,"speaker_label":"A","words":[{"start":100,"end":800}]}"#.into(),
        r#"{"type":"Turn","turn_order":1,"transcript":"Yes.","end_of_turn":true,"turn_is_formatted":true,"speaker_label":"PENDING","words":[{"start":900,"end":1000}]}"#.into(),
        r#"{"type":"Termination","audio_duration_seconds":1}"#.into(),
    ];
    let (url, seen) = serve(replies, |t| t.contains("Terminate"), None).await;
    let mut p = assemblyai::AssemblyAi::new("aai-test".into(), Some("en".into()), true);
    p.url = url;
    let events = tokio::task::spawn_blocking(move || transcribe(p)).await.unwrap().unwrap();

    assert_eq!(speakers_of(&events), vec![("Hello there.".into(), Some("A".into())), ("Yes.".into(), None)]);
    assert!(seen.lock().unwrap().uri.contains("speaker_labels=true"));
}
