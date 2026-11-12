//! Recognising Saved people by voice while the meeting is live (docs/spec.md › Speakers and
//! Saved people).
//!
//! The capture audio of each channel is kept for the last 90 seconds ([`VoiceTap`]). When the
//! cloud service labels a new speaker, the audio of their finished lines is gathered; after 3,
//! 8 and 15 seconds of it, Margin turns it into a voiceprint (on this computer, with the same
//! CAM++ model as the check after the meeting) and compares it with the Saved people. A close
//! enough match names the speaker. It is only a guess: nothing is learnt from it until the
//! person confirms or corrects the name ([`margin_store::Db::link_speaker`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use margin_diarize::{Diarizer, RECOGNISE_THRESHOLD};
use margin_domain::{AppEvent, AudioChannel, DataScope, Events, Id};
use margin_store::Db;
use tokio::sync::mpsc;

use crate::backend::SpeechBackend;

const RATE: usize = 16_000;
/// Audio kept per channel.
const KEEP: usize = RATE * 90;
/// Seconds of a speaker's speech after which recognition is tried.
const TRIES_AT: [usize; 3] = [RATE * 3, RATE * 8, RATE * 15];
/// The most recent speech used for one voiceprint.
const MAX_CLIP: usize = RATE * 20;

/// The last stretch of each channel's capture audio, on the recording timeline.
#[derive(Default)]
pub(crate) struct VoiceTap {
    channels: Mutex<HashMap<AudioChannel, Ring>>,
}

#[derive(Default)]
struct Ring {
    /// Timeline sample of `audio[0]`.
    start: u64,
    audio: VecDeque<f32>,
}

impl VoiceTap {
    /// Keep audio that starts at timeline sample `start`. A gap starts the ring afresh.
    pub fn push(&self, channel: AudioChannel, start: u64, samples: &[f32]) {
        let Ok(mut channels) = self.channels.lock() else { return };
        let ring = channels.entry(channel).or_default();
        if ring.start + ring.audio.len() as u64 != start {
            ring.start = start;
            ring.audio.clear();
        }
        ring.audio.extend(samples);
        let over = ring.audio.len().saturating_sub(KEEP);
        ring.audio.drain(..over);
        ring.start += over as u64;
    }

    /// The audio between two timeline samples, as much of it as is still kept.
    pub fn slice(&self, channel: AudioChannel, from: u64, to: u64) -> Vec<f32> {
        let Ok(channels) = self.channels.lock() else { return Vec::new() };
        let Some(ring) = channels.get(&channel) else { return Vec::new() };
        let end = ring.start + ring.audio.len() as u64;
        let (from, to) = (from.max(ring.start), to.min(end));
        if from >= to {
            return Vec::new();
        }
        ring.audio.range((from - ring.start) as usize..(to - ring.start) as usize).copied().collect()
    }
}

/// One speaker's speech so far.
#[derive(Default)]
struct Pool {
    audio: Vec<f32>,
    heard: usize,
    tried: usize,
}

struct Job {
    speaker_id: Id,
    channel: AudioChannel,
    clip: Vec<f32>,
}

/// Collects labelled speakers' audio and hands it to a worker that recognises them.
pub(crate) struct Recogniser {
    tap: Arc<VoiceTap>,
    pools: HashMap<Id, Pool>,
    named: Arc<Mutex<HashSet<Id>>>,
    jobs: Option<mpsc::UnboundedSender<Job>>,
    worker: tokio::task::JoinHandle<()>,
}

impl Recogniser {
    pub fn start(db: Db, events: Events, speech: Arc<dyn SpeechBackend>, note_id: Id, tap: Arc<VoiceTap>, rt: &tokio::runtime::Handle) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let named = Arc::new(Mutex::new(HashSet::new()));
        let worker = rt.spawn(work(db, events, speech, note_id, rx, named.clone()));
        Self { tap, pools: HashMap::new(), named, jobs: Some(tx), worker }
    }

    /// A finished line by a labelled speaker, at these recording-timeline times.
    pub fn heard(&mut self, speaker_id: Id, channel: AudioChannel, start_ms: u32, end_ms: u32) {
        if self.named.lock().map(|n| n.contains(&speaker_id)).unwrap_or(true) {
            return;
        }
        let clip = self.tap.slice(channel, start_ms as u64 * 16, end_ms as u64 * 16);
        let pool = self.pools.entry(speaker_id).or_default();
        pool.heard += clip.len();
        pool.audio.extend(clip);
        let over = pool.audio.len().saturating_sub(MAX_CLIP);
        pool.audio.drain(..over);
        if TRIES_AT.get(pool.tried).is_some_and(|&at| pool.heard >= at)
            && let Some(jobs) = &self.jobs
        {
            pool.tried += 1;
            let _ = jobs.send(Job { speaker_id, channel, clip: pool.audio.clone() });
        }
    }

    /// Let the last recognitions finish.
    pub async fn finish(mut self) {
        self.jobs.take();
        let _ = self.worker.await;
    }
}

async fn work(db: Db, events: Events, speech: Arc<dyn SpeechBackend>, note_id: Id, mut jobs: mpsc::UnboundedReceiver<Job>, named: Arc<Mutex<HashSet<Id>>>) {
    let mut diarizer: Option<Arc<Mutex<Diarizer>>> = None;
    let mut unavailable = false;
    while let Some(job) = jobs.recv().await {
        if unavailable || named.lock().map(|n| n.contains(&job.speaker_id)).unwrap_or(true) {
            continue;
        }
        if diarizer.is_none() {
            diarizer = load(&speech).await;
            if diarizer.is_none() {
                unavailable = true;
                continue;
            }
        }
        let Some(d) = diarizer.clone() else { continue };
        let clip = job.clip;
        let embedding = match tokio::task::spawn_blocking(move || d.lock().map_err(|_| ()).and_then(|mut d| d.embed(&clip).map_err(|_| ()))).await {
            Ok(Ok(e)) => e,
            _ => continue,
        };
        match recognise(&db, note_id, job.speaker_id, job.channel, &embedding).await {
            Ok(true) => {
                if let Ok(mut n) = named.lock() {
                    n.insert(job.speaker_id);
                }
                events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Transcript, DataScope::People], note_id: Some(note_id) });
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "could not compare a voice with Saved people"),
        }
    }
}

/// The voice model, fetched and loaded on first use. `None` when it cannot be had.
async fn load(speech: &Arc<dyn SpeechBackend>) -> Option<Arc<Mutex<Diarizer>>> {
    let models = speech.speaker_models().await?;
    let speech = speech.clone();
    match tokio::task::spawn_blocking(move || speech.diarizer(&models)).await {
        Ok(Ok(d)) => Some(Arc::new(Mutex::new(d))),
        Ok(Err(error)) => {
            tracing::warn!(message = %error.message, "voice recognition is off for this meeting");
            None
        }
        Err(_) => None,
    }
}

/// Keep the speaker's voiceprint (a later confirmation teaches with it) and name them if they
/// sound like a Saved person no one else in this meeting is. "You" can only be on the mic.
async fn recognise(db: &Db, note_id: Id, speaker_id: Id, channel: AudioChannel, embedding: &[f32]) -> margin_store::StoreResult<bool> {
    db.set_speaker_embedding(speaker_id, Some(embedding)).await?;
    let me = db.ensure_self_person().await?.id;
    let mut exclude: HashSet<Id> = db.list_speakers(note_id).await?.into_iter().filter(|s| s.id != speaker_id).filter_map(|s| s.person_id).collect();
    if channel == AudioChannel::Mic {
        exclude.remove(&me);
    } else {
        exclude.insert(me);
    }
    let Some((person, similarity)) = db.recognise_voice(embedding, RECOGNISE_THRESHOLD, &exclude).await? else { return Ok(false) };
    tracing::debug!(similarity, "recognised a Saved person's voice");
    db.assign_speaker(speaker_id, person).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tap_keeps_the_recent_audio_on_the_timeline() {
        let tap = VoiceTap::default();
        tap.push(AudioChannel::System, 1_000, &[1.0; 500]);
        tap.push(AudioChannel::System, 1_500, &[2.0; 500]);
        assert_eq!(tap.slice(AudioChannel::System, 1_400, 1_600), [vec![1.0; 100], vec![2.0; 100]].concat());
        assert!(tap.slice(AudioChannel::Mic, 1_400, 1_600).is_empty());
        // Older than the ring, or after it: only what is kept.
        assert_eq!(tap.slice(AudioChannel::System, 0, 1_010).len(), 10);
        assert!(tap.slice(AudioChannel::System, 5_000, 6_000).is_empty());
        // A gap starts afresh.
        tap.push(AudioChannel::System, 9_000, &[3.0; 10]);
        assert!(tap.slice(AudioChannel::System, 1_000, 2_000).is_empty());
        assert_eq!(tap.slice(AudioChannel::System, 9_000, 9_010), vec![3.0; 10]);
    }

    async fn meeting(db: &Db) -> Id {
        db.create_note(margin_store::NewNote { title: "Call".into(), kind: margin_domain::NoteKind::Meeting, ..Default::default() }).await.unwrap().id
    }

    async fn speaker(db: &Db, note: Id, label: &str) -> Id {
        db.create_speaker(note, margin_store::NewSpeaker { label: label.into(), color_index: 3, is_self: false, embedding: None }).await.unwrap().id
    }

    #[tokio::test]
    async fn a_known_voice_is_named_and_kept_for_teaching() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", Some(&[1.0, 0.0, 0.0])).await.unwrap();
        let note = meeting(&db).await;
        db.ensure_channel_speakers(note).await.unwrap();
        let p1 = speaker(&db, note, "Person 1").await;

        assert!(recognise(&db, note, p1, AudioChannel::System, &[0.95, 0.1, 0.0]).await.unwrap());
        assert_eq!(db.get_speaker(p1).await.unwrap().person_id, Some(sam.id));
        assert!(db.speaker_embedding(p1).await.unwrap().is_some(), "kept, so confirming teaches with it");
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1, "a guess teaches nothing");

        // Someone else who sounds like Sam is not named Sam twice in one meeting.
        let p2 = speaker(&db, note, "Person 2").await;
        assert!(!recognise(&db, note, p2, AudioChannel::System, &[0.95, 0.1, 0.0]).await.unwrap());
        // A voice nobody knows stays Person N.
        assert!(!recognise(&db, note, p2, AudioChannel::System, &[0.0, 0.0, 1.0]).await.unwrap());
    }

    #[tokio::test]
    async fn you_are_only_recognised_on_the_microphone() {
        let db = Db::open_in_memory().await.unwrap();
        let me = db.ensure_self_person().await.unwrap();
        let note = meeting(&db).await;
        let (you, _) = db.ensure_channel_speakers(note).await.unwrap();
        let s = db.create_speaker(note, margin_store::NewSpeaker { label: "x".into(), color_index: 1, is_self: false, embedding: Some(vec![0.0, 1.0]) }).await.unwrap();
        db.link_speaker(s.id, Some(me.id)).await.unwrap();
        let on_call = speaker(&db, note, "Person 1").await;
        assert!(!recognise(&db, note, on_call, AudioChannel::System, &[0.0, 1.0]).await.unwrap());
        let in_room = speaker(&db, note, "Person 2").await;
        assert!(recognise(&db, note, in_room, AudioChannel::Mic, &[0.0, 1.0]).await.unwrap());
        assert_eq!(db.get_speaker(in_room).await.unwrap().person_id, Some(me.id));
        assert_eq!(db.get_speaker(you.id).await.unwrap().person_id, Some(me.id));
    }

    #[test]
    fn the_tap_keeps_ninety_seconds() {
        let tap = VoiceTap::default();
        tap.push(AudioChannel::Mic, 0, &vec![0.5; KEEP + RATE]);
        assert!(tap.slice(AudioChannel::Mic, 0, RATE as u64).is_empty());
        assert_eq!(tap.slice(AudioChannel::Mic, RATE as u64, RATE as u64 * 2).len(), RATE);
    }
}
