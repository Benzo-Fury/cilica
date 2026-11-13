//! Scheduled task runs: the same loop as chat, in propose-only mode, with no thread.

use chrono::Duration;
use cilica_domain::{AppError, AppResult, Id, NoteKind, NoteSummary, Task, TaskScope, now};
use cilica_llm::{CancellationToken, ChatMessage, Effort};

use crate::prompt::{TASK_SYSTEM_PROMPT, quoted};
use crate::runner::CallContext;
use crate::turn::{LoopEnd, LoopRequest};
use crate::{Agent, tools};

/// What a task run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskOutcome {
    /// The model's short plain-text report.
    pub summary: String,
    /// Proposals made in this run.
    pub proposals: u32,
    pub cancelled: bool,
    pub error: Option<String>,
}

/// The notes a task's scope covers, newest first.
pub fn scope_notes(scope: &TaskScope, notes: Vec<NoteSummary>) -> Vec<NoteSummary> {
    let cutoff = match scope {
        TaskScope::LastDays { days } => Some(now() - Duration::days(i64::from(*days))),
        _ => None,
    };
    notes
        .into_iter()
        .filter(|n| match scope {
            TaskScope::AllNotes => true,
            TaskScope::Category { category_id } => n.category_id == Some(*category_id),
            TaskScope::LastDays { .. } => cutoff.is_some_and(|c| n.updated_at >= c),
        })
        .collect()
}

const MAX_INVENTORY: usize = 300;

impl Agent {
    /// Run a scheduled task once. Every write becomes a proposal linked to `task_run_id`.
    pub async fn run_task(&self, task: &Task, task_run_id: Id, cancel: CancellationToken) -> AppResult<TaskOutcome> {
        let (settings, provider) = self.provider().await?;
        if Agent::is_bridge(&settings) {
            return Err(AppError::unavailable(
                "Scheduled tasks need an API key or a local model; the Claude Code bridge only runs chats. Add one in Settings › Cilica.",
            ));
        }
        let all = self.inner.notes.list_notes().await?;
        let categories = self.inner.notes.list_categories().await?;
        let in_scope = scope_notes(&task.scope, all);
        let scope_text = match &task.scope {
            TaskScope::AllNotes => "all notes".to_string(),
            TaskScope::Category { category_id } => {
                format!("the category {}", categories.iter().find(|c| c.id == *category_id).map(|c| c.name.as_str()).unwrap_or("(deleted)"))
            }
            TaskScope::LastDays { days } => format!("notes updated in the last {days} days"),
        };
        let inventory: Vec<String> = in_scope
            .iter()
            .take(MAX_INVENTORY)
            .map(|n| {
                format!(
                    "- \"{}\" (id: {}, {}, updated {}) {}",
                    n.title,
                    n.id,
                    if n.kind == NoteKind::Meeting { "meeting" } else { "note" },
                    n.updated_at.format("%Y-%m-%d"),
                    n.snippet
                )
            })
            .collect();
        let mut prompt = format!(
            "Task: {}\nToday: {}\nScope: {} ({} notes).\n\n{}\n\n",
            task.name,
            now().format("%Y-%m-%d"),
            scope_text,
            in_scope.len(),
            task.prompt.trim()
        );
        if inventory.is_empty() {
            prompt.push_str("There are no notes in scope. Report that briefly.");
        } else {
            prompt.push_str(&quoted("search_results", &[("scope", &scope_text)], &inventory.join("\n")));
            if in_scope.len() > MAX_INVENTORY {
                prompt.push_str(&format!("\nOnly the {MAX_INVENTORY} most recent are listed; use search_notes for others."));
            }
        }

        let ctx = CallContext::propose_only(task_run_id);
        let request = LoopRequest {
            model: settings.model.clone(),
            system: TASK_SYSTEM_PROMPT.to_string(),
            tools: tools::specs(true),
            max_tokens: self.inner.limits.max_tokens,
            effort: Some(Effort::Medium),
        };
        let (state, end) = self.run_loop(provider.as_ref(), &request, vec![ChatMessage::user(prompt)], &ctx, &cancel, None).await;
        let proposals = u32::try_from(self.inner.db.list_proposals_for_run(task_run_id).await?.len()).unwrap_or(u32::MAX);
        let summary = state.last_text.trim().to_string();
        Ok(match end {
            LoopEnd::Completed => TaskOutcome { summary, proposals, cancelled: false, error: None },
            LoopEnd::Cancelled => TaskOutcome { summary, proposals, cancelled: true, error: None },
            LoopEnd::Error(e) => TaskOutcome { summary, proposals, cancelled: false, error: Some(e) },
        })
    }
}
