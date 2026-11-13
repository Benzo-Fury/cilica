//! The model catalogue and downloader.
//!
//! Speech models are whisper.cpp GGML files from Hugging Face (`ggerganov/whisper.cpp`, MIT),
//! pinned to a commit and verified by size and SHA-256. Downloads resume from a `.part` file,
//! report progress through `AppEvent::ModelDownload` and can be cancelled. The small companion
//! models (Silero VAD; pyannote segmentation and the CAM++ speaker embedding used by
//! `cilica-diarize`) are fetched with the first speech model, inside the same progress bar.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures::StreamExt;
use cilica_domain::{AppEvent, Events, HardwareProfile, ModelDownloadProgress, SttModel};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::{SttError, SttResult};

/// One downloadable file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    pub file_name: &'static str,
    pub url: &'static str,
    pub size: u64,
    /// Hex SHA-256 of the file.
    pub sha256: &'static str,
}

/// A Whisper model in the catalogue.
#[derive(Debug, Clone, Copy)]
pub struct SpeechModel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub multilingual: bool,
    pub recommended: bool,
    pub file: ModelFile,
}


macro_rules! hf {
    ($f:literal) => {
        concat!("https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/", $f)
    };
}

/// The speech models Cilica offers, smallest first.
pub const SPEECH_MODELS: [SpeechModel; 3] = [
    SpeechModel {
        id: "base",
        name: "Base",
        description: "Small and quick. Good for older or slower computers.",
        multilingual: true,
        recommended: false,
        file: ModelFile {
            file_name: "ggml-base.bin",
            url: hf!("ggml-base.bin"),
            size: 147_951_465,
            sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        },
    },
    SpeechModel {
        id: "small",
        name: "Small",
        description: "Better accuracy than Base at a moderate size.",
        multilingual: true,
        recommended: false,
        file: ModelFile {
            file_name: "ggml-small.bin",
            url: hf!("ggml-small.bin"),
            size: 487_601_967,
            sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        },
    },
    SpeechModel {
        id: "large-v3-turbo-q5_0",
        name: "Large v3 Turbo",
        description: "Most accurate, in many languages. Needs Apple silicon or a recent PC.",
        multilingual: true,
        recommended: true,
        file: ModelFile {
            file_name: "ggml-large-v3-turbo-q5_0.bin",
            url: hf!("ggml-large-v3-turbo-q5_0.bin"),
            size: 574_041_195,
            sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        },
    },
];

/// Silero VAD v5.1.2 (MIT), github.com/snakers4/silero-vad.
pub const VAD_MODEL: ModelFile = ModelFile {
    file_name: "silero_vad_v5.onnx",
    url: "https://raw.githubusercontent.com/snakers4/silero-vad/v5.1.2/src/silero_vad/data/silero_vad.onnx",
    size: 2_327_524,
    sha256: "2623a2953f6ff3d2c1e61740c6cdb7168133479b267dfef114a4a3cc5bdd788f",
};

/// pyannote/segmentation-3.0 (MIT) as converted to ONNX by sherpa-onnx.
pub const SEGMENTATION_MODEL: ModelFile = ModelFile {
    file_name: "pyannote-segmentation-3.0.onnx",
    url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/9403a6902bb58e3d5ae8c7e77c3422de279db2e0/model.onnx",
    size: 5_992_913,
    sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
};

/// 3D-Speaker CAM++ speaker embedding trained on VoxCeleb (Apache-2.0), ONNX by sherpa-onnx.
pub const EMBEDDING_MODEL: ModelFile = ModelFile {
    file_name: "campplus-voxceleb-16k.onnx",
    url: "https://huggingface.co/csukuangfj/speaker-embedding-models/resolve/0743f301363dec56491a490f6d6cbc9d67f9a3bf/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx",
    size: 29_596_978,
    sha256: "357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b",
};

/// Progress id used when only the speaker models are fetched (diarization on first use).
pub const SPEAKER_MODELS_ID: &str = "speaker-models";

const COMPANIONS: [ModelFile; 3] = [VAD_MODEL, SEGMENTATION_MODEL, EMBEDDING_MODEL];

pub fn speech_model(id: &str) -> Option<&'static SpeechModel> {
    SPEECH_MODELS.iter().find(|m| m.id == id)
}

/// Paths a Whisper engine needs.
#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub whisper: PathBuf,
    pub vad: Option<PathBuf>,
}

/// Told when a download ends: `Ok` once on disk, `Err` with the reason otherwise.
type DoneSender = oneshot::Sender<Result<(), String>>;

/// Owns `<data_dir>/models/`.
pub struct ModelManager {
    dir: PathBuf,
    events: Events,
    client: reqwest::Client,
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Told when a download ends: `Ok` once the model is on disk, `Err` with the reason otherwise.
    waiters: Mutex<HashMap<String, Vec<DoneSender>>>,
    /// What this computer can run; decides which model is marked recommended.
    hardware: OnceLock<HardwareProfile>,
}

impl ModelManager {
    pub fn new(dir: PathBuf, events: Events) -> Arc<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Cilica/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Arc::new(Self {
            dir,
            events,
            client,
            active: Mutex::new(HashMap::new()),
            waiters: Mutex::new(HashMap::new()),
            hardware: OnceLock::new(),
        })
    }

    /// Record what this computer can run (once, at startup).
    pub fn set_hardware(&self, profile: HardwareProfile) {
        let _ = self.hardware.set(profile);
    }

    pub fn hardware(&self) -> Option<HardwareProfile> {
        self.hardware.get().copied()
    }

    /// Id of the model to recommend on this computer (the catalogue's default when unknown).
    pub fn recommended_id(&self) -> &'static str {
        match self.hardware.get() {
            Some(h) => crate::recommend::recommended_model(h.cores, h.ram_gb, h.apple_silicon),
            None => SPEECH_MODELS.iter().find(|m| m.recommended).map(|m| m.id).unwrap_or(SPEECH_MODELS[0].id),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_of(&self, f: &ModelFile) -> PathBuf {
        self.dir.join(f.file_name)
    }

    /// Present with the expected size (the hash was checked when it was downloaded).
    pub fn has(&self, f: &ModelFile) -> bool {
        std::fs::metadata(self.path_of(f)).map(|m| m.len() == f.size).unwrap_or(false)
    }

    /// The catalogue with what is on disk.
    pub fn list(&self) -> Vec<SttModel> {
        let recommended = self.recommended_id();
        SPEECH_MODELS
            .iter()
            .map(|m| SttModel {
                id: m.id.into(),
                name: m.name.into(),
                description: m.description.into(),
                size_mb: (m.file.size / 1_000_000) as u32,
                multilingual: m.multilingual,
                recommended: m.id == recommended,
                downloaded: self.has(&m.file),
            })
            .collect()
    }

    /// Paths for a speech model, or a `ModelMissing` error that names the fix.
    pub fn speech_paths(&self, id: &str) -> SttResult<ModelPaths> {
        let m = speech_model(id).ok_or_else(|| SttError::UnknownModel(id.into()))?;
        if !self.has(&m.file) {
            return Err(SttError::ModelMissing(m.name.into()));
        }
        Ok(ModelPaths { whisper: self.path_of(&m.file), vad: self.has(&VAD_MODEL).then(|| self.path_of(&VAD_MODEL)) })
    }

    /// Silero VAD, when it is on disk.
    pub fn vad_path(&self) -> Option<PathBuf> {
        self.has(&VAD_MODEL).then(|| self.path_of(&VAD_MODEL))
    }

    /// (segmentation, embedding) model paths when both are present.
    pub fn speaker_model_paths(&self) -> Option<(PathBuf, PathBuf)> {
        (self.has(&SEGMENTATION_MODEL) && self.has(&EMBEDDING_MODEL))
            .then(|| (self.path_of(&SEGMENTATION_MODEL), self.path_of(&EMBEDDING_MODEL)))
    }

    pub fn is_downloading(&self, id: &str) -> bool {
        self.active.lock().map(|a| a.contains_key(id)).unwrap_or(false)
    }

    /// Start downloading a speech model (and any missing companions) in the background.
    /// Progress and the outcome arrive as `ModelDownload` events.
    pub fn start_download(self: &Arc<Self>, id: &str) -> SttResult<()> {
        let model = speech_model(id).ok_or_else(|| SttError::UnknownModel(id.into()))?;
        let mut files = vec![model.file];
        files.extend(COMPANIONS.iter().filter(|f| !self.has(f)).copied());
        self.spawn(id.to_string(), files);
        Ok(())
    }

    /// Start (or join) a download and get told when it ends. Resolves at once when the model is
    /// already on disk.
    pub fn download_and_wait(self: &Arc<Self>, id: &str) -> SttResult<oneshot::Receiver<Result<(), String>>> {
        let model = speech_model(id).ok_or_else(|| SttError::UnknownModel(id.into()))?;
        let (tx, rx) = oneshot::channel();
        if self.has(&model.file) && COMPANIONS.iter().all(|f| self.has(f)) {
            let _ = tx.send(Ok(()));
            return Ok(rx);
        }
        if let Ok(mut waiters) = self.waiters.lock() {
            waiters.entry(id.to_string()).or_default().push(tx);
        }
        self.start_download(id)?;
        Ok(rx)
    }

    /// Fetch the speaker models if they are missing; waits for the result.
    pub async fn ensure_speaker_models(self: &Arc<Self>) -> SttResult<(PathBuf, PathBuf)> {
        if let Some(p) = self.speaker_model_paths() {
            return Ok(p);
        }
        let files: Vec<ModelFile> = [SEGMENTATION_MODEL, EMBEDDING_MODEL].into_iter().filter(|f| !self.has(f)).collect();
        self.download_files(SPEAKER_MODELS_ID, &files, Arc::new(AtomicBool::new(false))).await?;
        self.speaker_model_paths().ok_or_else(|| SttError::Download("the speaker models are incomplete".into()))
    }

    fn spawn(self: &Arc<Self>, id: String, files: Vec<ModelFile>) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let Ok(mut active) = self.active.lock() else { return };
            if active.contains_key(&id) {
                return; // already running
            }
            active.insert(id.clone(), cancel.clone());
        }
        let this = self.clone();
        tokio::spawn(async move {
            let result = this.download_files(&id, &files, cancel).await;
            if let Ok(mut active) = this.active.lock() {
                active.remove(&id);
            }
            if let Err(e) = &result {
                tracing::warn!(model = %id, error = %e, "model download failed");
            }
            let waiting = this.waiters.lock().ok().and_then(|mut w| w.remove(&id)).unwrap_or_default();
            for tx in waiting {
                let _ = tx.send(result.as_ref().map(|_| ()).map_err(|e| e.to_string()));
            }
        });
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(active) = self.active.lock()
            && let Some(flag) = active.get(id)
        {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Delete a downloaded speech model (and any partial download of it).
    pub fn delete(&self, id: &str) -> SttResult<()> {
        let m = speech_model(id).ok_or_else(|| SttError::UnknownModel(id.into()))?;
        self.cancel(id);
        for p in [self.path_of(&m.file), part_path(&self.path_of(&m.file))] {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    async fn download_files(&self, id: &str, files: &[ModelFile], cancel: Arc<AtomicBool>) -> SttResult<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let total: u64 = files.iter().map(|f| f.size).sum();
        let mut progress = Progress { id: id.to_string(), events: self.events.clone(), total, base: 0, last: None };
        progress.emit(0, false, None);
        for f in files {
            if self.has(f) {
                progress.base += f.size;
                continue;
            }
            match self.download_one(f, &cancel, &mut progress).await {
                Ok(()) => progress.base += f.size,
                Err(e) => {
                    progress.emit(0, false, Some(e.to_string()));
                    return Err(e);
                }
            }
        }
        progress.emit(0, true, None);
        Ok(())
    }

    async fn download_one(&self, f: &ModelFile, cancel: &AtomicBool, progress: &mut Progress) -> SttResult<()> {
        let dest = self.path_of(f);
        let part = part_path(&dest);
        let mut hasher = Sha256::new();
        // Resume: hash what we already have.
        let mut have = 0u64;
        if let Ok(mut existing) = tokio::fs::File::open(&part).await {
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = existing.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                have += n as u64;
            }
        }
        if f.size > 0 && have > f.size {
            tokio::fs::remove_file(&part).await?;
            have = 0;
            hasher = Sha256::new();
        }
        let mut req = self.client.get(f.url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = req.send().await.map_err(|e| SttError::Download(e.to_string()))?;
        let status = resp.status();
        if !(status.is_success()) {
            return Err(SttError::Download(format!("the server answered {status}")));
        }
        let mut file = if status == reqwest::StatusCode::PARTIAL_CONTENT && have > 0 {
            tokio::fs::OpenOptions::new().append(true).open(&part).await?
        } else {
            // Server ignored the range: start over.
            have = 0;
            hasher = Sha256::new();
            tokio::fs::File::create(&part).await?
        };
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                file.flush().await?;
                return Err(SttError::Cancelled);
            }
            let chunk = chunk.map_err(|e| SttError::Download(e.to_string()))?;
            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            have += chunk.len() as u64;
            progress.emit(have, false, None);
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        if f.size > 0 && have != f.size {
            return Err(SttError::Checksum(format!("expected {} bytes, got {have}", f.size)));
        }
        let digest = hex(&hasher.finalize());
        if !f.sha256.is_empty() && digest != f.sha256 {
            tokio::fs::remove_file(&part).await.ok();
            return Err(SttError::Checksum("its checksum does not match".into()));
        }
        tokio::fs::rename(&part, &dest).await?;
        Ok(())
    }
}

fn part_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct Progress {
    id: String,
    events: Events,
    total: u64,
    /// Bytes of files already finished.
    base: u64,
    last: Option<Instant>,
}

impl Progress {
    fn emit(&mut self, current_file_bytes: u64, done: bool, error: Option<String>) {
        let throttled = !done && error.is_none() && self.last.is_some_and(|t| t.elapsed() < Duration::from_millis(200));
        if throttled {
            return;
        }
        self.last = Some(Instant::now());
        let downloaded = if done { self.total } else { (self.base + current_file_bytes).min(self.total) };
        self.events.emit(AppEvent::ModelDownload {
            progress: ModelDownloadProgress {
                model_id: self.id.clone(),
                downloaded_bytes: downloaded as f64,
                total_bytes: self.total as f64,
                done,
                error,
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cilica_domain::{NullSink, RecordingSink};

    #[test]
    fn default_model_is_in_catalogue() {
        let id = cilica_domain::TranscriptionSettings::default().model_id;
        let m = speech_model(&id).expect("default model id must exist");
        assert!(m.recommended);
        assert_eq!(SPEECH_MODELS.iter().filter(|m| m.recommended).count(), 1);
    }

    #[test]
    fn recommendation_follows_hardware() {
        let mgr = ModelManager::new(std::env::temp_dir().join("cilica-models-none"), Arc::new(NullSink));
        let marked = |m: &ModelManager| m.list().into_iter().filter(|x| x.recommended).map(|x| x.id).collect::<Vec<_>>();
        assert_eq!(marked(&mgr), vec!["large-v3-turbo-q5_0".to_string()]);
        mgr.set_hardware(crate::recommend::profile(2, 4, false));
        assert_eq!(marked(&mgr), vec!["base".to_string()]);
    }

    #[test]
    fn list_and_paths_reflect_disk() {
        let dir = std::env::temp_dir().join(format!("cilica-models-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mgr = ModelManager::new(dir.clone(), Arc::new(NullSink));
        assert!(mgr.list().iter().all(|m| !m.downloaded));
        assert!(matches!(mgr.speech_paths("base"), Err(SttError::ModelMissing(_))));
        assert!(matches!(mgr.speech_paths("huge"), Err(SttError::UnknownModel(_))));
        // A file with the right size counts as downloaded.
        let f = std::fs::File::create(dir.join("ggml-base.bin")).unwrap();
        f.set_len(SPEECH_MODELS[0].file.size).unwrap();
        assert!(mgr.list()[0].downloaded);
        assert!(mgr.speech_paths("base").unwrap().vad.is_none());
        mgr.delete("base").unwrap();
        assert!(!mgr.list()[0].downloaded);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Serves a file over plain HTTP from a local socket, honouring `Range`, to test resume and
    /// verification without the network.
    async fn serve(body: Vec<u8>) -> String {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else { return };
                let body = body.clone();
                tokio::spawn(async move {
                    let (r, mut w) = sock.into_split();
                    let mut lines = BufReader::new(r).lines();
                    let mut from = 0usize;
                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.is_empty() {
                            break;
                        }
                        if let Some(v) = line.to_lowercase().strip_prefix("range: bytes=") {
                            from = v.trim_end_matches('-').parse().unwrap_or(0);
                        }
                    }
                    let part = &body[from.min(body.len())..];
                    let status = if from > 0 { "206 Partial Content" } else { "200 OK" };
                    let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", part.len());
                    let _ = w.write_all(head.as_bytes()).await;
                    let _ = w.write_all(part).await;
                });
            }
        });
        format!("http://{addr}/model.bin")
    }

    #[tokio::test]
    async fn resumes_and_verifies() {
        let body: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let sha = hex(&Sha256::digest(&body));
        let url: &'static str = Box::leak(serve(body.clone()).await.into_boxed_str());
        let sha: &'static str = Box::leak(sha.into_boxed_str());
        let dir = std::env::temp_dir().join(format!("cilica-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A previous attempt left the first 100 kB.
        std::fs::write(dir.join("m.bin.part"), &body[..100_000]).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let mgr = ModelManager::new(dir.clone(), sink.clone());
        let file = ModelFile { file_name: "m.bin", url, size: body.len() as u64, sha256: sha };
        mgr.download_files("test", &[file], Arc::new(AtomicBool::new(false))).await.unwrap();
        assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), body);
        let last = sink.0.lock().unwrap().last().cloned().unwrap();
        assert!(matches!(last, AppEvent::ModelDownload { progress } if progress.done && progress.downloaded_bytes == 300_000.0));

        // A corrupt download is rejected.
        let bad = ModelFile { file_name: "bad.bin", url, size: body.len() as u64, sha256: "00" };
        let err = mgr.download_files("test", &[bad], Arc::new(AtomicBool::new(false))).await.unwrap_err();
        assert!(matches!(err, SttError::Checksum(_)));
        assert!(!dir.join("bad.bin").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
