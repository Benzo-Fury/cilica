//! After capture: finish the transcript, redo it carefully, keep the audio, find the speakers,
//! write the summary.
//! Each step is recorded in the sidecar so a recording interrupted by a quit or crash resumes at
//! the first step it had not finished (see [`crate::files::Stage`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use margin_audio::{SAMPLE_RATE, WavWriter, read_wav_16k, read_wav_channel_16k, recover_wav};
use margin_diarize::{Diarization, RECOGNISE_THRESHOLD, SUGGEST_THRESHOLD, assign_segments, cosine};
use margin_domain::{
    AppError, AppResult, AudioChannel, DataScope, ErrorCode, Id, RecordingPhase, Segment, ToastAction, ToastKind,
    ToastRequest, Timestamp,
};
use margin_store::{NewSpeaker, SELF_LABEL};

use crate::echo::EchoFilter;
use crate::files::{RecordingFiles, Sidecar, Stage, kept_audio};
use crate::live::Draining;
use crate::service::Inner;

/// A recording whose capture has ended.
pub(crate) struct Job {
    pub files: RecordingFiles,
    pub sidecar: Sidecar,
    /// The live transcript still catching up (absent for a recovered recording).
    pub draining: Option<Draining>,
    pub ended_at: Timestamp,
    /// Recovered at launch: the tail of the audio was never transcribed live.
    pub recovered: bool,
}

/// Mic speech needed before the mic channel is diarized for in-person speakers.
const MIC_DIARIZE_MIN_MS: u32 = 10_000;
/// A second voice on the mic counts as a person in the room with at least this much speech,
/// and only if it does not sound like anyone on the call (that would be echo).
const ROOM_SPEAKER_MIN_MS: u32 = 3_000;
const ROOM_SPEAKER_MAX_CALL_SIMILARITY: f32 = 0.5;

/// Run every remaining step. Failures of optional steps (speakers, summary) are reported as
/// toasts and do not stop the rest.
pub(crate) async fn finalize(inner: &Arc<Inner>, epoch: u64, mut job: Job) {
    let note_id = job.sidecar.note_id;
    if let Some(draining) = job.draining.take() {
        draining.finish().await;
    }
    if job.sidecar.stage < Stage::Captured {
        if job.recovered {
            recover_capture(inner, &job).await;
        }
        if let Err(error) = inner.db.delete_partial_segments(note_id).await {
            tracing::warn!(%error, "could not remove partial segments");
        }
        let started_at = inner.db.get_note(note_id).await.ok().and_then(|n| n.started_at).unwrap_or(job.sidecar.started_at);
        if let Err(error) = inner.db.set_note_meeting_times(note_id, Some(started_at), Some(job.ended_at)).await {
            tracing::warn!(%error, "could not record the meeting times");
        }
        advance(&job.files, &mut job.sidecar, Stage::Captured);
        inner.data_changed(&[DataScope::Notes, DataScope::Transcript], note_id);
    }

    if job.sidecar.stage < Stage::Refined {
        if refine(inner, &job).await {
            inner.data_changed(&[DataScope::Transcript], note_id);
        }
        advance(&job.files, &mut job.sidecar, Stage::Refined);
    }

    let audio = audio_for_analysis(&job);
    if job.sidecar.stage < Stage::Encoded {
        match keep_audio(inner, &job).await {
            Ok(()) => inner.data_changed(&[DataScope::Notes, DataScope::Transcript], note_id),
            Err(error) => {
                tracing::error!(message = %error.message, "could not keep the recording");
                inner.toast(ToastKind::Error, "The recording could not be saved", error.message, None);
            }
        }
        advance(&job.files, &mut job.sidecar, Stage::Encoded);
    }

    if job.sidecar.stage < Stage::Diarized {
        let identify = inner.db.get_settings().await.map(|s| s.transcription.identify_speakers_after_meeting).unwrap_or(true);
        if identify {
            inner.set_phase(epoch, RecordingPhase::Diarizing);
            if let Some(wav) = audio.as_deref() {
                match diarize(inner, note_id, wav).await {
                    Ok(_) => {}
                    Err(error) => tracing::warn!(message = %error.message, "speaker detection failed; keeping the live speakers"),
                }
            }
        }
        // Without the check (turned off, or it could not run) the live labels stay: drop any
        // speaker left with nothing said, such as "Others" once the service labelled everyone.
        if let Err(error) = inner.db.drop_unused_speakers(note_id).await {
            tracing::warn!(%error, "could not tidy the note's speakers");
        }
        if let Err(error) = inner.db.reindex_note(note_id).await {
            tracing::warn!(%error, "could not reindex the note");
        }
        inner.data_changed(&[DataScope::Transcript, DataScope::People, DataScope::Notes], note_id);
        advance(&job.files, &mut job.sidecar, Stage::Diarized);
    }

    if job.sidecar.stage < Stage::Summarized {
        inner.set_phase(epoch, RecordingPhase::Summarizing);
        summarize(inner, note_id).await;
        advance(&job.files, &mut job.sidecar, Stage::Summarized);
    }
    job.files.remove_working_files();
}

fn advance(files: &RecordingFiles, sidecar: &mut Sidecar, stage: Stage) {
    sidecar.stage = stage;
    if let Err(error) = files.write_sidecar(sidecar) {
        tracing::warn!(%error, "could not update the recording sidecar");
    }
}

/// The WAV that holds the whole note's audio: the combined file when this recording was
/// appended to an earlier one, otherwise the capture file.
fn audio_for_analysis(job: &Job) -> Option<PathBuf> {
    [job.files.combined_wav(), job.files.wav()].into_iter().find(|p| p.exists())
}

// ── Recovery ─────────────────────────────────────────────────────────────

/// A recording the app never finished: repair the WAV header and transcribe what the live
/// transcript had not reached.
async fn recover_capture(inner: &Arc<Inner>, job: &Job) {
    let wav = job.files.wav();
    let wav_for_repair = wav.clone();
    match tokio::task::spawn_blocking(move || recover_wav(&wav_for_repair)).await {
        Ok(Ok(frames)) => tracing::info!(frames, "recovered an interrupted recording"),
        Ok(Err(error)) => {
            tracing::warn!(%error, "the interrupted recording could not be read");
            return;
        }
        Err(_) => return,
    }
    let note_id = job.sidecar.note_id;
    let Ok(settings) = inner.db.get_settings().await else { return };
    let finals: Vec<Segment> = inner.db.list_segments(note_id).await.unwrap_or_default().into_iter().filter(|s| s.is_final).collect();
    let offset = job.sidecar.offset_ms;
    let (you, others) = match inner.db.ensure_channel_speakers(note_id).await {
        Ok((y, o)) => (y.id, o.id),
        Err(_) => return,
    };
    let speech = inner.speech.clone();
    let result = tokio::task::spawn_blocking(move || -> AppResult<Vec<Segment>> {
        // Never send a whole interrupted recording to a cloud service unasked: use the local
        // model (if it is not downloaded, the tail stays untranscribed).
        let mut local = settings.clone();
        local.transcription.engine = margin_domain::SttEngine::WhisperLocal;
        let engine = speech.transcriber(&local)?;
        let mut out = Vec::new();
        for (index, channel, speaker) in [(0, AudioChannel::Mic, you), (1, AudioChannel::System, others)] {
            let done_ms = finals.iter().filter(|s| s.channel == channel).map(|s| s.end_ms).max().unwrap_or(0).saturating_sub(offset);
            let audio = read_wav_channel_16k(&wav, index).map_err(AppError::from)?;
            let from = (done_ms as usize * SAMPLE_RATE as usize / 1000).min(audio.len());
            if audio.len() - from < SAMPLE_RATE as usize / 2 {
                continue;
            }
            for u in engine.transcribe(&audio[from..], settings.transcription.language.as_deref()).map_err(AppError::from)? {
                out.push(Segment {
                    id: margin_domain::new_id(),
                    note_id,
                    speaker_id: Some(speaker),
                    channel,
                    start_ms: u.start_ms + done_ms + offset,
                    end_ms: u.end_ms + done_ms + offset,
                    text: u.text,
                    is_final: true,
                });
            }
        }
        Ok(out)
    })
    .await;
    match result {
        Ok(Ok(segments)) => {
            for s in &segments {
                if let Err(error) = inner.db.upsert_segment(s).await {
                    tracing::warn!(%error, "could not store a recovered segment");
                }
            }
            tracing::info!(count = segments.len(), "transcribed the rest of an interrupted recording");
        }
        Ok(Err(error)) => tracing::warn!(message = %error.message, "could not transcribe the rest of an interrupted recording"),
        Err(_) => {}
    }
}

// ── After-meeting pass ───────────────────────────────────────────────────

/// A careful pass that returns less than this share of the live transcript's words for a
/// channel went wrong; that channel keeps its live text.
const REFINE_MIN_WORD_SHARE: f32 = 0.5;

/// Transcribe this recording again with the careful engine and swap the result in for the live
/// transcript, channel by channel. Returns whether anything changed. Any failure keeps the live
/// text.
async fn refine(inner: &Arc<Inner>, job: &Job) -> bool {
    let wav = job.files.wav();
    if !wav.exists() {
        return false;
    }
    let Ok(settings) = inner.db.get_settings().await else { return false };
    // A cloud service already gives a good transcript; redoing it on this computer would only
    // cost time.
    if !settings.transcription.refine_after_meeting || settings.transcription.engine.is_cloud() {
        return false;
    }
    let note_id = job.sidecar.note_id;
    let offset = job.sidecar.offset_ms;
    let (you, others) = match inner.db.ensure_channel_speakers(note_id).await {
        Ok((y, o)) => (y.id, o.id),
        Err(_) => return false,
    };
    let speech = inner.speech.clone();
    let started = std::time::Instant::now();
    let result = tokio::task::spawn_blocking(move || -> AppResult<Option<Vec<Segment>>> {
        let Some(engine) = speech.refiner(&settings) else { return Ok(None) };
        let language = settings.transcription.language.clone();
        let mut segments = Vec::new();
        for (index, channel, speaker) in [(0, AudioChannel::Mic, you), (1, AudioChannel::System, others)] {
            let audio = read_wav_channel_16k(&wav, index).map_err(AppError::from)?;
            for u in engine.transcribe_recording(&audio, language.as_deref()).map_err(AppError::from)? {
                segments.push(Segment {
                    id: margin_domain::new_id(),
                    note_id,
                    speaker_id: Some(speaker),
                    channel,
                    start_ms: u.start_ms + offset,
                    end_ms: u.end_ms + offset,
                    text: u.text,
                    is_final: true,
                });
            }
        }
        Ok(Some(drop_echo(segments)))
    })
    .await;
    let segments = match result {
        Ok(Ok(Some(segments))) => segments,
        Ok(Ok(None)) => return false,
        Ok(Err(error)) => {
            tracing::warn!(message = %error.message, "the after-meeting pass failed; keeping the live transcript");
            return false;
        }
        Err(_) => return false,
    };
    let live = inner.db.list_segments(note_id).await.unwrap_or_default();
    let mut changed = false;
    for channel in [AudioChannel::Mic, AudioChannel::System] {
        let words = |list: &mut dyn Iterator<Item = &Segment>| list.map(|s| s.text.split_whitespace().count()).sum::<usize>();
        let live_words = words(&mut live.iter().filter(|s| s.channel == channel && s.start_ms >= offset));
        let mine: Vec<Segment> = segments.iter().filter(|s| s.channel == channel).cloned().collect();
        let new_words = words(&mut mine.iter());
        if (new_words as f32) < live_words as f32 * REFINE_MIN_WORD_SHARE {
            tracing::warn!(?channel, live_words, new_words, "the after-meeting pass lost too much text; keeping the live transcript");
            continue;
        }
        match inner.db.replace_channel_segments(note_id, channel, offset, &mine).await {
            Ok(()) => changed = true,
            Err(error) => tracing::warn!(%error, "could not store the after-meeting transcript"),
        }
    }
    tracing::info!(took = ?started.elapsed(), changed, "after-meeting pass");
    changed
}

/// Drop mic segments that repeat what the call audio said at the same time (speaker echo).
fn drop_echo(mut segments: Vec<Segment>) -> Vec<Segment> {
    segments.sort_by_key(|s| (s.end_ms, s.start_ms));
    let mut filter = EchoFilter::default();
    let mut echo = std::collections::HashSet::new();
    for s in &segments {
        if let Some(id) = filter.on_final(s.channel, s.id, s.start_ms, s.end_ms, &s.text) {
            echo.insert(id);
        }
    }
    segments.retain(|s| !echo.contains(&s.id));
    segments.sort_by_key(|s| (s.start_ms, s.end_ms));
    segments
}

// ── Audio ────────────────────────────────────────────────────────────────

/// Write the kept file (Ogg Opus; WAV when this build has no encoder), appending this recording
/// to the note's earlier audio if it had some, and record it on the note.
async fn keep_audio(inner: &Arc<Inner>, job: &Job) -> AppResult<()> {
    let note_id = job.sidecar.note_id;
    let data_dir = inner.config.data_dir.clone();
    let files = job.files.clone();
    let previous = job.sidecar.previous_audio.as_ref().map(|rel| data_dir.join(rel)).filter(|p| p.exists());
    let offset_ms = job.sidecar.offset_ms;
    let (rel, duration_ms) = tokio::task::spawn_blocking(move || -> AppResult<(String, u32)> {
        let wav = files.wav();
        if !wav.exists() {
            return Err(AppError::not_found("The recording file is missing, so only the transcript was kept."));
        }
        let source = match previous {
            Some(prev) => {
                combine(&prev, offset_ms, &wav, &files.combined_wav()).map_err(AppError::from)?;
                files.combined_wav()
            }
            None => wav,
        };
        let (ogg, ogg_rel) = kept_audio(&data_dir, note_id, "ogg");
        match margin_audio::encode_ogg_opus(&source, &ogg) {
            Ok(ms) => Ok((ogg_rel, ms)),
            Err(error) => {
                tracing::warn!(%error, "keeping the recording as WAV");
                let (dest, rel) = kept_audio(&data_dir, note_id, "wav");
                std::fs::copy(&source, &dest).map_err(|e| AppError::internal(format!("The recording could not be saved: {e}. Check that the disk has space.")))?;
                let frames = hound::WavReader::open(&dest).map(|r| r.duration()).unwrap_or(0);
                Ok((rel, margin_audio::samples_to_ms(frames as u64)))
            }
        }
    })
    .await
    .map_err(|_| AppError::internal("Saving the recording stopped unexpectedly."))??;
    // A previous recording kept under another name (WAV before, Ogg now) is now included.
    if let Some(prev) = job.sidecar.previous_audio.as_ref().filter(|p| **p != rel) {
        let _ = std::fs::remove_file(inner.config.data_dir.join(prev));
    }
    inner.db.set_note_audio(note_id, Some(&rel), Some(duration_ms)).await?;
    Ok(())
}

/// `previous` (any WAV or Ogg Opus we wrote) padded or cut to `offset_ms`, then `wav`, into `out`.
fn combine(previous: &Path, offset_ms: u32, wav: &Path, out: &Path) -> margin_audio::AudioResult<()> {
    let mut prev = if previous.extension().and_then(|e| e.to_str()) == Some("ogg") {
        decode_previous(previous)?
    } else {
        read_wav_16k(previous)?
    };
    let len = offset_ms as usize * SAMPLE_RATE as usize / 1000;
    prev.resize(2, Vec::new());
    for ch in &mut prev {
        ch.resize(len, 0.0);
    }
    let mut writer = WavWriter::create(out, 2, SAMPLE_RATE)?;
    let mut buf = Vec::with_capacity(SAMPLE_RATE as usize * 2);
    for (left, right) in prev[0].chunks(SAMPLE_RATE as usize).zip(prev[1].chunks(SAMPLE_RATE as usize)) {
        buf.clear();
        for (l, r) in left.iter().zip(right) {
            buf.push(*l);
            buf.push(*r);
        }
        writer.write_interleaved(&buf)?;
    }
    drop(prev);
    let mut reader = hound::WavReader::open(wav).map_err(|e| margin_audio::AudioError::Encode(e.to_string()))?;
    buf.clear();
    for s in reader.samples::<i16>() {
        buf.push(s.unwrap_or(0) as f32 / 32768.0);
        if buf.len() >= SAMPLE_RATE as usize * 2 {
            writer.write_interleaved(&buf)?;
            buf.clear();
        }
    }
    writer.write_interleaved(&buf)?;
    writer.finish()?;
    Ok(())
}

#[cfg(feature = "opus")]
fn decode_previous(path: &Path) -> margin_audio::AudioResult<Vec<Vec<f32>>> {
    margin_audio::decode_ogg_opus(path)
}

#[cfg(not(feature = "opus"))]
fn decode_previous(_path: &Path) -> margin_audio::AudioResult<Vec<Vec<f32>>> {
    Err(margin_audio::AudioError::Encode("this build cannot read Ogg Opus".into()))
}

// ── Speakers ─────────────────────────────────────────────────────────────

/// Colour of "Person n" (1-based): `speaker-2` onwards, wrapping; `speaker-1` is You.
pub(crate) fn person_color(n: usize) -> u8 {
    (n % 8) as u8 + 1
}

/// Who spoke when, on both channels, then voiceprint suggestions. Returns how many people were
/// found on the call.
async fn diarize(inner: &Arc<Inner>, note_id: Id, wav: &Path) -> AppResult<usize> {
    let Some(models) = inner.speech.speaker_models().await else { return Ok(0) };
    let segments: Vec<Segment> = inner.db.list_segments(note_id).await?.into_iter().filter(|s| s.is_final).collect();
    let speech_ms = |ch: AudioChannel| segments.iter().filter(|s| s.channel == ch).map(|s| s.end_ms - s.start_ms).sum::<u32>();
    let (want_system, want_mic) = (speech_ms(AudioChannel::System) > 0, speech_ms(AudioChannel::Mic) >= MIC_DIARIZE_MIN_MS);
    if !want_system && !want_mic {
        return Ok(0);
    }
    let speech = inner.speech.clone();
    let wav = wav.to_path_buf();
    let (system, mic) = tokio::task::spawn_blocking(move || -> AppResult<(Option<Diarization>, Option<Diarization>)> {
        let started = std::time::Instant::now();
        let mut d = speech.diarizer(&models)?;
        let mut run = |want: bool, channel: usize| -> AppResult<Option<Diarization>> {
            if !want {
                return Ok(None);
            }
            let audio = read_wav_channel_16k(&wav, channel).map_err(AppError::from)?;
            Ok(Some(d.diarize(&audio)?))
        };
        let system = run(want_system, 1)?;
        let mic = run(want_mic, 0)?;
        tracing::info!(took = ?started.elapsed(), "diarized");
        Ok((system, mic))
    })
    .await
    .map_err(|_| AppError::internal("Speaker detection stopped unexpectedly."))??;

    let spans = |ch: AudioChannel| -> Vec<(Id, u32, u32)> {
        segments.iter().filter(|s| s.channel == ch).map(|s| (s.id, s.start_ms, s.end_ms)).collect()
    };
    let recognise_on = inner.db.get_settings().await.map(|s| s.transcription.recognise_voices).unwrap_or(true);
    let me_person = inner.db.ensure_self_person().await?;
    let my_voice = if recognise_on { inner.db.list_voiceprints().await?.into_iter().find(|v| v.person_id == me_person.id) } else { None };
    let mut people = 0;
    if let Some(sys) = system.as_ref().filter(|d| !d.speakers.is_empty()) {
        let speakers = sys
            .speakers
            .iter()
            .map(|s| NewSpeaker { label: format!("Person {}", s.index + 1), color_index: person_color(s.index + 1), is_self: false, embedding: Some(s.embedding.clone()) })
            .collect();
        let assignments = assign_segments(&sys.turns, &spans(AudioChannel::System));
        inner.db.apply_diarization(note_id, AudioChannel::System, speakers, &assignments, None).await?;
        people = sys.speakers.len();
    }
    if let Some(mic) = mic.as_ref().filter(|d| !d.speakers.is_empty()) {
        let call: Vec<&[f32]> = system.iter().flat_map(|d| d.speakers.iter().map(|s| s.embedding.as_slice())).collect();
        // You: the voice that sounds most like your saved voice, or, until Margin knows it, the
        // one that talked most.
        let by_voice = my_voice.as_ref().and_then(|v| {
            mic.speakers.iter().map(|s| (s.index, v.similarity(&s.embedding))).filter(|(_, sim)| *sim >= SUGGEST_THRESHOLD).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
        });
        let me = by_voice.unwrap_or_else(|| mic.speakers.iter().max_by_key(|s| s.speech_ms).map(|s| s.index).unwrap_or(0));
        let room: Vec<usize> = mic
            .speakers
            .iter()
            .filter(|s| s.index != me && s.speech_ms >= ROOM_SPEAKER_MIN_MS)
            .filter(|s| call.iter().all(|c| cosine(c, &s.embedding) < ROOM_SPEAKER_MAX_CALL_SIMILARITY))
            .map(|s| s.index)
            .collect();
        let mine = mic.speakers[me].embedding.clone();
        if room.is_empty() {
            let (you, _) = inner.db.ensure_channel_speakers(note_id).await?;
            inner.db.set_speaker_embedding(you.id, Some(&mine)).await?;
        } else {
            // You first, then the people in the room, numbered after those on the call.
            let mut speakers = vec![NewSpeaker { label: SELF_LABEL.into(), color_index: 1, is_self: true, embedding: Some(mine) }];
            for (k, &i) in room.iter().enumerate() {
                let n = people + k + 1;
                speakers.push(NewSpeaker { label: format!("Person {n}"), color_index: person_color(n), is_self: false, embedding: Some(mic.speakers[i].embedding.clone()) });
            }
            let assignments: Vec<(Id, usize)> = assign_segments(&mic.turns, &spans(AudioChannel::Mic))
                .into_iter()
                .map(|(id, c)| (id, room.iter().position(|&r| r == c).map(|p| p + 1).unwrap_or(0)))
                .collect();
            inner.db.apply_diarization(note_id, AudioChannel::Mic, speakers, &assignments, Some(0)).await?;
            people += room.len();
        }
    }
    inner.db.link_self_speakers(note_id).await?;
    if recognise_on {
        name_known_voices(inner, note_id).await?;
    }

    // Less sure: suggestions only, which the person confirms in the transcript.
    let suggestions = inner.db.speaker_suggestions(note_id, SUGGEST_THRESHOLD).await?;
    if let Some(first) = suggestions.first() {
        let label = inner.db.get_speaker(first.speaker_id).await.map(|s| s.label).unwrap_or_else(|_| "a speaker".into());
        let title = if suggestions.len() == 1 { "Margin may know a voice".to_string() } else { format!("Margin may know {} voices", suggestions.len()) };
        inner.toast(
            ToastKind::Action,
            &title,
            format!("Is {label} {}? Confirm in the transcript.", first.person_name),
            Some(("Review", ToastAction::OpenNote { note_id })),
        );
    }
    Ok(people)
}

/// Name the speakers whose voice clearly belongs to a Saved person (one speaker per person, the
/// closest first). These are guesses: nothing is learnt until the person confirms them.
async fn name_known_voices(inner: &Arc<Inner>, note_id: Id) -> AppResult<()> {
    let speakers = inner.db.list_speakers(note_id).await?;
    let mut taken: std::collections::HashSet<Id> = speakers.iter().filter_map(|s| s.person_id).collect();
    let mut matches = Vec::new();
    for s in speakers.iter().filter(|s| s.person_id.is_none() && !s.is_self) {
        let Some(embedding) = inner.db.speaker_embedding(s.id).await? else { continue };
        if let Some((person, similarity)) = inner.db.recognise_voice(&embedding, RECOGNISE_THRESHOLD, &taken).await? {
            matches.push((similarity, s.id, person));
        }
    }
    matches.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, speaker, person) in matches {
        if taken.insert(person) {
            inner.db.assign_speaker(speaker, person).await?;
        }
    }
    Ok(())
}

// ── Summary ──────────────────────────────────────────────────────────────

async fn summarize(inner: &Arc<Inner>, note_id: Id) {
    let summarizer = (inner.summarizer)();
    match summarizer.summarize_meeting(note_id).await {
        Ok(()) => inner.data_changed(&[DataScope::Notes, DataScope::Versions], note_id),
        Err(error) if error.code == ErrorCode::Unavailable => {
            tracing::info!(message = %error.message, "no summary: the assistant is not set up");
            inner.emit_toast(ToastRequest {
                kind: ToastKind::Action,
                title: "Transcript saved".into(),
                description: Some(format!("{} Margin can write the meeting notes once that is done.", error.message)),
                action_label: Some("Open settings".into()),
                action: Some(ToastAction::OpenSettings { page: "margin".into() }),
            });
        }
        Err(error) => {
            tracing::warn!(message = %error.message, "summary failed");
            inner.emit_toast(ToastRequest {
                kind: ToastKind::Error,
                title: "Meeting notes not written".into(),
                description: Some(error.message),
                action_label: Some("Try again".into()),
                action: Some(ToastAction::RegenerateSummary { note_id }),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_skip_you_and_wrap() {
        assert_eq!(person_color(1), 2);
        assert_eq!(person_color(7), 8);
        assert_eq!(person_color(8), 1);
    }

    #[test]
    fn combine_pads_previous_audio_to_the_offset() {
        let dir = tempfile::tempdir().unwrap();
        let (prev, new, out) = (dir.path().join("prev.wav"), dir.path().join("new.wav"), dir.path().join("out.wav"));
        let mut w = WavWriter::create(&prev, 2, SAMPLE_RATE).unwrap();
        w.write_interleaved(&vec![0.5; 2 * 8_000]).unwrap();
        w.finish().unwrap();
        let mut w = WavWriter::create(&new, 2, SAMPLE_RATE).unwrap();
        w.write_interleaved(&vec![-0.25; 2 * 4_000]).unwrap();
        w.finish().unwrap();
        combine(&prev, 1_000, &new, &out).unwrap();
        let ch = read_wav_16k(&out).unwrap();
        assert_eq!(ch[0].len(), 16_000 + 4_000);
        assert!((ch[0][100] - 0.5).abs() < 1e-3 && ch[1][12_000].abs() < 1e-6 && (ch[1][17_000] + 0.25).abs() < 1e-3);
    }
}
