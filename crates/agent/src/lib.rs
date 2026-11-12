//! Margin's assistant: the only write path for AI.
//!
//! - [`tools`]: the registry (names, tiers, schemas); [`runner`]: runs calls with the tier rules
//!   enforced in code, writes the audit log, makes proposals; [`ops`]: apply and undo.
//! - [`Agent`]: threads, chat turns (streaming, parallel tool calls, cancellation, history
//!   trimming, titles), proposals (apply / dismiss), undo (one action or a whole turn), prompt chips,
//!   scheduled-task runs in propose-only mode, and the [`MeetingSummarizer`].
//! - [`store::NoteStore`]: the notes/transcripts seam, with a real and an in-memory implementation.
//!
//! Crates never depend on Tauri: everything reaches the UI through `margin_domain::Events`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use margin_domain::{
    AiAction, AiActionStatus, AiSettings, AppError, AppEvent, AppResult, DataScope, ErrorCode, Events, Id, Message,
    MeetingSummarizer, PromptChip, Proposal, ProposalStatus, ProviderKind, Thread, new_id, now,
};
use margin_llm::{CancellationToken, Provider};
use margin_store::Db;

mod bridge;
mod chips;
pub mod ops;
pub mod patch;
pub mod prompt;
pub mod runner;
pub mod store;
mod summarizer;
mod task;
pub mod tools;
mod turn;
mod week;

pub use runner::{CallContext, ChatEmit, RunMode, ToolOutcome, ToolRunner};
pub use store::{DbNoteStore, FakeNotes, NoteStore};
pub use summarizer::Summarizer;
pub use task::{TaskOutcome, scope_notes};

/// Where the agent gets its settings and model providers. The app reads keys from the keychain;
/// tests hand in a mock.
#[async_trait::async_trait]
pub trait ProviderSource: Send + Sync {
    /// Current AI settings (provider, models, auto-apply).
    async fn ai_settings(&self) -> AiSettings;
    /// A provider for the given settings, or an `Unavailable` error saying what to set up.
    fn provider(&self, settings: &AiSettings) -> AppResult<Arc<dyn Provider>>;
    /// How to launch Margin's MCP server for the Claude Code bridge, when this build supports it.
    fn bridge(&self) -> Option<BridgeConfig> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// `claude` binary; `None` searches PATH.
    pub binary: Option<PathBuf>,
    /// Margin's own executable and the arguments that start its MCP server.
    pub mcp_command: PathBuf,
    pub mcp_args: Vec<String>,
    /// An empty working folder for the CLI.
    pub cwd: PathBuf,
}

/// A [`ProviderSource`] with fixed settings and one provider. For tests and tools.
pub struct StaticProviders {
    pub settings: Mutex<AiSettings>,
    pub provider: Option<Arc<dyn Provider>>,
}

impl StaticProviders {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self { settings: Mutex::new(AiSettings::default()), provider: Some(provider) }
    }

    pub fn none() -> Self {
        Self { settings: Mutex::new(AiSettings::default()), provider: None }
    }
}

#[async_trait::async_trait]
impl ProviderSource for StaticProviders {
    async fn ai_settings(&self) -> AiSettings {
        self.settings.lock().unwrap().clone()
    }
    fn provider(&self, _settings: &AiSettings) -> AppResult<Arc<dyn Provider>> {
        self.provider.clone().ok_or_else(|| AppError::unavailable("Add your Anthropic API key in Settings › Margin to use Margin."))
    }
}

/// Limits for one turn.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Model calls per turn (each may run several tools).
    pub max_steps: u32,
    pub max_tokens: u32,
    /// Rough size of history (in characters) before old turns are dropped.
    pub history_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        // ~100k tokens of history: well inside a 1M window, cheap with caching, and safe for
        // smaller local models after trimming.
        Self { max_steps: 12, max_tokens: 32_000, history_chars: 400_000 }
    }
}

pub(crate) struct Inner {
    pub db: Db,
    pub notes: Arc<dyn NoteStore>,
    pub events: Events,
    pub providers: Arc<dyn ProviderSource>,
    pub runner: ToolRunner,
    pub limits: Limits,
    /// Running turns: turn id → cancel token.
    pub turns: Mutex<HashMap<Id, CancellationToken>>,
    /// Threads with a running turn.
    pub busy_threads: Mutex<HashSet<Id>>,
    pub chips: Mutex<HashMap<String, Vec<PromptChip>>>,
    pub chip_jobs: Mutex<HashSet<String>>,
}

/// The assistant service. Cheap to clone.
#[derive(Clone)]
pub struct Agent {
    pub(crate) inner: Arc<Inner>,
}

impl Agent {
    pub fn new(db: Db, notes: Arc<dyn NoteStore>, events: Events, providers: Arc<dyn ProviderSource>) -> Self {
        Self::with_limits(db, notes, events, providers, Limits::default())
    }

    pub fn with_limits(db: Db, notes: Arc<dyn NoteStore>, events: Events, providers: Arc<dyn ProviderSource>, limits: Limits) -> Self {
        let runner = ToolRunner::new(db.clone(), notes.clone(), events.clone());
        Self {
            inner: Arc::new(Inner {
                db,
                notes,
                events,
                providers,
                runner,
                limits,
                turns: Mutex::default(),
                busy_threads: Mutex::default(),
                chips: Mutex::default(),
                chip_jobs: Mutex::default(),
            }),
        }
    }

    pub fn db(&self) -> &Db {
        &self.inner.db
    }

    pub fn runner(&self) -> &ToolRunner {
        &self.inner.runner
    }

    pub fn summarizer(&self) -> Arc<dyn MeetingSummarizer> {
        Arc::new(Summarizer::new(self.clone()))
    }

    pub(crate) fn emit_changed(&self, scopes: Vec<DataScope>, note_id: Option<Id>) {
        self.inner.events.emit(AppEvent::DataChanged { scopes, note_id });
    }

    /// The provider for the current settings, or an error saying what to set up.
    pub(crate) async fn provider(&self) -> AppResult<(AiSettings, Arc<dyn Provider>)> {
        let settings = self.inner.providers.ai_settings().await;
        let provider = self.inner.providers.provider(&settings)?;
        Ok((settings, provider))
    }

    // ── Threads ──────────────────────────────────────────────────────────

    pub async fn list_threads(&self) -> AppResult<Vec<Thread>> {
        Ok(self.inner.db.list_threads().await?)
    }

    pub async fn create_thread(&self) -> AppResult<Thread> {
        let t = self.inner.db.create_thread("").await?;
        self.emit_changed(vec![DataScope::Threads], None);
        Ok(t)
    }

    pub async fn delete_thread(&self, thread_id: Id) -> AppResult<()> {
        if self.inner.busy_threads.lock().unwrap().contains(&thread_id) {
            return Err(AppError::new(ErrorCode::Conflict, "Margin is still answering in this conversation. Stop it first, then delete it."));
        }
        self.inner.db.delete_thread(thread_id).await?;
        self.emit_changed(vec![DataScope::Threads], None);
        Ok(())
    }

    /// Delete conversations with no new message for `conversation_retention_days` (0 keeps them
    /// for ever). A conversation with a running turn is left alone. Returns how many went.
    pub async fn delete_old_threads(&self) -> AppResult<u32> {
        let days = self.inner.providers.ai_settings().await.conversation_retention_days;
        if days == 0 {
            return Ok(0);
        }
        let cutoff = margin_domain::now() - chrono::Duration::days(i64::from(days));
        let busy: Vec<Id> = self.inner.busy_threads.lock().unwrap().iter().copied().collect();
        let deleted = self.inner.db.delete_threads_idle_since(cutoff, &busy).await?;
        if !deleted.is_empty() {
            self.emit_changed(vec![DataScope::Threads], None);
        }
        Ok(deleted.len() as u32)
    }

    pub async fn get_thread_messages(&self, thread_id: Id) -> AppResult<Vec<Message>> {
        self.inner.db.get_thread(thread_id).await?;
        Ok(self.inner.db.list_messages(thread_id).await?.into_iter().map(|(m, _)| m).collect())
    }

    /// Stop a running turn. Unknown or finished turns are a no-op.
    pub fn cancel_turn(&self, turn_id: Id) {
        if let Some(token) = self.inner.turns.lock().unwrap().get(&turn_id) {
            token.cancel();
        }
    }

    /// Cancel every running turn (app shutdown).
    pub fn cancel_all(&self) {
        for token in self.inner.turns.lock().unwrap().values() {
            token.cancel();
        }
    }

    pub fn is_turn_running(&self, turn_id: Id) -> bool {
        self.inner.turns.lock().unwrap().contains_key(&turn_id)
    }

    // ── Proposals ────────────────────────────────────────────────────────

    pub async fn list_proposals(&self, status: Option<ProposalStatus>) -> AppResult<Vec<Proposal>> {
        Ok(self.inner.db.list_proposals(status).await?)
    }

    pub async fn get_proposal(&self, proposal_id: Id) -> AppResult<Proposal> {
        Ok(self.inner.db.get_proposal(proposal_id).await?)
    }

    fn not_pending(p: &Proposal) -> AppError {
        let what = match p.status {
            ProposalStatus::Applied => "already applied",
            ProposalStatus::Dismissed => "already dismissed",
            ProposalStatus::Pending => "busy",
        };
        AppError::new(ErrorCode::Conflict, format!("This suggestion was {what}."))
    }

    /// Apply a pending proposal. `final_md` is the body after the user accepted only some hunks
    /// (edit proposals) or edited the preview (merge / new note). Safe to call concurrently: a
    /// proposal is claimed atomically, so "Apply all" cannot apply anything twice.
    pub async fn apply_proposal(&self, proposal_id: Id, final_md: Option<String>) -> AppResult<Proposal> {
        let db = &self.inner.db;
        let p = db.get_proposal(proposal_id).await?;
        if p.status != ProposalStatus::Pending || !db.transition_proposal(proposal_id, ProposalStatus::Pending, ProposalStatus::Applied).await? {
            return Err(Self::not_pending(&db.get_proposal(proposal_id).await?));
        }
        // The audit row made when the proposal was created; one is made now if it is missing.
        let (mut action, _) = match db.ai_action_for_proposal(proposal_id).await? {
            Some(found) => found,
            None => {
                let a = AiAction {
                    id: new_id(),
                    thread_id: p.thread_id,
                    turn_id: p.task_run_id.unwrap_or_else(new_id),
                    tool: ops::tool_for_kind(&p.kind).into(),
                    tier: p.tier,
                    args_json: "{}".into(),
                    summary: String::new(),
                    status: AiActionStatus::Proposed,
                    error: None,
                    can_undo: false,
                    proposal_id: Some(proposal_id),
                    created_at: now(),
                };
                db.insert_ai_action(&a, None).await?;
                (a, None)
            }
        };
        let op = ops::WriteOp::from_proposal_kind(&p.kind, final_md);
        match ops::apply(self.inner.notes.as_ref(), db, &op, action.id).await {
            Ok(undo) => {
                let undo_json = undo.as_ref().and_then(|u| serde_json::to_string(u).ok());
                action.status = AiActionStatus::Applied;
                action.summary = op.applied_summary();
                action.can_undo = undo_json.is_some();
                action.error = None;
                db.update_ai_action(&action, undo_json.as_deref()).await?;
                let (mut scopes, note_id) = op.scopes();
                scopes.extend([DataScope::Proposals, DataScope::AiActions, DataScope::Tasks]);
                self.emit_changed(scopes, note_id);
                Ok(db.get_proposal(proposal_id).await?)
            }
            Err(e) => {
                // Put it back so the user can retry or dismiss it.
                db.transition_proposal(proposal_id, ProposalStatus::Applied, ProposalStatus::Pending).await?;
                Err(e)
            }
        }
    }

    pub async fn dismiss_proposal(&self, proposal_id: Id) -> AppResult<Proposal> {
        let db = &self.inner.db;
        if !db.transition_proposal(proposal_id, ProposalStatus::Pending, ProposalStatus::Dismissed).await? {
            return Err(Self::not_pending(&db.get_proposal(proposal_id).await?));
        }
        if let Some((mut action, undo)) = db.ai_action_for_proposal(proposal_id).await? {
            action.status = AiActionStatus::Rejected;
            action.can_undo = false;
            db.update_ai_action(&action, undo.as_deref()).await?;
        }
        self.emit_changed(vec![DataScope::Proposals, DataScope::AiActions, DataScope::Tasks], None);
        Ok(db.get_proposal(proposal_id).await?)
    }

    // ── Audit log and undo ───────────────────────────────────────────────

    pub async fn list_ai_actions(&self, limit: u32, offset: u32) -> AppResult<Vec<AiAction>> {
        Ok(self.inner.db.list_ai_actions(limit.clamp(1, 500), offset).await?)
    }

    pub async fn undo_ai_action(&self, action_id: Id) -> AppResult<AiAction> {
        let action = self.undo_one(action_id).await?;
        self.emit_changed(vec![DataScope::AiActions], None);
        Ok(action)
    }

    async fn undo_one(&self, action_id: Id) -> AppResult<AiAction> {
        let db = &self.inner.db;
        let (mut action, undo_json) = db.get_ai_action(action_id).await?;
        let undo: ops::UndoOp = match (action.status, undo_json.as_deref()) {
            (AiActionStatus::Applied, Some(json)) => serde_json::from_str(json).map_err(|e| AppError::internal(format!("Undo data could not be read: {e}")))?,
            (AiActionStatus::Undone, _) => return Err(AppError::new(ErrorCode::Conflict, "That change was already undone.")),
            _ => return Err(AppError::invalid("That action can't be undone.")),
        };
        ops::undo(self.inner.notes.as_ref(), db, &undo, action.id).await?;
        action.status = AiActionStatus::Undone;
        action.can_undo = false;
        db.update_ai_action(&action, undo_json.as_deref()).await?;
        let (scopes, note_id) = undo.scopes();
        self.emit_changed(scopes, note_id);
        Ok(action)
    }

    /// Undo every applied action of a turn, newest first, as one step. Returns how many were undone.
    /// Stops at the first one that cannot be undone (the user changed the same thing since) and
    /// reports it, leaving the earlier ones in place.
    pub async fn undo_turn(&self, turn_id: Id) -> AppResult<u32> {
        let actions = self.inner.db.list_ai_actions_for_turn(turn_id).await?;
        let mut undone = 0u32;
        for (action, undo) in actions.into_iter().rev() {
            if action.status != AiActionStatus::Applied || undo.is_none() {
                continue;
            }
            match self.undo_one(action.id).await {
                Ok(_) => undone += 1,
                Err(e) => {
                    self.emit_changed(vec![DataScope::AiActions], None);
                    let message = if undone == 0 { e.message } else { format!("Undid {undone} changes, then stopped: {}", e.message) };
                    return Err(AppError::new(e.code, message));
                }
            }
        }
        self.emit_changed(vec![DataScope::AiActions], None);
        Ok(undone)
    }

    // ── Prompt chips ─────────────────────────────────────────────────────

    pub async fn get_prompt_chips(&self, view: &margin_domain::ViewContext) -> AppResult<Vec<PromptChip>> {
        chips::prompt_chips(self, view).await
    }

    /// One short line about the last seven days of notes, or `None` when there is nothing to say
    /// (few notes, no model). Costs one small model call: callers keep the answer for the day.
    pub async fn get_week_line(&self) -> AppResult<Option<String>> {
        week::week_line(self).await
    }

    /// Whether the chosen provider is the Claude Code bridge.
    pub(crate) fn is_bridge(settings: &AiSettings) -> bool {
        settings.provider == ProviderKind::ClaudeCode
    }
}

#[cfg(test)]
mod tests;
