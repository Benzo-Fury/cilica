//! The note, category, transcript and people operations the assistant's tools need, behind a trait:
//! [`DbNoteStore`] uses the real database; [`FakeNotes`] is an in-memory stand-in for tests.
//!
//! The assistant's own tables (threads, messages, proposals, audit log, tasks) always go through
//! `margin_store::Db` directly; only areas owned by other engineers sit behind this seam.

use std::collections::HashMap;
use std::sync::Mutex;

use margin_domain::{
    AppError, AppResult, Author, Category, Id, Note, NoteKind, NoteSummary, NoteVersion, Person, SearchHit, SearchSource,
    Speaker, new_id, now,
};
use margin_store::{Db, NewNote};

#[async_trait::async_trait]
pub trait NoteStore: Send + Sync {
    async fn list_notes(&self) -> AppResult<Vec<NoteSummary>>;
    async fn get_note(&self, id: Id) -> AppResult<Note>;
    async fn create_note(&self, new: NewNote) -> AppResult<Note>;
    async fn rename_note(&self, id: Id, title: &str) -> AppResult<NoteSummary>;
    async fn set_note_body(&self, id: Id, body_md: &str, author: Author, ai_action_id: Option<Id>) -> AppResult<NoteSummary>;
    async fn set_note_snippet(&self, id: Id, snippet: &str) -> AppResult<()>;
    async fn snapshot_note(&self, id: Id, author: Author, ai_action_id: Option<Id>) -> AppResult<()>;
    async fn move_note(&self, id: Id, category_id: Option<Id>, position: f64) -> AppResult<NoteSummary>;
    async fn trash_note(&self, id: Id) -> AppResult<()>;
    async fn restore_note(&self, id: Id) -> AppResult<NoteSummary>;
    async fn list_categories(&self) -> AppResult<Vec<Category>>;
    async fn get_category(&self, id: Id) -> AppResult<Category>;
    async fn create_category(&self, name: &str) -> AppResult<Category>;
    async fn rename_category(&self, id: Id, name: &str) -> AppResult<Category>;
    async fn reorder_category(&self, id: Id, position: f64) -> AppResult<Category>;
    async fn delete_category(&self, id: Id) -> AppResult<()>;
    async fn list_note_versions(&self, note_id: Id) -> AppResult<Vec<NoteVersion>>;
    async fn search_notes(&self, query: &str, limit: u32) -> AppResult<Vec<SearchHit>>;
    /// `[12:34] Sam: text` lines; empty when there is no transcript.
    async fn transcript_text(&self, note_id: Id) -> AppResult<String>;
    async fn list_speakers(&self, note_id: Id) -> AppResult<Vec<Speaker>>;
    async fn rename_speaker(&self, speaker_id: Id, label: &str) -> AppResult<Speaker>;
    async fn link_speaker(&self, speaker_id: Id, person_id: Option<Id>) -> AppResult<Speaker>;
    async fn list_people(&self) -> AppResult<Vec<Person>>;
    async fn get_person(&self, person_id: Id) -> AppResult<Person>;
    async fn delete_person(&self, person_id: Id) -> AppResult<()>;
}

/// The real store.
pub struct DbNoteStore(pub Db);

#[async_trait::async_trait]
impl NoteStore for DbNoteStore {
    async fn list_notes(&self) -> AppResult<Vec<NoteSummary>> {
        Ok(self.0.list_notes().await?)
    }
    async fn get_note(&self, id: Id) -> AppResult<Note> {
        Ok(self.0.get_note(id).await?)
    }
    async fn create_note(&self, new: NewNote) -> AppResult<Note> {
        Ok(self.0.create_note(new).await?)
    }
    async fn rename_note(&self, id: Id, title: &str) -> AppResult<NoteSummary> {
        Ok(self.0.rename_note(id, title).await?)
    }
    async fn set_note_body(&self, id: Id, body_md: &str, author: Author, ai_action_id: Option<Id>) -> AppResult<NoteSummary> {
        Ok(self.0.set_note_body(id, body_md, author, ai_action_id).await?)
    }
    async fn set_note_snippet(&self, id: Id, snippet: &str) -> AppResult<()> {
        Ok(self.0.set_note_snippet(id, snippet).await?)
    }
    async fn snapshot_note(&self, id: Id, author: Author, ai_action_id: Option<Id>) -> AppResult<()> {
        self.0.snapshot_note(id, author, ai_action_id).await?;
        Ok(())
    }
    async fn move_note(&self, id: Id, category_id: Option<Id>, position: f64) -> AppResult<NoteSummary> {
        Ok(self.0.move_note(id, category_id, position).await?)
    }
    async fn trash_note(&self, id: Id) -> AppResult<()> {
        Ok(self.0.trash_note(id).await?)
    }
    async fn restore_note(&self, id: Id) -> AppResult<NoteSummary> {
        Ok(self.0.restore_note(id).await?)
    }
    async fn list_categories(&self) -> AppResult<Vec<Category>> {
        Ok(self.0.list_categories().await?)
    }
    async fn get_category(&self, id: Id) -> AppResult<Category> {
        Ok(self.0.get_category(id).await?)
    }
    async fn create_category(&self, name: &str) -> AppResult<Category> {
        Ok(self.0.create_category(name).await?)
    }
    async fn rename_category(&self, id: Id, name: &str) -> AppResult<Category> {
        Ok(self.0.rename_category(id, name).await?)
    }
    async fn reorder_category(&self, id: Id, position: f64) -> AppResult<Category> {
        Ok(self.0.reorder_category(id, position).await?)
    }
    async fn delete_category(&self, id: Id) -> AppResult<()> {
        Ok(self.0.delete_category(id).await?)
    }
    async fn list_note_versions(&self, note_id: Id) -> AppResult<Vec<NoteVersion>> {
        Ok(self.0.list_note_versions(note_id).await?)
    }
    async fn search_notes(&self, query: &str, limit: u32) -> AppResult<Vec<SearchHit>> {
        Ok(self.0.search_notes(query, limit).await?)
    }
    async fn transcript_text(&self, note_id: Id) -> AppResult<String> {
        Ok(self.0.transcript_text(note_id).await?)
    }
    async fn list_speakers(&self, note_id: Id) -> AppResult<Vec<Speaker>> {
        Ok(self.0.list_speakers(note_id).await?)
    }
    async fn rename_speaker(&self, speaker_id: Id, label: &str) -> AppResult<Speaker> {
        Ok(self.0.rename_speaker(speaker_id, label).await?)
    }
    async fn link_speaker(&self, speaker_id: Id, person_id: Option<Id>) -> AppResult<Speaker> {
        Ok(self.0.link_speaker(speaker_id, person_id).await?)
    }
    async fn list_people(&self) -> AppResult<Vec<Person>> {
        Ok(self.0.list_people().await?)
    }
    async fn get_person(&self, person_id: Id) -> AppResult<Person> {
        Ok(self.0.get_person(person_id).await?)
    }
    async fn delete_person(&self, person_id: Id) -> AppResult<()> {
        Ok(self.0.delete_person(person_id).await?)
    }
}

// ── In-memory fake ───────────────────────────────────────────────────────

#[derive(Default)]
struct FakeState {
    notes: Vec<Note>,
    categories: Vec<Category>,
    versions: Vec<NoteVersion>,
    transcripts: HashMap<Id, String>,
    speakers: Vec<Speaker>,
    people: Vec<Person>,
}

/// An in-memory [`NoteStore`] with just enough behaviour for tests: versions on AI writes,
/// soft delete, substring search.
#[derive(Default)]
pub struct FakeNotes {
    state: Mutex<FakeState>,
}

fn summary(n: &Note) -> NoteSummary {
    NoteSummary {
        id: n.id,
        title: n.title.clone(),
        snippet: n.snippet.clone(),
        category_id: n.category_id,
        kind: n.kind,
        meeting_app: n.meeting_app.clone(),
        position: n.position,
        created_at: n.created_at,
        updated_at: n.updated_at,
        deleted_at: n.deleted_at,
    }
}

fn missing(what: &str) -> AppError {
    AppError::not_found(format!("{what} was not found."))
}

impl FakeNotes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a note directly (test setup).
    pub fn add_note(&self, title: &str, body: &str) -> Note {
        let mut s = self.state.lock().unwrap();
        let note = Note {
            id: new_id(),
            title: title.into(),
            body_md: body.into(),
            snippet: body.lines().next().unwrap_or("").chars().take(120).collect(),
            category_id: None,
            kind: NoteKind::Manual,
            meeting_app: None,
            started_at: None,
            ended_at: None,
            position: s.notes.len() as f64,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        };
        s.notes.push(note.clone());
        note
    }

    pub fn add_meeting(&self, title: &str, body: &str, transcript: &str) -> Note {
        let note = self.add_note(title, body);
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == note.id).unwrap();
        n.kind = NoteKind::Meeting;
        n.meeting_app = Some("Zoom".into());
        n.snippet.clear();
        let note = n.clone();
        s.transcripts.insert(note.id, transcript.into());
        note
    }

    pub fn add_speaker(&self, note_id: Id, label: &str) -> Speaker {
        let speaker = Speaker { id: new_id(), note_id, label: label.into(), person_id: None, color_index: 1, is_self: false };
        self.state.lock().unwrap().speakers.push(speaker.clone());
        speaker
    }

    pub fn add_person(&self, name: &str) -> Person {
        let person = Person { id: new_id(), name: name.into(), color_index: 1, sample_count: 1, meeting_count: 1, created_at: now() };
        self.state.lock().unwrap().people.push(person.clone());
        person
    }

    pub fn note(&self, id: Id) -> Note {
        self.state.lock().unwrap().notes.iter().find(|n| n.id == id).cloned().expect("note")
    }

    /// Notes not in Trash.
    pub fn live_count(&self) -> usize {
        self.state.lock().unwrap().notes.iter().filter(|n| n.deleted_at.is_none()).count()
    }

    pub fn versions(&self, note_id: Id) -> Vec<NoteVersion> {
        self.state.lock().unwrap().versions.iter().filter(|v| v.note_id == note_id).cloned().collect()
    }

    pub fn categories(&self) -> Vec<Category> {
        self.state.lock().unwrap().categories.clone()
    }

    pub fn speakers(&self) -> Vec<Speaker> {
        self.state.lock().unwrap().speakers.clone()
    }

    pub fn people(&self) -> Vec<Person> {
        self.state.lock().unwrap().people.clone()
    }

    /// Edit a note as the user would (no AI version).
    pub fn user_edit(&self, id: Id, body: &str) {
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == id).unwrap();
        n.body_md = body.into();
        n.updated_at = now();
    }

    fn snapshot(s: &mut FakeState, id: Id, author: Author, ai_action_id: Option<Id>) -> AppResult<()> {
        let n = s.notes.iter().find(|n| n.id == id).ok_or_else(|| missing("That note"))?.clone();
        s.versions.push(NoteVersion { id: new_id(), note_id: id, title: n.title, body_md: n.body_md, author, ai_action_id, created_at: now() });
        Ok(())
    }
}

#[async_trait::async_trait]
impl NoteStore for FakeNotes {
    async fn list_notes(&self) -> AppResult<Vec<NoteSummary>> {
        let s = self.state.lock().unwrap();
        let mut out: Vec<_> = s.notes.iter().filter(|n| n.deleted_at.is_none()).map(summary).collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }
    async fn get_note(&self, id: Id) -> AppResult<Note> {
        self.state.lock().unwrap().notes.iter().find(|n| n.id == id).cloned().ok_or_else(|| missing("That note"))
    }
    async fn create_note(&self, new: NewNote) -> AppResult<Note> {
        let note = self.add_note(&new.title, &new.body_md);
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == note.id).unwrap();
        n.category_id = new.category_id;
        n.kind = new.kind;
        Ok(n.clone())
    }
    async fn rename_note(&self, id: Id, title: &str) -> AppResult<NoteSummary> {
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| missing("That note"))?;
        n.title = title.into();
        n.updated_at = now();
        Ok(summary(n))
    }
    async fn set_note_body(&self, id: Id, body_md: &str, author: Author, ai_action_id: Option<Id>) -> AppResult<NoteSummary> {
        let mut s = self.state.lock().unwrap();
        if author == Author::Ai {
            Self::snapshot(&mut s, id, author, ai_action_id)?;
        }
        let n = s.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| missing("That note"))?;
        n.body_md = body_md.into();
        n.updated_at = now();
        Ok(summary(n))
    }
    async fn set_note_snippet(&self, id: Id, snippet: &str) -> AppResult<()> {
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| missing("That note"))?;
        n.snippet = snippet.into();
        Ok(())
    }
    async fn snapshot_note(&self, id: Id, author: Author, ai_action_id: Option<Id>) -> AppResult<()> {
        Self::snapshot(&mut self.state.lock().unwrap(), id, author, ai_action_id)
    }
    async fn move_note(&self, id: Id, category_id: Option<Id>, position: f64) -> AppResult<NoteSummary> {
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| missing("That note"))?;
        n.category_id = category_id;
        n.position = position;
        Ok(summary(n))
    }
    async fn trash_note(&self, id: Id) -> AppResult<()> {
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| missing("That note"))?;
        n.deleted_at = Some(now());
        Ok(())
    }
    async fn restore_note(&self, id: Id) -> AppResult<NoteSummary> {
        let mut s = self.state.lock().unwrap();
        let n = s.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| missing("That note"))?;
        n.deleted_at = None;
        Ok(summary(n))
    }
    async fn list_categories(&self) -> AppResult<Vec<Category>> {
        Ok(self.categories())
    }
    async fn get_category(&self, id: Id) -> AppResult<Category> {
        self.state.lock().unwrap().categories.iter().find(|c| c.id == id).cloned().ok_or_else(|| missing("That category"))
    }
    async fn create_category(&self, name: &str) -> AppResult<Category> {
        let mut s = self.state.lock().unwrap();
        let c = Category { id: new_id(), name: name.into(), position: s.categories.len() as f64, collapsed: false };
        s.categories.push(c.clone());
        Ok(c)
    }
    async fn rename_category(&self, id: Id, name: &str) -> AppResult<Category> {
        let mut s = self.state.lock().unwrap();
        let c = s.categories.iter_mut().find(|c| c.id == id).ok_or_else(|| missing("That category"))?;
        c.name = name.into();
        Ok(c.clone())
    }
    async fn reorder_category(&self, id: Id, position: f64) -> AppResult<Category> {
        let mut s = self.state.lock().unwrap();
        let c = s.categories.iter_mut().find(|c| c.id == id).ok_or_else(|| missing("That category"))?;
        c.position = position;
        Ok(c.clone())
    }
    async fn delete_category(&self, id: Id) -> AppResult<()> {
        let mut s = self.state.lock().unwrap();
        let before = s.categories.len();
        s.categories.retain(|c| c.id != id);
        if before == s.categories.len() {
            return Err(missing("That category"));
        }
        for n in s.notes.iter_mut().filter(|n| n.category_id == Some(id)) {
            n.category_id = None;
        }
        Ok(())
    }
    async fn list_note_versions(&self, note_id: Id) -> AppResult<Vec<NoteVersion>> {
        let mut v = self.versions(note_id);
        v.reverse();
        Ok(v)
    }
    async fn search_notes(&self, query: &str, limit: u32) -> AppResult<Vec<SearchHit>> {
        let q = query.to_lowercase();
        let s = self.state.lock().unwrap();
        Ok(s.notes
            .iter()
            .filter(|n| n.deleted_at.is_none())
            .filter_map(|n| {
                let transcript = s.transcripts.get(&n.id).cloned().unwrap_or_default();
                let source = if n.title.to_lowercase().contains(&q) {
                    SearchSource::Title
                } else if n.body_md.to_lowercase().contains(&q) {
                    SearchSource::Body
                } else if transcript.to_lowercase().contains(&q) {
                    SearchSource::Transcript
                } else {
                    return None;
                };
                Some(SearchHit { note: summary(n), source, excerpt: n.snippet.clone(), at_ms: None })
            })
            .take(limit as usize)
            .collect())
    }
    async fn transcript_text(&self, note_id: Id) -> AppResult<String> {
        Ok(self.state.lock().unwrap().transcripts.get(&note_id).cloned().unwrap_or_default())
    }
    async fn list_speakers(&self, note_id: Id) -> AppResult<Vec<Speaker>> {
        Ok(self.state.lock().unwrap().speakers.iter().filter(|s| s.note_id == note_id).cloned().collect())
    }
    async fn rename_speaker(&self, speaker_id: Id, label: &str) -> AppResult<Speaker> {
        let mut s = self.state.lock().unwrap();
        let sp = s.speakers.iter_mut().find(|s| s.id == speaker_id).ok_or_else(|| missing("That speaker"))?;
        sp.label = label.into();
        Ok(sp.clone())
    }
    async fn link_speaker(&self, speaker_id: Id, person_id: Option<Id>) -> AppResult<Speaker> {
        let mut s = self.state.lock().unwrap();
        let sp = s.speakers.iter_mut().find(|s| s.id == speaker_id).ok_or_else(|| missing("That speaker"))?;
        sp.person_id = person_id;
        Ok(sp.clone())
    }
    async fn list_people(&self) -> AppResult<Vec<Person>> {
        Ok(self.people())
    }
    async fn get_person(&self, person_id: Id) -> AppResult<Person> {
        self.state.lock().unwrap().people.iter().find(|p| p.id == person_id).cloned().ok_or_else(|| missing("That person"))
    }
    async fn delete_person(&self, person_id: Id) -> AppResult<()> {
        let mut s = self.state.lock().unwrap();
        s.people.retain(|p| p.id != person_id);
        for sp in s.speakers.iter_mut().filter(|sp| sp.person_id == Some(person_id)) {
            sp.person_id = None;
        }
        Ok(())
    }
}
