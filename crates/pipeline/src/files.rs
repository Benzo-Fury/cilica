//! Where a recording lives on disk, the crash-recovery sidecar, and audio retention.
//!
//! While capturing, `<data_dir>/recordings/<note>-<unix ms>.wav` grows (crash-safe, see
//! `cilica_audio::WavWriter`) next to `<same stem>.json`, the [`Sidecar`] that says how far the
//! recording got. A finished recording is `recordings/<note>.ogg` (or `.wav` in builds without
//! Opus), stored in `notes.audio_path` relative to the data dir. A sidecar left at launch means
//! the app stopped mid-recording; [`crate::RecordingService::startup`] finishes it.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cilica_domain::{Id, Timestamp};
use serde::{Deserialize, Serialize};

/// The recordings folder, relative to the data dir (the webview may only play files here).
pub const RECORDINGS_DIR: &str = "recordings";

/// How far a recording's post-processing got. Steps are redone only from the first one missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage {
    /// Audio is being captured (or the app stopped while it was).
    Recording,
    /// Capture finished and the live transcript is complete.
    Captured,
    /// The after-meeting pass has redone the transcript (or was not needed).
    Refined,
    /// The kept audio file is written and recorded on the note.
    Encoded,
    /// Speakers are assigned and the note is reindexed.
    Diarized,
    /// The summary is written (or failed for good). Nothing left to do.
    Summarized,
}

/// Written next to an in-progress recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sidecar {
    pub note_id: Id,
    pub started_at: Timestamp,
    /// Where this recording starts on the note's timeline: a second recording into the same note
    /// continues after the first.
    pub offset_ms: u32,
    /// The note's earlier audio (relative path) that this recording is appended to, if any.
    pub previous_audio: Option<String>,
    pub stage: Stage,
}

/// The files of one recording.
#[derive(Debug, Clone)]
pub(crate) struct RecordingFiles {
    pub data_dir: PathBuf,
    pub stem: String,
}

impl RecordingFiles {
    pub fn new(data_dir: &Path, note_id: Id) -> Self {
        let ms = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        Self { data_dir: data_dir.to_path_buf(), stem: format!("{note_id}-{ms}") }
    }

    pub fn dir(&self) -> PathBuf {
        self.data_dir.join(RECORDINGS_DIR)
    }
    /// The capture file (stereo: mic left, system right).
    pub fn wav(&self) -> PathBuf {
        self.dir().join(format!("{}.wav", self.stem))
    }
    /// The capture file with the note's earlier audio in front, when there was some.
    pub fn combined_wav(&self) -> PathBuf {
        self.dir().join(format!("{}.combined.wav", self.stem))
    }
    pub fn sidecar(&self) -> PathBuf {
        self.dir().join(format!("{}.json", self.stem))
    }

    pub fn write_sidecar(&self, sidecar: &Sidecar) -> std::io::Result<()> {
        std::fs::create_dir_all(self.dir())?;
        let tmp = self.dir().join(format!("{}.json.tmp", self.stem));
        std::fs::write(&tmp, serde_json::to_vec_pretty(sidecar).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, self.sidecar())
    }

    /// Remove the working files once the note is finished.
    pub fn remove_working_files(&self) {
        for p in [self.wav(), self.combined_wav(), self.sidecar()] {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(path = %p.display(), error = %e, "could not remove a working file"),
            }
        }
    }
}

/// The kept audio file of a note, absolute, and its path relative to the data dir.
pub(crate) fn kept_audio(data_dir: &Path, note_id: Id, ext: &str) -> (PathBuf, String) {
    let rel = format!("{RECORDINGS_DIR}/{note_id}.{ext}");
    (data_dir.join(&rel), rel)
}

/// Sidecars of recordings that were never finished, oldest first.
pub(crate) fn unfinished(data_dir: &Path) -> Vec<(RecordingFiles, Sidecar)> {
    let dir = data_dir.join(RECORDINGS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else { continue };
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice::<Sidecar>(&b).map_err(|e| e.to_string())) {
            Ok(sidecar) => out.push((RecordingFiles { data_dir: data_dir.to_path_buf(), stem }, sidecar)),
            Err(error) => tracing::warn!(path = %path.display(), %error, "ignoring an unreadable recording sidecar"),
        }
    }
    out.sort_by_key(|(_, s)| s.started_at);
    out
}

/// Kept recordings (`<note id>.ogg|.wav`) last written more than `days` ago, with their note id.
/// `days == 0` keeps everything.
pub(crate) fn expired(data_dir: &Path, days: u32) -> Vec<(Id, PathBuf)> {
    if days == 0 {
        return Vec::new();
    }
    let cutoff = SystemTime::now().checked_sub(Duration::from_secs(u64::from(days) * 86_400)).unwrap_or(SystemTime::UNIX_EPOCH);
    let Ok(entries) = std::fs::read_dir(data_dir.join(RECORDINGS_DIR)) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("ogg" | "wav")) {
            continue;
        }
        // Working files have a `-<ms>` suffix and do not parse as a bare id.
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| Id::parse_str(s).ok()) else { continue };
        let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or_else(|_| SystemTime::now());
        if modified < cutoff {
            out.push((id, path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_roundtrip_and_listing() {
        let dir = tempfile::tempdir().unwrap();
        let note = cilica_domain::new_id();
        let files = RecordingFiles::new(dir.path(), note);
        let sidecar = Sidecar { note_id: note, started_at: cilica_domain::now(), offset_ms: 1200, previous_audio: None, stage: Stage::Recording };
        files.write_sidecar(&sidecar).unwrap();
        std::fs::write(files.wav(), b"RIFF").unwrap();
        let found = unfinished(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, sidecar);
        assert_eq!(found[0].0.wav(), files.wav());
        // Working files are never treated as kept recordings.
        assert!(expired(dir.path(), 1).is_empty());
        files.remove_working_files();
        assert!(unfinished(dir.path()).is_empty());
        assert!(Stage::Recording < Stage::Captured && Stage::Diarized < Stage::Summarized);
    }

    #[test]
    fn retention_picks_old_kept_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let note = cilica_domain::new_id();
        let (path, rel) = kept_audio(dir.path(), note, "ogg");
        assert_eq!(rel, format!("recordings/{note}.ogg"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"OggS").unwrap();
        assert!(expired(dir.path(), 0).is_empty(), "0 keeps for ever");
        assert!(expired(dir.path(), 30).is_empty(), "fresh file kept");
        let old = SystemTime::now() - Duration::from_secs(40 * 86_400);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        assert_eq!(expired(dir.path(), 30), vec![(note, path)]);
    }
}
