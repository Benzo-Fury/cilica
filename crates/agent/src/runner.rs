//! Runs tool calls with the guardrails enforced in code, whatever the model or prompt says:
//!
//! - Read runs freely.
//! - Additive applies at once and records undo data.
//! - Edit becomes a diff proposal unless `auto_apply_edits` is on (still versioned).
//! - Destructive always becomes a confirm proposal.
//! - A write that would take the turn past 5 distinct existing notes becomes Destructive.
//! - At most [`MAX_WRITES_PER_TURN`] writes per turn.
//! - In propose-only mode (scheduled tasks) every write becomes a proposal, and suggestions that
//!   were dismissed (or are still pending) are not made again.
//!
//! Every call writes an `ai_actions` audit row and emits the matching events.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use margin_domain::{
    AiAction, AiActionStatus, AppEvent, AppResult, AssistantEvent, DataScope, Events, Id, NoteRef, Proposal,
    ProposalSource, ProposalStatus, TaskDraft, TaskSchedule, TaskScope, ToolTier, new_id, now,
};
use margin_store::Db;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::ops::{self, WriteOp};
use crate::patch::{self, Edit};
use crate::prompt::quoted;
use crate::store::NoteStore;
use crate::tools::{self, ToolDef};

pub const MAX_WRITES_PER_TURN: u32 = 20;
pub const MAX_NOTES_WITHOUT_CONFIRM: usize = 5;
const MAX_TRANSCRIPT_CHARS: usize = 150_000;
const MAX_NOTE_CHARS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// A chat turn (or the MCP bridge): tiers apply as configured.
    Chat,
    /// A scheduled task run: every write becomes a proposal linked to the run.
    ProposeOnly { task_run_id: Id },
}

/// Where to stream tool rows and proposal cards for a chat turn.
#[derive(Debug, Clone, Copy)]
pub struct ChatEmit {
    pub thread_id: Id,
    pub message_id: Id,
}

/// Per-turn state for tool calls.
pub struct CallContext {
    pub thread_id: Option<Id>,
    pub turn_id: Id,
    pub mode: RunMode,
    pub auto_apply_edits: bool,
    pub emit: Option<ChatEmit>,
    writes: AtomicU32,
    touched: Mutex<HashSet<Id>>,
}

impl CallContext {
    pub fn chat(thread_id: Option<Id>, turn_id: Id, auto_apply_edits: bool, emit: Option<ChatEmit>) -> Self {
        Self { thread_id, turn_id, mode: RunMode::Chat, auto_apply_edits, emit, writes: AtomicU32::new(0), touched: Mutex::default() }
    }

    pub fn propose_only(task_run_id: Id) -> Self {
        Self {
            thread_id: None,
            turn_id: task_run_id,
            mode: RunMode::ProposeOnly { task_run_id },
            auto_apply_edits: false,
            emit: None,
            writes: AtomicU32::new(0),
            touched: Mutex::default(),
        }
    }

    pub fn writes_used(&self) -> u32 {
        self.writes.load(Ordering::SeqCst)
    }
}

/// What a call produced.
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub action: AiAction,
    pub proposal: Option<Proposal>,
    /// The tool result for the model.
    pub content: String,
    pub is_error: bool,
}

enum Exec {
    Read { content: String, summary: String },
    Write { op: WriteOp, reason: String },
}

#[derive(Clone)]
pub struct ToolRunner {
    pub(crate) db: Db,
    pub(crate) notes: Arc<dyn NoteStore>,
    pub(crate) events: Events,
}

fn parse<T: DeserializeOwned>(input: &Value) -> Result<T, String> {
    let input = if input.is_null() { Value::Object(Default::default()) } else { input.clone() };
    serde_json::from_value(input).map_err(|e| format!("The arguments do not fit this tool: {e}. Check the schema and try again."))
}

fn parse_id(field: &str, s: &str) -> Result<Id, String> {
    Id::parse_str(s.trim()).map_err(|_| format!("{field} \"{s}\" is not a valid id. Use an id from a list or search result."))
}

/// A store error as a tool error for the model.
fn e<T>(r: AppResult<T>) -> Result<T, String> {
    r.map_err(|e| e.message)
}

fn day(t: margin_domain::Timestamp) -> String {
    t.format("%Y-%m-%d").to_string()
}

fn strip_marks(s: &str) -> String {
    s.replace("<mark>", "").replace("</mark>", "").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&").replace("&quot;", "\"")
}

fn clip(s: &str, max: usize) -> (String, bool) {
    if s.chars().count() <= max { (s.to_string(), false) } else { (s.chars().take(max).collect(), true) }
}

fn mmss(ms: u32) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl ToolRunner {
    pub fn new(db: Db, notes: Arc<dyn NoteStore>, events: Events) -> Self {
        Self { db, notes, events }
    }

    fn emit_tool(&self, ctx: &CallContext, action: &AiAction) {
        if let Some(e) = ctx.emit {
            self.events.emit(AppEvent::Assistant {
                event: AssistantEvent::Tool { thread_id: e.thread_id, turn_id: ctx.turn_id, message_id: e.message_id, action: action.clone() },
            });
        }
    }

    /// Run one tool call. Never panics on bad input; failures come back as an error result for the model.
    pub async fn call(&self, ctx: &CallContext, name: &str, input: &Value) -> ToolOutcome {
        let def = tools::find(name);
        let mut action = AiAction {
            id: new_id(),
            thread_id: ctx.thread_id,
            turn_id: ctx.turn_id,
            tool: name.to_string(),
            tier: def.map(|d| d.tier).unwrap_or(ToolTier::Read),
            args_json: input.to_string(),
            summary: def.map(|d| d.running.to_string()).unwrap_or_else(|| name.to_string()),
            status: AiActionStatus::Running,
            error: None,
            can_undo: false,
            proposal_id: None,
            created_at: now(),
        };
        if let Err(e) = self.db.insert_ai_action(&action, None).await {
            tracing::error!(error = %e, "could not write the audit row");
        }
        self.emit_tool(ctx, &action);

        let mut proposal = None;
        let mut undo_json = None;
        let mut scopes = vec![DataScope::AiActions];
        let mut note_scope = None;
        let result: Result<String, String> = async {
            let def = def.ok_or_else(|| format!("There is no tool called {name}."))?;
            if matches!(ctx.mode, RunMode::ProposeOnly { .. }) && !def.in_tasks {
                return Err(format!("{name} is not available in scheduled tasks."));
            }
            if def.tier > ToolTier::Read {
                let used = ctx.writes.fetch_add(1, Ordering::SeqCst);
                if used >= MAX_WRITES_PER_TURN {
                    return Err(format!(
                        "Margin can make at most {MAX_WRITES_PER_TURN} changes in one turn. Stop here and tell the user what is left to do."
                    ));
                }
            }
            match self.execute(def, input).await? {
                Exec::Read { content, summary } => {
                    action.summary = summary;
                    action.status = AiActionStatus::Applied;
                    Ok(content)
                }
                Exec::Write { op, reason } => self.dispose(ctx, &mut action, op, reason, &mut proposal, &mut undo_json, &mut scopes, &mut note_scope).await,
            }
        }
        .await;

        let (content, is_error) = match result {
            Ok(content) => (content, false),
            Err(message) => {
                action.status = AiActionStatus::Failed;
                action.summary = format!("Could not run {}", name.replace('_', " "));
                action.error = Some(message.clone());
                (message, true)
            }
        };
        action.can_undo = action.status == AiActionStatus::Applied && undo_json.is_some();
        if let Err(e) = self.db.update_ai_action(&action, undo_json.as_deref()).await {
            tracing::error!(error = %e, "could not update the audit row");
        }
        self.emit_tool(ctx, &action);
        if let (Some(p), Some(e)) = (&proposal, ctx.emit) {
            self.events.emit(AppEvent::Assistant {
                event: AssistantEvent::Proposal { thread_id: e.thread_id, turn_id: ctx.turn_id, message_id: e.message_id, proposal: p.clone() },
            });
        }
        scopes.dedup();
        self.events.emit(AppEvent::DataChanged { scopes, note_id: note_scope });
        ToolOutcome { action, proposal, content, is_error }
    }

    /// Decide how a write is handled, by tier and mode, then apply it or turn it into a proposal.
    #[allow(clippy::too_many_arguments)]
    async fn dispose(
        &self,
        ctx: &CallContext,
        action: &mut AiAction,
        op: WriteOp,
        reason: String,
        proposal_out: &mut Option<Proposal>,
        undo_out: &mut Option<String>,
        scopes: &mut Vec<DataScope>,
        note_scope: &mut Option<Id>,
    ) -> Result<String, String> {
        let mut tier = op.base_tier();
        let touched = op.touched_notes();
        // Decide under one lock so parallel calls cannot each slip under the limit.
        let propose = {
            let mut seen = ctx.touched.lock().unwrap();
            // More than 5 distinct existing notes in one turn needs confirmation.
            if !touched.is_empty() && tier < ToolTier::Destructive {
                let union: HashSet<Id> = seen.iter().copied().chain(touched.iter().copied()).collect();
                if union.len() > MAX_NOTES_WITHOUT_CONFIRM {
                    tier = ToolTier::Destructive;
                }
            }
            let propose = match (ctx.mode, tier) {
                (RunMode::ProposeOnly { .. }, _) => true,
                (_, ToolTier::Destructive) => true,
                (_, ToolTier::Edit) => !ctx.auto_apply_edits,
                _ => false,
            };
            if !propose {
                seen.extend(touched.iter().copied());
            }
            propose
        };
        action.tier = tier;

        if !propose {
            let undo = ops::apply(self.notes.as_ref(), &self.db, &op, action.id).await.map_err(|e| e.message)?;
            *undo_out = undo.as_ref().and_then(|u| serde_json::to_string(u).ok());
            action.status = AiActionStatus::Applied;
            action.summary = op.applied_summary();
            let (s, n) = op.scopes();
            scopes.extend(s);
            *note_scope = n;
            return Ok(format!("Done: {}.", action.summary));
        }

        let kind = op.to_proposal_kind().ok_or_else(|| "This change cannot be suggested for review here.".to_string())?;
        let fingerprint = ops::fingerprint(&kind);
        let task_run_id = match ctx.mode {
            RunMode::ProposeOnly { task_run_id } => {
                let earlier = self.db.proposal_fingerprint_status(&fingerprint).await.map_err(|e| e.to_string())?;
                if matches!(earlier, Some(ProposalStatus::Dismissed | ProposalStatus::Pending)) {
                    action.status = AiActionStatus::Rejected;
                    action.summary = "Skipped a suggestion made before".into();
                    return Ok(
                        "Skipped: this suggestion was already made before and is pending or was dismissed by the user. Do not suggest it again."
                            .into(),
                    );
                }
                Some(task_run_id)
            }
            RunMode::Chat => None,
        };
        let proposal = Proposal {
            id: new_id(),
            source: if task_run_id.is_some() { ProposalSource::Task } else { ProposalSource::Chat },
            thread_id: ctx.thread_id,
            task_run_id,
            tier,
            kind,
            reason,
            status: ProposalStatus::Pending,
            created_at: now(),
            resolved_at: None,
        };
        self.db.insert_proposal(&proposal, Some(&fingerprint)).await.map_err(|e| e.to_string())?;
        action.status = AiActionStatus::Proposed;
        action.proposal_id = Some(proposal.id);
        action.summary = op.proposed_summary();
        scopes.push(DataScope::Proposals);
        let waiting = if tier == ToolTier::Destructive { "a confirmation card" } else { "a diff card" };
        *proposal_out = Some(proposal);
        Ok(format!(
            "Not applied yet: {}. The user sees {waiting} and decides. Do not claim it is done.",
            action.summary
        ))
    }

    async fn note_ref(&self, field: &str, raw: &str, live: bool) -> Result<(NoteRef, margin_domain::Note), String> {
        let id = parse_id(field, raw)?;
        let note = self.notes.get_note(id).await.map_err(|_| format!("No note has the id {id}. Search for it first."))?;
        if live && note.deleted_at.is_some() {
            return Err(format!("\"{}\" is in Trash.", note.title));
        }
        Ok((NoteRef { id, title: note.title.clone() }, note))
    }

    async fn category_ref(&self, raw: &str) -> Result<(Id, String), String> {
        let id = parse_id("category_id", raw)?;
        let c = self.notes.get_category(id).await.map_err(|_| format!("No category has the id {id}. Call list_categories."))?;
        Ok((id, c.name))
    }

    async fn execute(&self, def: &ToolDef, input: &Value) -> Result<Exec, String> {
        match def.name {
            // ── Read ──
            "search_notes" => {
                #[derive(Deserialize)]
                struct A {
                    query: Option<String>,
                    category_id: Option<String>,
                    limit: Option<u32>,
                }
                let a: A = parse(input)?;
                let limit = a.limit.unwrap_or(20).clamp(1, 50);
                let category = a.category_id.as_deref().map(|c| parse_id("category_id", c)).transpose()?;
                let categories = e(self.notes.list_categories().await)?;
                let cat_name = |id: Option<Id>| id.and_then(|id| categories.iter().find(|c| c.id == id).map(|c| c.name.clone()));
                let query = a.query.unwrap_or_default();
                let mut lines = Vec::new();
                if query.trim().is_empty() {
                    let notes = e(self.notes.list_notes().await)?;
                    for n in notes.iter().filter(|n| category.is_none() || n.category_id == category).take(limit as usize) {
                        lines.push(format!(
                            "- \"{}\" (id: {}, {}, updated {}{}) {}",
                            n.title,
                            n.id,
                            if n.kind == margin_domain::NoteKind::Meeting { "meeting" } else { "note" },
                            day(n.updated_at),
                            cat_name(n.category_id).map(|c| format!(", category: {c}")).unwrap_or_default(),
                            n.snippet
                        ));
                    }
                    let summary = format!("Listed {}", if lines.len() == 1 { "1 note".into() } else { format!("{} notes", lines.len()) });
                    let content = if lines.is_empty() { "No notes.".into() } else { quoted("search_results", &[], &lines.join("\n")) };
                    return Ok(Exec::Read { content, summary });
                }
                let hits = e(self.notes.search_notes(&query, limit).await)?;
                for h in hits.iter().filter(|h| category.is_none() || h.note.category_id == category) {
                    let at = h.at_ms.map(|ms| format!(" at {}", mmss(ms))).unwrap_or_default();
                    lines.push(format!(
                        "- \"{}\" (id: {}, updated {}{}) match in {:?}{}: {}",
                        h.note.title,
                        h.note.id,
                        day(h.note.updated_at),
                        cat_name(h.note.category_id).map(|c| format!(", category: {c}")).unwrap_or_default(),
                        h.source,
                        at,
                        strip_marks(&h.excerpt)
                    ));
                }
                let n = lines.len();
                let summary = format!("Searched notes for “{}” ({} found)", query.trim(), n);
                let content = if n == 0 { format!("No notes match \"{query}\".") } else { quoted("search_results", &[("query", &query)], &lines.join("\n")) };
                Ok(Exec::Read { content, summary })
            }
            "get_note" => {
                #[derive(Deserialize)]
                struct A {
                    note_id: String,
                }
                let a: A = parse(input)?;
                let (r, n) = self.note_ref("note_id", &a.note_id, false).await?;
                let category = match n.category_id {
                    Some(c) => self.notes.get_category(c).await.ok().map(|c| c.name),
                    None => None,
                };
                let (body, cut) = clip(&n.body_md, MAX_NOTE_CHARS);
                let mut meta = format!(
                    "Note \"{}\" (id: {}), {}, created {}, updated {}",
                    n.title,
                    n.id,
                    if n.kind == margin_domain::NoteKind::Meeting { "meeting note" } else { "note" },
                    day(n.created_at),
                    day(n.updated_at)
                );
                if let Some(c) = category {
                    meta.push_str(&format!(", category: {c}"));
                }
                if n.deleted_at.is_some() {
                    meta.push_str(", in Trash");
                }
                if cut {
                    meta.push_str(". The body is long and was cut short here");
                }
                let content = format!("{meta}.\n{}", quoted("note", &[("id", &n.id.to_string()), ("title", &n.title)], &body));
                Ok(Exec::Read { content, summary: format!("Read {}", NoteRefExt(&r).title()) })
            }
            "get_transcript" => {
                #[derive(Deserialize)]
                struct A {
                    note_id: String,
                }
                let a: A = parse(input)?;
                let (r, n) = self.note_ref("note_id", &a.note_id, false).await?;
                let text = e(self.notes.transcript_text(n.id).await)?;
                let summary = format!("Read the transcript of {}", NoteRefExt(&r).title());
                if text.trim().is_empty() {
                    return Ok(Exec::Read { content: format!("\"{}\" has no transcript.", n.title), summary });
                }
                let speakers = e(self.notes.list_speakers(n.id).await)?;
                let people = e(self.notes.list_people().await)?;
                let speaker_lines: Vec<String> = speakers
                    .iter()
                    .map(|s| {
                        let linked = s.person_id.and_then(|p| people.iter().find(|x| x.id == p)).map(|p| format!(", linked to {}", p.name)).unwrap_or_default();
                        format!("{} (speaker_id: {}{}{})", s.label, s.id, if s.is_self { ", the user" } else { "" }, linked)
                    })
                    .collect();
                let (body, cut) = clip(&text, MAX_TRANSCRIPT_CHARS);
                let mut content = format!("Transcript of \"{}\" (id: {}). Speakers: {}.", n.title, n.id, if speaker_lines.is_empty() { "unknown".into() } else { speaker_lines.join("; ") });
                if cut {
                    content.push_str(" It is long and was cut short here.");
                }
                content.push('\n');
                content.push_str(&quoted("transcript", &[("note_id", &n.id.to_string()), ("title", &n.title)], &body));
                Ok(Exec::Read { content, summary })
            }
            "list_categories" => {
                let cats = e(self.notes.list_categories().await)?;
                let notes = e(self.notes.list_notes().await)?;
                let uncategorised = notes.iter().filter(|n| n.category_id.is_none()).count();
                let mut lines: Vec<String> = cats
                    .iter()
                    .map(|c| format!("- {} (id: {}, {} notes)", c.name, c.id, notes.iter().filter(|n| n.category_id == Some(c.id)).count()))
                    .collect();
                lines.push(format!("- Uncategorised: {uncategorised} notes"));
                Ok(Exec::Read { content: lines.join("\n"), summary: format!("Listed {} categories", cats.len()) })
            }
            "list_people" => {
                let people = e(self.notes.list_people().await)?;
                let lines: Vec<String> = people.iter().map(|p| format!("- {} (id: {}, in {} meetings)", p.name, p.id, p.meeting_count)).collect();
                let content = if lines.is_empty() { "No Saved people yet.".into() } else { lines.join("\n") };
                Ok(Exec::Read { content, summary: format!("Listed {} saved people", people.len()) })
            }
            "list_tasks" => {
                let tasks = e(self.db.list_tasks().await.map_err(margin_domain::AppError::from))?;
                let lines: Vec<String> = tasks
                    .iter()
                    .map(|t| {
                        format!(
                            "- {} (id: {}, {}, {}, next run {})",
                            t.name,
                            t.id,
                            if t.enabled { "on" } else { "off" },
                            serde_json::to_string(&t.schedule).unwrap_or_default(),
                            t.next_run_at.map(|d| d.to_rfc3339()).unwrap_or_else(|| "not scheduled".into())
                        )
                    })
                    .collect();
                let content = if lines.is_empty() { "No scheduled tasks.".into() } else { lines.join("\n") };
                Ok(Exec::Read { content, summary: format!("Listed {} scheduled tasks", tasks.len()) })
            }
            "get_note_history" => {
                #[derive(Deserialize)]
                struct A {
                    note_id: String,
                }
                let a: A = parse(input)?;
                let (r, n) = self.note_ref("note_id", &a.note_id, false).await?;
                let versions = e(self.notes.list_note_versions(n.id).await)?;
                let lines: Vec<String> = versions
                    .iter()
                    .take(30)
                    .map(|v| {
                        let first: String = v.body_md.lines().find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(120).collect();
                        format!(
                            "- {} by {} — \"{}\", {} characters: {}",
                            v.created_at.format("%Y-%m-%d %H:%M"),
                            if v.author == margin_domain::Author::Ai { "Margin" } else { "the user" },
                            v.title,
                            v.body_md.chars().count(),
                            first
                        )
                    })
                    .collect();
                let content = if lines.is_empty() {
                    format!("\"{}\" has no saved versions.", n.title)
                } else {
                    quoted("history", &[("note_id", &n.id.to_string())], &lines.join("\n"))
                };
                Ok(Exec::Read { content, summary: format!("Read the history of {}", NoteRefExt(&r).title()) })
            }

            // ── Additive ──
            "create_note" => {
                #[derive(Deserialize)]
                struct A {
                    title: String,
                    #[serde(default)]
                    body_md: String,
                    category_id: Option<String>,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                if a.title.trim().is_empty() {
                    return Err("title is empty.".into());
                }
                let category = match a.category_id.as_deref() {
                    Some(c) if !c.trim().is_empty() => Some(self.category_ref(c).await?),
                    _ => None,
                };
                Ok(Exec::Write { op: WriteOp::CreateNote { title: a.title.trim().into(), body_md: a.body_md, category }, reason: a.reason })
            }
            "create_category" => {
                #[derive(Deserialize)]
                struct A {
                    name: String,
                }
                let a: A = parse(input)?;
                let name = a.name.trim().to_string();
                if name.is_empty() {
                    return Err("name is empty.".into());
                }
                if let Some(c) = e(self.notes.list_categories().await)?.into_iter().find(|c| c.name.eq_ignore_ascii_case(&name)) {
                    return Err(format!("A category called {} already exists (id: {}).", c.name, c.id));
                }
                Ok(Exec::Write { op: WriteOp::CreateCategory { name }, reason: String::new() })
            }
            "move_note" => {
                #[derive(Deserialize)]
                struct A {
                    note_ids: Vec<String>,
                    category_id: Option<String>,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                if a.note_ids.is_empty() {
                    return Err("note_ids is empty.".into());
                }
                let category = match a.category_id.as_deref() {
                    Some(c) if !c.trim().is_empty() => Some(self.category_ref(c).await?),
                    _ => None,
                };
                let mut notes = Vec::new();
                for raw in dedup(a.note_ids) {
                    notes.push(self.note_ref("note_ids", &raw, true).await?.0);
                }
                Ok(Exec::Write { op: WriteOp::MoveNotes { notes, category }, reason: a.reason })
            }
            "rename_category" => {
                #[derive(Deserialize)]
                struct A {
                    category_id: String,
                    name: String,
                }
                let a: A = parse(input)?;
                let (category_id, old_name) = self.category_ref(&a.category_id).await?;
                if a.name.trim().is_empty() {
                    return Err("name is empty.".into());
                }
                Ok(Exec::Write { op: WriteOp::RenameCategory { category_id, old_name, new_name: a.name.trim().into() }, reason: String::new() })
            }
            "label_speaker" => {
                #[derive(Deserialize)]
                struct A {
                    note_id: String,
                    speaker_id: String,
                    label: Option<String>,
                    person_id: Option<String>,
                }
                let a: A = parse(input)?;
                let (r, _) = self.note_ref("note_id", &a.note_id, false).await?;
                let speaker_id = parse_id("speaker_id", &a.speaker_id)?;
                let speaker = e(self.notes.list_speakers(r.id).await)?
                    .into_iter()
                    .find(|s| s.id == speaker_id)
                    .ok_or_else(|| format!("No speaker {speaker_id} in \"{}\". Call get_transcript for the speaker ids.", r.title))?;
                let person = match a.person_id.as_deref().filter(|p| !p.trim().is_empty()) {
                    Some(p) => {
                        let pid = parse_id("person_id", p)?;
                        let person = self.notes.get_person(pid).await.map_err(|_| format!("No Saved person has the id {pid}."))?;
                        Some((pid, person.name))
                    }
                    None => None,
                };
                let label = a.label.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).or_else(|| person.as_ref().map(|p| p.1.clone()));
                if label.is_none() && person.is_none() {
                    return Err("Give a label, a person_id, or both.".into());
                }
                Ok(Exec::Write {
                    op: WriteOp::LabelSpeaker { note_id: r.id, speaker_id, old_label: speaker.label, label, person },
                    reason: String::new(),
                })
            }
            "create_task" => {
                #[derive(Deserialize)]
                struct A {
                    name: String,
                    prompt: String,
                    schedule: TaskSchedule,
                    scope: Option<TaskScope>,
                }
                let a: A = parse(input)?;
                validate_schedule(&a.schedule)?;
                if a.name.trim().is_empty() || a.prompt.trim().is_empty() {
                    return Err("name and prompt are required.".into());
                }
                if let Some(TaskScope::Category { category_id }) = &a.scope {
                    self.category_ref(&category_id.to_string()).await?;
                }
                let draft = TaskDraft {
                    name: a.name.trim().into(),
                    prompt: a.prompt.trim().into(),
                    scope: a.scope.unwrap_or(TaskScope::AllNotes),
                    schedule: a.schedule,
                    enabled: true,
                };
                Ok(Exec::Write { op: WriteOp::CreateTask { draft }, reason: String::new() })
            }

            // ── Edit ──
            "edit_note" => {
                #[derive(Deserialize)]
                struct A {
                    note_id: String,
                    edits: Vec<Edit>,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                let (r, n) = self.note_ref("note_id", &a.note_id, true).await?;
                let after = patch::apply_edits(&n.body_md, &a.edits)?;
                Ok(Exec::Write { op: WriteOp::EditNote { note: r, before_md: n.body_md, after_md: after }, reason: a.reason })
            }
            "rename_note" => {
                #[derive(Deserialize)]
                struct A {
                    note_id: String,
                    title: String,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                let (r, n) = self.note_ref("note_id", &a.note_id, true).await?;
                let title = a.title.trim().to_string();
                if title.is_empty() || title == n.title {
                    return Err("Give a new, non-empty title.".into());
                }
                Ok(Exec::Write { op: WriteOp::RenameNote { note: r, new_title: title }, reason: a.reason })
            }

            // ── Destructive ──
            "trash_note" => {
                #[derive(Deserialize)]
                struct A {
                    note_ids: Vec<String>,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                if a.note_ids.is_empty() {
                    return Err("note_ids is empty.".into());
                }
                let mut notes = Vec::new();
                for raw in dedup(a.note_ids) {
                    notes.push(self.note_ref("note_ids", &raw, true).await?.0);
                }
                Ok(Exec::Write { op: WriteOp::TrashNotes { notes }, reason: a.reason })
            }
            "merge_notes" => {
                #[derive(Deserialize)]
                struct A {
                    note_ids: Vec<String>,
                    title: String,
                    body_md: String,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                let ids = dedup(a.note_ids);
                if ids.len() < 2 {
                    return Err("merge_notes needs at least 2 different notes.".into());
                }
                if a.title.trim().is_empty() || a.body_md.trim().is_empty() {
                    return Err("title and body_md are required for the merged note.".into());
                }
                let mut sources = Vec::new();
                for raw in ids {
                    sources.push(self.note_ref("note_ids", &raw, true).await?.0);
                }
                Ok(Exec::Write { op: WriteOp::MergeNotes { sources, merged_title: a.title.trim().into(), merged_md: a.body_md }, reason: a.reason })
            }
            "delete_category" => {
                #[derive(Deserialize)]
                struct A {
                    category_id: String,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                let (category_id, name) = self.category_ref(&a.category_id).await?;
                let note_count = e(self.notes.list_notes().await)?.iter().filter(|n| n.category_id == Some(category_id)).count();
                Ok(Exec::Write {
                    op: WriteOp::DeleteCategory { category_id, name, note_count: u32::try_from(note_count).unwrap_or(u32::MAX) },
                    reason: a.reason,
                })
            }
            "delete_person" => {
                #[derive(Deserialize)]
                struct A {
                    person_id: String,
                    #[serde(default)]
                    reason: String,
                }
                let a: A = parse(input)?;
                let person_id = parse_id("person_id", &a.person_id)?;
                let p = self.notes.get_person(person_id).await.map_err(|_| format!("No Saved person has the id {person_id}."))?;
                Ok(Exec::Write { op: WriteOp::DeletePerson { person_id, name: p.name }, reason: a.reason })
            }
            other => Err(format!("There is no tool called {other}.")),
        }
    }
}

struct NoteRefExt<'a>(&'a NoteRef);
impl NoteRefExt<'_> {
    fn title(&self) -> String {
        if self.0.title.trim().is_empty() { "Untitled".into() } else { self.0.title.clone() }
    }
}

fn dedup(ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.into_iter().filter(|i| seen.insert(i.trim().to_lowercase())).collect()
}

fn validate_schedule(s: &TaskSchedule) -> Result<(), String> {
    let (hour, minute) = match s {
        TaskSchedule::EveryNDays { days, hour, minute } => {
            if *days == 0 || *days > 365 {
                return Err("days must be between 1 and 365.".into());
            }
            (*hour, *minute)
        }
        TaskSchedule::Weekly { weekday, hour, minute } => {
            if *weekday > 6 {
                return Err("weekday must be 0 (Monday) to 6 (Sunday).".into());
            }
            (*hour, *minute)
        }
        TaskSchedule::Daily { hour, minute } => (*hour, *minute),
    };
    if hour > 23 || minute > 59 {
        return Err("hour must be 0–23 and minute 0–59.".into());
    }
    Ok(())
}
