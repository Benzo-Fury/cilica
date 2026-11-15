//! Voiceprints of the meeting's speakers, and recognising Saved people by them while the
//! meeting is live (docs/spec.md › Speakers and Saved people).
//!
//! The capture audio of each channel is kept for the last 90 seconds ([`VoiceTap`]). Every
//! speaker told apart (by Cilica or by the cloud service, "You" included) has the audio of their
//! finished lines gathered. After 3, 8 and 15 seconds of it Cilica makes a draft voiceprint from
//! everything so far (on this computer, always with the default CAM++ voice model, whichever
//! model tells speakers apart); from then on each further 20 seconds of speech is voiceprinted
//! and averaged in, and what is left when the meeting ends is added too. Each new voiceprint is
//! stored on the speaker ([`Db::set_speaker_embedding`]), which also replaces the sample the
//! speaker gave if someone confirmed who they are, so the person learns from the whole meeting.
//!
//! With Recognise saved voices off, only the voiceprints of speakers someone confirmed are kept.
//! With it on, a speaker no one chose for is compared with the Saved people
//! each time; a close enough match names them. It is only a guess: nothing is learnt from it
//! until the person confirms the name ([`cilica_store::Db::link_speaker`]), and it never goes
//! over someone's own pick or unlink.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use cilica_diarize::{RECOGNISE_THRESHOLD, SUGGEST_THRESHOLD};
use cilica_diarize::embedding::Embedder;
use cilica_domain::{AppEvent, AudioChannel, DataScope, Events, Id};
use cilica_store::Db;
use tokio::sync::mpsc;

use crate::backend::SpeechBackend;

const RATE: usize = 16_000;
/// Audio kept per channel.
const KEEP: usize = RATE * 90;
/// Seconds of a speaker's speech after which a draft voiceprint is made.
const TRIES_AT: [usize; 3] = [RATE * 3, RATE * 8, RATE * 15];
/// Speech per full voiceprint: the first replaces the drafts, later ones are averaged in.
const WINDOW: usize = RATE * 20;
/// The least speech left at the end of the meeting worth adding as one more window.
const MIN_LEFTOVER: usize = RATE * 8;

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

// ── Gathering speech ─────────────────────────────────────────────────────

/// One speaker's speech so far.
#[derive(Default)]
struct Pool {
    /// Speech not yet in a full window (all of it, before the first window).
    audio: Vec<f32>,
    heard: usize,
    /// Drafts made.
    tried: usize,
    /// Full windows made.
    windows: usize,
    /// Speech arrived since the last voiceprint.
    fresh: bool,
    /// The channel their speech came from.
    channel: Option<AudioChannel>,
}

impl Pool {
    /// Take in another stretch of speech.
    fn absorb(&mut self, other: Pool) {
        self.audio.extend(other.audio);
        self.heard += other.heard;
        self.tried = self.tried.max(other.tried);
        self.windows += other.windows;
        self.fresh |= other.fresh;
        self.channel = self.channel.or(other.channel);
    }

    /// The voiceprint due after new speech, if any.
    fn due(&mut self) -> Option<(Clip, Vec<f32>)> {
        if self.audio.len() >= WINDOW {
            let clip: Vec<f32> = self.audio.drain(..WINDOW).collect();
            self.windows += 1;
            self.fresh = !self.audio.is_empty();
            return Some((Clip::Window, clip));
        }
        if self.windows == 0 && TRIES_AT.get(self.tried).is_some_and(|&at| self.heard >= at) {
            while TRIES_AT.get(self.tried).is_some_and(|&at| self.heard >= at) {
                self.tried += 1;
            }
            self.fresh = false;
            return Some((Clip::Draft, self.audio.clone()));
        }
        None
    }

    /// The voiceprint for what is left when the meeting ends, if it is worth one.
    fn leftover(&mut self) -> Option<(Clip, Vec<f32>)> {
        if !self.fresh {
            return None;
        }
        self.fresh = false;
        if self.windows == 0 && self.audio.len() >= TRIES_AT[0] {
            Some((Clip::Draft, std::mem::take(&mut self.audio)))
        } else if self.windows > 0 && self.audio.len() >= MIN_LEFTOVER {
            Some((Clip::Window, std::mem::take(&mut self.audio)))
        } else {
            None
        }
    }
}

/// What a clip's voiceprint is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clip {
    /// All the speech so far, before there is a full window: replaces the voiceprint.
    Draft,
    /// One full window (or the last of the speech): averaged with the other windows.
    Window,
}

struct Job {
    speaker_id: Id,
    channel: AudioChannel,
    clip: Vec<f32>,
    kind: Clip,
}

/// Collects speakers' audio and hands it to a worker that voiceprints and recognises them.
pub(crate) struct Recogniser {
    db: Db,
    tap: Arc<VoiceTap>,
    pools: HashMap<Id, Pool>,
    jobs: Option<mpsc::UnboundedSender<Job>>,
    worker: tokio::task::JoinHandle<()>,
}

impl Recogniser {
    /// `recognise`: compare voices with the Saved people and name the speakers that match.
    /// Without it voiceprints are still made, so confirming who someone is teaches.
    pub fn start(db: Db, events: Events, speech: Arc<dyn SpeechBackend>, note_id: Id, tap: Arc<VoiceTap>, recognise: bool, rt: &tokio::runtime::Handle) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = rt.spawn(work(db.clone(), events, speech, note_id, rx, recognise));
        Self { db, tap, pools: HashMap::new(), jobs: Some(tx), worker }
    }

    /// A finished line by a told-apart speaker, at these recording-timeline times.
    pub fn heard(&mut self, speaker_id: Id, channel: AudioChannel, start_ms: u32, end_ms: u32) {
        self.follow_merges();
        let speaker_id = self.db.resolve_speaker(speaker_id);
        let clip = self.tap.slice(channel, start_ms as u64 * 16, end_ms as u64 * 16);
        if clip.is_empty() {
            return;
        }
        let pool = self.pools.entry(speaker_id).or_default();
        pool.heard += clip.len();
        pool.audio.extend(clip);
        pool.fresh = true;
        pool.channel = Some(channel);
        if let Some((kind, clip)) = pool.due() {
            self.send(Job { speaker_id, channel, clip, kind });
        }
    }

    /// Speakers found to be one person share one pool.
    fn follow_merges(&mut self) {
        let moved: Vec<(Id, Id)> = self.pools.keys().map(|&id| (id, self.db.resolve_speaker(id))).filter(|(a, b)| a != b).collect();
        for (gone, keep) in moved {
            if let Some(pool) = self.pools.remove(&gone) {
                self.pools.entry(keep).or_default().absorb(pool);
            }
        }
    }

    fn send(&self, job: Job) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }

    /// Voiceprint what is left of everyone's speech, and let the last voiceprints finish.
    pub async fn finish(mut self) {
        self.follow_merges();
        let mut left = Vec::new();
        for (&speaker_id, pool) in self.pools.iter_mut() {
            if let (Some(channel), Some((kind, clip))) = (pool.channel, pool.leftover()) {
                left.push(Job { speaker_id, channel, clip, kind });
            }
        }
        for job in left {
            self.send(job);
        }
        self.jobs.take();
        let _ = self.worker.await;
    }
}

// ── Voiceprinting and naming ─────────────────────────────────────────────

/// The windows of one speaker voiceprinted so far.
#[derive(Default)]
struct Print {
    sum: Vec<f32>,
    windows: usize,
    /// Speech in the windows, in samples.
    heard: usize,
}

impl Print {
    /// Take in a new voiceprint made from `clip_len` samples of speech; returns the speaker's
    /// voiceprint now and how much speech it was made from, or `None` when a late draft has
    /// nothing to add over the windows.
    fn add(&mut self, kind: Clip, embedding: &[f32], clip_len: usize) -> Option<(Vec<f32>, usize)> {
        match kind {
            Clip::Draft if self.windows > 0 => None,
            Clip::Draft => Some((embedding.to_vec(), clip_len)),
            Clip::Window => {
                if self.sum.len() != embedding.len() {
                    self.sum = vec![0.0; embedding.len()];
                    self.windows = 0;
                    self.heard = 0;
                }
                for (s, e) in self.sum.iter_mut().zip(embedding) {
                    *s += e;
                }
                self.windows += 1;
                self.heard += clip_len;
                Some((normalize(&self.sum), self.heard))
            }
        }
    }

    fn absorb(&mut self, other: Print) {
        if other.windows == 0 {
            return;
        }
        if self.windows == 0 || self.sum.len() != other.sum.len() {
            *self = other;
            return;
        }
        for (s, o) in self.sum.iter_mut().zip(&other.sum) {
            *s += o;
        }
        self.windows += other.windows;
        self.heard += other.heard;
    }
}

fn normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON { v.to_vec() } else { v.iter().map(|x| x / norm).collect() }
}

async fn work(db: Db, events: Events, speech: Arc<dyn SpeechBackend>, note_id: Id, mut jobs: mpsc::UnboundedReceiver<Job>, recognise_on: bool) {
    let mut diarizer: Option<Arc<Mutex<Embedder>>> = None;
    let mut unavailable = false;
    let mut prints: HashMap<Id, Print> = HashMap::new();
    while let Some(job) = jobs.recv().await {
        if unavailable {
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
        let clip_len = clip.len();
        let embedding = match tokio::task::spawn_blocking(move || d.lock().map_err(|_| ()).and_then(|mut d| d.embed(&clip).map_err(|_| ()))).await {
            Ok(Ok(e)) => e,
            _ => continue,
        };
        // Speakers joined since this audio was heard voiceprint as one.
        let moved: Vec<(Id, Id)> = prints.keys().map(|&id| (id, db.resolve_speaker(id))).filter(|(a, b)| a != b).collect();
        for (gone, keep) in moved {
            if let Some(p) = prints.remove(&gone) {
                prints.entry(keep).or_default().absorb(p);
            }
        }
        let speaker_id = db.resolve_speaker(job.speaker_id);
        let Some((voiceprint, heard)) = prints.entry(speaker_id).or_default().add(job.kind, &embedding, clip_len) else { continue };
        let speech_ms = (heard / (RATE / 1000)).min(u32::MAX as usize) as u32;
        match voiced(&db, note_id, speaker_id, job.channel, &voiceprint, speech_ms, recognise_on).await {
            Ok(changed) => {
                if changed {
                    events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Transcript, DataScope::People], note_id: Some(note_id) });
                }
            }
            Err(error) => tracing::warn!(%error, "could not keep a speaker's voiceprint"),
        }
    }
}

/// The voice model, fetched and loaded on first use. `None` when it cannot be had.
async fn load(speech: &Arc<dyn SpeechBackend>) -> Option<Arc<Mutex<Embedder>>> {
    let (_, embedding) = speech.speaker_models().await?;
    let speech = speech.clone();
    match tokio::task::spawn_blocking(move || speech.voice_embedder(&embedding)).await {
        Ok(Ok(d)) => Some(Arc::new(Mutex::new(d))),
        Ok(Err(error)) => {
            tracing::warn!(message = %error.message, "voice recognition is off for this meeting");
            None
        }
        Err(_) => None,
    }
}

/// Keep the speaker's new voiceprint (it teaches if someone confirmed who they are) and, when
/// `recognise_on`, try to name them. With recognition off, only the voiceprints of speakers
/// someone confirmed are kept: no one else's voice is stored. Returns whether what people see
/// changed.
async fn voiced(db: &Db, note_id: Id, speaker_id: Id, channel: AudioChannel, embedding: &[f32], speech_ms: u32, recognise_on: bool) -> cilica_store::StoreResult<bool> {
    let taught = db.speaker_teaches(speaker_id).await?;
    if !recognise_on && !taught {
        return Ok(false);
    }
    db.set_speaker_embedding(speaker_id, Some(embedding), speech_ms).await?;
    let named = recognise_on && recognise(db, note_id, speaker_id, channel, embedding).await?;
    Ok(taught || named)
}

/// Name the speaker from their voice, or revise Cilica's earlier guess as the voiceprint
/// sharpens: keep it while it is still a clear match; move it to someone who now clearly
/// matches; take it back once they no longer sound alike. A speaker someone chose for is left
/// as they are. One speaker per person in a meeting; "You" only on the mic. Returns whether the
/// speaker's name changed.
async fn recognise(db: &Db, note_id: Id, speaker_id: Id, channel: AudioChannel, embedding: &[f32]) -> cilica_store::StoreResult<bool> {
    let speakers = db.list_speakers(note_id).await?;
    let Some(speaker) = speakers.iter().find(|s| s.id == speaker_id) else { return Ok(false) };
    if db.speaker_picked(speaker_id).await? {
        return Ok(false);
    }
    let guess = db.speaker_guess(speaker_id).await?;
    // Someone linked without a guess (a meeting from before guesses were kept) stays.
    if speaker.person_id.is_some() && guess.is_none() {
        return Ok(false);
    }
    let me = db.ensure_self_person().await?.id;
    let mut exclude: HashSet<Id> = speakers.iter().filter(|s| s.id != speaker_id).filter_map(|s| s.person_id).collect();
    if channel == AudioChannel::Mic {
        exclude.remove(&me);
    } else {
        exclude.insert(me);
    }
    let best = db.recognise_voice(embedding, RECOGNISE_THRESHOLD, &exclude).await?;
    match (guess, best) {
        (Some(current), Some((person, _))) if person == current => Ok(false),
        (Some(current), best) => {
            let still = db.voice_similarity(current, embedding).await?.unwrap_or(0.0);
            if still >= RECOGNISE_THRESHOLD {
                return Ok(false);
            }
            if let Some((person, similarity)) = best {
                tracing::debug!(similarity, "a voice sounds more like someone else now");
                return db.assign_speaker(speaker_id, person).await;
            }
            if still < SUGGEST_THRESHOLD {
                tracing::debug!(still, "a voice no longer sounds like the person Cilica named");
                return db.clear_guess(speaker_id).await;
            }
            Ok(false)
        }
        (None, Some((person, similarity))) => {
            tracing::debug!(similarity, "recognised a Saved person's voice");
            db.assign_speaker(speaker_id, person).await
        }
        (None, None) => Ok(false),
    }
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
        db.create_note(cilica_store::NewNote { title: "Call".into(), kind: cilica_domain::NoteKind::Meeting, ..Default::default() }).await.unwrap().id
    }

    async fn speaker(db: &Db, note: Id, label: &str) -> Id {
        db.create_speaker(note, cilica_store::NewSpeaker { label: label.into(), color_index: 3, is_self: false, embedding: None }).await.unwrap().id
    }

    #[tokio::test]
    async fn a_known_voice_is_named_and_kept_for_teaching() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", Some(&[1.0, 0.0, 0.0])).await.unwrap();
        let note = meeting(&db).await;
        db.ensure_channel_speakers(note).await.unwrap();
        let p1 = speaker(&db, note, "Person 1").await;

        assert!(voiced(&db, note, p1, AudioChannel::System, &[0.95, 0.1, 0.0], 3_000, true).await.unwrap());
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
        let s = db.create_speaker(note, cilica_store::NewSpeaker { label: "x".into(), color_index: 1, is_self: false, embedding: Some(vec![0.0, 1.0]) }).await.unwrap();
        db.link_speaker(s.id, Some(me.id)).await.unwrap();
        let on_call = speaker(&db, note, "Person 1").await;
        assert!(!recognise(&db, note, on_call, AudioChannel::System, &[0.0, 1.0]).await.unwrap());
        let in_room = speaker(&db, note, "Person 2").await;
        assert!(recognise(&db, note, in_room, AudioChannel::Mic, &[0.0, 1.0]).await.unwrap());
        assert_eq!(db.get_speaker(in_room).await.unwrap().person_id, Some(me.id));
        assert_eq!(db.get_speaker(you.id).await.unwrap().person_id, Some(me.id));
    }

    /// Every new voiceprint is kept; a confirmed speaker's replaces their sample; an unlinked
    /// speaker is not guessed again; with recognition off nobody is named but voices still teach.
    #[tokio::test]
    async fn voiceprints_teach_confirmed_speakers_and_respect_unlinks() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", Some(&[1.0, 0.0])).await.unwrap();
        let kim = db.create_person("Kim", None).await.unwrap();
        let note = meeting(&db).await;
        db.ensure_channel_speakers(note).await.unwrap();

        let p1 = speaker(&db, note, "Person 1").await;
        assert!(voiced(&db, note, p1, AudioChannel::System, &[1.0, 0.1], 3_000, true).await.unwrap(), "named Sam");
        db.link_speaker(p1, None).await.unwrap();
        assert!(!voiced(&db, note, p1, AudioChannel::System, &[1.0, 0.0], 3_000, true).await.unwrap());
        assert!(db.get_speaker(p1).await.unwrap().person_id.is_none(), "not Sam, said the person");

        let p2 = speaker(&db, note, "Person 2").await;
        db.link_speaker(p2, Some(kim.id)).await.unwrap();
        assert!(voiced(&db, note, p2, AudioChannel::System, &[0.0, 1.0], 3_000, false).await.unwrap(), "Kim learnt");
        assert_eq!(db.get_person(kim.id).await.unwrap().sample_count, 1);
        voiced(&db, note, p2, AudioChannel::System, &[0.1, 1.0], 3_000, false).await.unwrap();
        assert_eq!(db.get_person(kim.id).await.unwrap().sample_count, 1, "replaced, not added");
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1, "Sam never learnt");

        let p3 = speaker(&db, note, "Person 3").await;
        assert!(!voiced(&db, note, p3, AudioChannel::System, &[1.0, 0.0], 3_000, false).await.unwrap(), "recognition off");
        assert!(db.speaker_embedding(p3).await.unwrap().is_none(), "nobody confirmed: their voice is not kept");
    }

    /// A guess made from a few seconds follows the sharper voiceprints: kept while it still
    /// matches, moved when someone else clearly matches, taken back when no one does. It counts
    /// as Cilica naming them only while it stands.
    #[tokio::test]
    async fn guesses_are_revised_as_the_voiceprint_sharpens() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", Some(&[1.0, 0.0, 0.0])).await.unwrap();
        let kim = db.create_person("Kim", Some(&[0.0, 1.0, 0.0])).await.unwrap();
        let note = meeting(&db).await;
        let p1 = speaker(&db, note, "Person 1").await;

        assert!(voiced(&db, note, p1, AudioChannel::System, &[1.0, 0.2, 0.0], 3_000, true).await.unwrap());
        assert_eq!(db.get_speaker(p1).await.unwrap().person_id, Some(sam.id));
        assert!(!voiced(&db, note, p1, AudioChannel::System, &[1.0, 0.3, 0.0], 8_000, true).await.unwrap(), "still Sam");
        assert!(voiced(&db, note, p1, AudioChannel::System, &[0.2, 1.0, 0.0], 20_000, true).await.unwrap(), "Kim after all");
        assert_eq!(db.get_speaker(p1).await.unwrap().person_id, Some(kim.id));
        assert!(voiced(&db, note, p1, AudioChannel::System, &[0.0, 0.0, 1.0], 40_000, true).await.unwrap(), "no one");
        assert!(db.get_speaker(p1).await.unwrap().person_id.is_none());
        assert_eq!(db.person_voice(kim.id).await.unwrap().named, 0);

        // Confirmed, the voiceprint teaches with how much speech it holds.
        db.link_speaker(p1, Some(kim.id)).await.unwrap();
        voiced(&db, note, p1, AudioChannel::System, &[0.0, 1.0, 0.1], 60_000, true).await.unwrap();
        assert_eq!(db.get_speaker(p1).await.unwrap().person_id, Some(kim.id), "a pick is never revised");
        assert_eq!(db.person_voice(kim.id).await.unwrap().speech_ms, 60_000);
    }

    #[test]
    fn a_pool_drafts_then_windows_then_leftovers() {
        let mut pool = Pool::default();
        let feed = |pool: &mut Pool, secs: usize| {
            pool.audio.extend(vec![0.0; RATE * secs]);
            pool.heard += RATE * secs;
            pool.fresh = true;
            pool.due().map(|(kind, clip)| (kind, clip.len() / RATE))
        };
        assert_eq!(feed(&mut pool, 2), None);
        assert_eq!(feed(&mut pool, 2), Some((Clip::Draft, 4)));
        // A long line passes two checkpoints at once: one draft.
        assert_eq!(feed(&mut pool, 12), Some((Clip::Draft, 16)));
        assert_eq!(feed(&mut pool, 2), None);
        assert_eq!(feed(&mut pool, 3), Some((Clip::Window, 20)));
        assert_eq!(feed(&mut pool, 10), None);
        assert_eq!(feed(&mut pool, 9), Some((Clip::Window, 20)));
        // 0 s left: nothing more at the end. Then 9 s: one more window.
        assert!(pool.leftover().is_none());
        assert_eq!(feed(&mut pool, 9), None);
        assert_eq!(pool.leftover().map(|(k, c)| (k, c.len() / RATE)), Some((Clip::Window, 9)));

        // Short of a window, the end makes a draft of everything if it is long enough.
        let mut short = Pool::default();
        assert_eq!(feed(&mut short, 5), Some((Clip::Draft, 5)));
        assert!(short.leftover().is_none(), "nothing new since");
        assert_eq!(feed(&mut short, 1), None);
        assert_eq!(short.leftover().map(|(k, c)| (k, c.len() / RATE)), Some((Clip::Draft, 6)));
    }

    #[test]
    fn windows_are_averaged_and_late_drafts_ignored() {
        let mut print = Print::default();
        assert_eq!(print.add(Clip::Draft, &[1.0, 0.0], 5), Some((vec![1.0, 0.0], 5)));
        assert_eq!(print.add(Clip::Window, &[1.0, 0.0], 20), Some((vec![1.0, 0.0], 20)));
        let (mean, heard) = print.add(Clip::Window, &[0.0, 1.0], 20).unwrap();
        assert!((mean[0] - mean[1]).abs() < 1e-6 && (mean[0] - 0.70710677).abs() < 1e-6);
        assert_eq!(heard, 40, "the windows' speech adds up");
        assert_eq!(print.add(Clip::Draft, &[0.0, 1.0], 9), None);
    }

    #[test]
    fn the_tap_keeps_ninety_seconds() {
        let tap = VoiceTap::default();
        tap.push(AudioChannel::Mic, 0, &vec![0.5; KEEP + RATE]);
        assert!(tap.slice(AudioChannel::Mic, 0, RATE as u64).is_empty());
        assert_eq!(tap.slice(AudioChannel::Mic, RATE as u64, RATE as u64 * 2).len(), RATE);
    }
}
