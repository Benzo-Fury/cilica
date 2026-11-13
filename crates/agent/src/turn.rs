//! Chat turns: the model ↔ tools loop, streaming, cancellation, history rebuild and trimming,
//! thread titles, and the turn's Undo toast.

use futures::StreamExt;
use cilica_domain::{
    AiActionStatus, AppError, AppEvent, AppResult, AssistantEvent, DataScope, ErrorCode, Id, Message, MessagePart,
    Role, Timestamp, ToastAction, ToastKind, ToastRequest, TurnEnd, ViewContext, new_id, now,
};
use cilica_llm::{
    CancellationToken, ChatMessage, ChatRequest, ChatRole, ContentBlock, Effort, LlmError, Provider, StopReason,
    StreamEvent,
};

use crate::prompt::{self, SYSTEM_PROMPT};
use crate::runner::{CallContext, ChatEmit};
use crate::{Agent, tools};

const MAX_MESSAGE_CHARS: usize = 20_000;
/// Tool results from older turns are cut to this size when history must be trimmed.
const OLD_TOOL_RESULT_CHARS: usize = 2_000;

/// Where to save the assistant message as it grows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AssistantMessage {
    pub id: Id,
    pub thread_id: Id,
    pub turn_id: Id,
    pub created_at: Timestamp,
}

#[derive(Debug, Default)]
pub(crate) struct LoopState {
    pub parts: Vec<MessagePart>,
    /// Provider content produced this turn (assistant messages and tool results).
    pub raw: Vec<ChatMessage>,
    /// Text of the last model step that wrote any.
    pub last_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LoopEnd {
    Completed,
    Cancelled,
    Error(String),
}

impl LoopState {
    fn push_text(&mut self, delta: &str) {
        if let Some(MessagePart::Text { text }) = self.parts.last_mut() {
            text.push_str(delta);
        } else {
            self.parts.push(MessagePart::Text { text: delta.to_string() });
        }
    }

    fn push_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.parts.iter().any(|p| matches!(p, MessagePart::Error { message: m } if *m == message)) {
            self.parts.push(MessagePart::Error { message });
        }
    }
}

/// The base request for a loop: everything except messages.
pub(crate) struct LoopRequest {
    pub model: String,
    pub system: String,
    pub tools: Vec<cilica_llm::ToolSpec>,
    pub max_tokens: u32,
    pub effort: Option<Effort>,
}

impl Agent {
    /// Start a turn. Returns its id at once; progress streams as `AppEvent::Assistant`.
    pub async fn send_message(&self, thread_id: Id, text: String, view: ViewContext) -> AppResult<Id> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(AppError::invalid("Type a message first."));
        }
        if text.chars().count() > MAX_MESSAGE_CHARS {
            return Err(AppError::invalid("That message is too long. Shorten it or put the text in a note and ask about the note."));
        }
        let db = &self.inner.db;
        db.get_thread(thread_id).await?;
        if !self.inner.busy_threads.lock().unwrap().insert(thread_id) {
            return Err(AppError::new(ErrorCode::Conflict, "Cilica is still answering. Wait for it to finish or stop it first."));
        }

        let turn_id = new_id();
        let category_name = match view.category_id {
            Some(c) => self.inner.notes.get_category(c).await.ok().map(|c| c.name),
            None => None,
        };
        let user_raw = vec![ChatMessage::user(prompt::user_turn_text(&view, category_name.as_deref(), &text))];
        let user_message = Message {
            id: new_id(),
            thread_id,
            turn_id,
            role: Role::User,
            parts: vec![MessagePart::Text { text: text.clone() }],
            created_at: now(),
        };
        let saved = async {
            db.save_message(&user_message, Some(&serde_json::to_string(&user_raw).unwrap_or_default())).await?;
            db.touch_thread(thread_id).await
        }
        .await;
        if let Err(e) = saved {
            self.inner.busy_threads.lock().unwrap().remove(&thread_id);
            return Err(e.into());
        }

        let cancel = CancellationToken::new();
        self.inner.turns.lock().unwrap().insert(turn_id, cancel.clone());
        self.inner.events.emit(AppEvent::Assistant { event: AssistantEvent::TurnStarted { thread_id, turn_id, user_message } });
        self.emit_changed(vec![DataScope::Threads], None);

        let this = self.clone();
        tokio::spawn(async move {
            let end = this.run_chat_turn(thread_id, turn_id, &user_raw[0], cancel).await;
            this.inner.turns.lock().unwrap().remove(&turn_id);
            this.inner.busy_threads.lock().unwrap().remove(&thread_id);
            this.inner.events.emit(AppEvent::Assistant { event: AssistantEvent::TurnFinished { thread_id, turn_id, end } });
            this.after_turn(thread_id, turn_id, &text).await;
        });
        Ok(turn_id)
    }

    async fn run_chat_turn(&self, thread_id: Id, turn_id: Id, user_raw: &ChatMessage, cancel: CancellationToken) -> TurnEnd {
        let message = AssistantMessage { id: new_id(), thread_id, turn_id, created_at: now() };
        let settings = self.inner.providers.ai_settings().await;
        if Agent::is_bridge(&settings) {
            return self.run_bridge_turn(&settings, message, &user_raw.text(), cancel).await;
        }
        let provider = match self.inner.providers.provider(&settings) {
            Ok(p) => p,
            Err(e) => {
                let mut state = LoopState::default();
                state.push_error(e.message.clone());
                self.save_assistant(&message, &state).await;
                return TurnEnd::Error { message: e.message };
            }
        };
        let history = match self.history(thread_id).await {
            Ok(h) => h,
            Err(e) => return TurnEnd::Error { message: e.message },
        };
        let ctx = CallContext::chat(Some(thread_id), turn_id, settings.auto_apply_edits, Some(ChatEmit { thread_id, message_id: message.id }));
        let request = LoopRequest {
            model: settings.model.clone(),
            system: SYSTEM_PROMPT.to_string(),
            tools: tools::specs(false),
            max_tokens: self.inner.limits.max_tokens,
            effort: Some(Effort::Medium),
        };
        let (_, end) = self.run_loop(provider.as_ref(), &request, history, &ctx, &cancel, Some(&message)).await;
        match end {
            LoopEnd::Completed => TurnEnd::Completed,
            LoopEnd::Cancelled => TurnEnd::Cancelled,
            LoopEnd::Error(message) => TurnEnd::Error { message },
        }
    }

    pub(crate) async fn save_assistant(&self, message: &AssistantMessage, state: &LoopState) {
        let msg = Message {
            id: message.id,
            thread_id: message.thread_id,
            turn_id: message.turn_id,
            role: Role::Assistant,
            parts: state.parts.clone(),
            created_at: message.created_at,
        };
        let raw = serde_json::to_string(&state.raw).unwrap_or_else(|_| "[]".into());
        if let Err(e) = self.inner.db.save_message(&msg, Some(&raw)).await {
            tracing::error!(error = %e, "could not save the assistant message");
        }
    }

    async fn persist(&self, message: Option<&AssistantMessage>, state: &LoopState) {
        if let Some(m) = message {
            self.save_assistant(m, state).await;
        }
    }

    fn emit_text(&self, ctx: &CallContext, delta: &str) {
        if let Some(e) = ctx.emit {
            self.inner.events.emit(AppEvent::Assistant {
                event: AssistantEvent::TextDelta { thread_id: e.thread_id, turn_id: ctx.turn_id, message_id: e.message_id, delta: delta.to_string() },
            });
        }
    }

    /// The model ↔ tools loop shared by chat turns and task runs.
    pub(crate) async fn run_loop(
        &self,
        provider: &dyn Provider,
        base: &LoopRequest,
        history: Vec<ChatMessage>,
        ctx: &CallContext,
        cancel: &CancellationToken,
        persist: Option<&AssistantMessage>,
    ) -> (LoopState, LoopEnd) {
        let mut state = LoopState::default();

        for _step in 0..self.inner.limits.max_steps {
            if cancel.is_cancelled() {
                self.persist(persist, &state).await;
                return (state, LoopEnd::Cancelled);
            }
            let mut messages = history.clone();
            messages.extend(state.raw.iter().cloned());
            let request = ChatRequest {
                model: base.model.clone(),
                system: base.system.clone(),
                messages,
                tools: base.tools.clone(),
                max_tokens: base.max_tokens,
                effort: base.effort,
            };
            let mut stream = provider.stream(request, cancel.clone());
            let mut completion = None;
            let mut step_text = String::new();
            while let Some(event) = stream.next().await {
                match event {
                    Ok(StreamEvent::TextDelta(delta)) => {
                        state.push_text(&delta);
                        step_text.push_str(&delta);
                        self.emit_text(ctx, &delta);
                    }
                    Ok(StreamEvent::ToolCall(_)) => {}
                    Ok(StreamEvent::Done(c)) => completion = Some(c),
                    Err(LlmError::Cancelled) => {
                        self.persist(persist, &state).await;
                        return (state, LoopEnd::Cancelled);
                    }
                    Err(e) => {
                        let message = e.to_string();
                        state.push_error(message.clone());
                        self.persist(persist, &state).await;
                        return (state, LoopEnd::Error(message));
                    }
                }
            }
            let Some(completion) = completion else {
                if cancel.is_cancelled() {
                    self.persist(persist, &state).await;
                    return (state, LoopEnd::Cancelled);
                }
                let message = "The answer stopped early. Try again.".to_string();
                state.push_error(message.clone());
                self.persist(persist, &state).await;
                return (state, LoopEnd::Error(message));
            };
            if let Some(notice) = &completion.notice {
                state.push_error(notice.clone());
            }
            if !step_text.trim().is_empty() {
                state.last_text = step_text;
            }
            let calls: Vec<_> = completion
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => Some((id.clone(), name.clone(), input.clone())),
                    _ => None,
                })
                .collect();
            state.raw.push(ChatMessage { role: ChatRole::Assistant, content: completion.content.clone() });

            if completion.stop_reason != StopReason::ToolUse || calls.is_empty() {
                if !calls.is_empty() {
                    // Cut off mid tool call: answer the calls so the history stays valid.
                    let results = calls
                        .iter()
                        .map(|(id, _, _)| ContentBlock::ToolResult { tool_use_id: id.clone(), content: "Not run: the response was cut off.".into(), is_error: true })
                        .collect();
                    state.raw.push(ChatMessage { role: ChatRole::User, content: results });
                }
                match completion.stop_reason {
                    StopReason::Refusal => state.push_error("Cilica can't help with that request."),
                    StopReason::MaxTokens => state.push_error("The answer was cut off because it ran long. Ask Cilica to continue."),
                    _ => {}
                }
                self.persist(persist, &state).await;
                return (state, LoopEnd::Completed);
            }

            // Run this step's tool calls in parallel; results go back in one message, in order.
            let outcomes = futures::future::join_all(calls.iter().map(|(_, name, input)| self.inner.runner.call(ctx, name, input))).await;
            let mut results = Vec::with_capacity(outcomes.len());
            for ((id, _, _), outcome) in calls.iter().zip(outcomes) {
                state.parts.push(MessagePart::Tool { action: outcome.action.clone() });
                if let Some(p) = &outcome.proposal {
                    state.parts.push(MessagePart::Proposal { proposal_id: p.id });
                }
                results.push(ContentBlock::ToolResult { tool_use_id: id.clone(), content: outcome.content, is_error: outcome.is_error });
            }
            state.raw.push(ChatMessage { role: ChatRole::User, content: results });
            self.persist(persist, &state).await;
        }
        state.push_error(format!("Cilica stopped after {} steps. Ask it to continue if there is more to do.", self.inner.limits.max_steps));
        self.persist(persist, &state).await;
        (state, LoopEnd::Completed)
    }

    /// Rebuild the conversation from stored raw content, repaired and trimmed to the budget.
    pub(crate) async fn history(&self, thread_id: Id) -> AppResult<Vec<ChatMessage>> {
        let rows = self.inner.db.list_messages(thread_id).await?;
        let mut turns: Vec<(Id, Vec<ChatMessage>)> = Vec::new();
        for (message, raw) in rows {
            let Some(raw) = raw else { continue };
            let Ok(chunk) = serde_json::from_str::<Vec<ChatMessage>>(&raw) else { continue };
            match turns.last_mut() {
                Some((turn, msgs)) if *turn == message.turn_id => msgs.extend(chunk),
                _ => turns.push((message.turn_id, chunk)),
            }
        }
        Ok(trim_history(turns, self.inner.limits.history_chars))
    }

    /// Title the thread (fast model) after its first turn, and offer one Undo for the turn's changes.
    async fn after_turn(&self, thread_id: Id, turn_id: Id, first_text: &str) {
        // Undo toast for the turn's applied changes.
        if let Ok(actions) = self.inner.db.list_ai_actions_for_turn(turn_id).await {
            let applied: Vec<_> = actions.iter().filter(|(a, _)| a.status == AiActionStatus::Applied && a.can_undo).collect();
            if !applied.is_empty() {
                let title = if applied.len() == 1 { applied[0].0.summary.clone() } else { format!("Cilica made {} changes", applied.len()) };
                self.inner.events.emit(AppEvent::Toast {
                    toast: ToastRequest {
                        kind: ToastKind::Undo,
                        title,
                        description: None,
                        action_label: Some("Undo".into()),
                        action: Some(ToastAction::UndoTurn { turn_id }),
                    },
                });
            }
        }
        // Title.
        let Ok(thread) = self.inner.db.get_thread(thread_id).await else { return };
        if !thread.title.trim().is_empty() {
            return;
        }
        let title = self.make_title(first_text).await;
        if self.inner.db.rename_thread(thread_id, &title).await.is_ok() {
            self.emit_changed(vec![DataScope::Threads], None);
        }
    }

    async fn make_title(&self, text: &str) -> String {
        let fallback = fallback_title(text);
        let Ok((settings, provider)) = self.provider().await else { return fallback };
        if Agent::is_bridge(&settings) {
            return fallback;
        }
        let mut req = ChatRequest::new(
            settings.fast_model.clone(),
            "Write a title of 2 to 6 words for a conversation that starts with the message below. Sentence case, no quotes, no punctuation at the end, no emoji. Reply with the title only.",
            vec![ChatMessage::user(prompt::quoted("message", &[], text))],
        );
        req.max_tokens = 2_000;
        req.effort = Some(Effort::Low);
        match cilica_llm::complete(provider.as_ref(), req, CancellationToken::new()).await {
            Ok((t, _)) => clean_title(&t).unwrap_or(fallback),
            Err(_) => fallback,
        }
    }
}

/// First words of the message, as a title.
pub(crate) fn fallback_title(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().take(6).collect();
    let mut t = words.join(" ");
    if t.chars().count() > 60 {
        t = t.chars().take(60).collect();
    }
    if t.is_empty() { "New conversation".into() } else { t }
}

pub(crate) fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let t = line.trim_matches(|c: char| c == '"' || c == '\'' || c == '“' || c == '”' || c == '#' || c == '*').trim().trim_end_matches('.');
    (!t.is_empty() && t.chars().count() <= 80).then(|| t.to_string())
}

fn chars(msgs: &[ChatMessage]) -> usize {
    msgs.iter().map(|m| serde_json::to_string(m).map(|s| s.len()).unwrap_or(0)).sum()
}

/// Make the stored conversation safe to send: every tool call answered, and the whole within
/// `budget` characters. Trimming drops whole old turns first (the latest turns stay), then
/// shortens old tool results. Because trimming changes the prefix, provider-internal blocks
/// (thinking) are stripped from what remains, so the provider never sees edited history with
/// thinking signatures bound to the old prefix.
pub(crate) fn trim_history(turns: Vec<(Id, Vec<ChatMessage>)>, budget: usize) -> Vec<ChatMessage> {
    let mut turns: Vec<Vec<ChatMessage>> = turns.into_iter().map(|(_, m)| repair(m)).collect();
    let total: usize = turns.iter().map(|t| chars(t)).sum();
    if total <= budget {
        return turns.concat();
    }
    // Drop the oldest turns, keeping at least the last 2.
    let mut size = total;
    while size > budget && turns.len() > 2 {
        size -= chars(&turns[0]);
        turns.remove(0);
    }
    let last = turns.len().saturating_sub(1);
    for (i, turn) in turns.iter_mut().enumerate() {
        for msg in turn.iter_mut() {
            msg.content.retain(|b| !b.is_provider_internal());
            if i < last {
                for block in msg.content.iter_mut() {
                    if let ContentBlock::ToolResult { content, .. } = block
                        && content.chars().count() > OLD_TOOL_RESULT_CHARS
                    {
                        *content = format!("{}\n[cut to save space]", content.chars().take(OLD_TOOL_RESULT_CHARS).collect::<String>());
                    }
                }
            }
        }
        turn.retain(|m| !m.content.is_empty());
    }
    let mut out = turns.concat();
    // The conversation must start with the user.
    while out.first().is_some_and(|m| m.role != ChatRole::User) {
        out.remove(0);
    }
    out
}

/// Answer any tool call that has no result (a crash mid-turn), so the provider accepts the history.
fn repair(msgs: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::with_capacity(msgs.len());
    let mut iter = msgs.into_iter().peekable();
    while let Some(m) = iter.next() {
        let calls: Vec<String> = m.tool_uses().into_iter().map(|c| c.id).collect();
        let is_assistant = m.role == ChatRole::Assistant;
        out.push(m);
        if !is_assistant || calls.is_empty() {
            continue;
        }
        let answered: Vec<String> = iter
            .peek()
            .filter(|n| n.role == ChatRole::User)
            .map(|n| {
                n.content
                    .iter()
                    .filter_map(|b| if let ContentBlock::ToolResult { tool_use_id, .. } = b { Some(tool_use_id.clone()) } else { None })
                    .collect()
            })
            .unwrap_or_default();
        let missing: Vec<ContentBlock> = calls
            .into_iter()
            .filter(|id| !answered.contains(id))
            .map(|id| ContentBlock::ToolResult { tool_use_id: id, content: "This step was interrupted.".into(), is_error: true })
            .collect();
        if missing.is_empty() {
            continue;
        }
        match iter.peek_mut() {
            Some(next) if next.role == ChatRole::User && !answered.is_empty() => {
                next.content.splice(0..0, missing);
            }
            _ => out.push(ChatMessage { role: ChatRole::User, content: missing }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str) -> ChatMessage {
        ChatMessage {
            role: ChatRole::Assistant,
            content: vec![
                ContentBlock::Thinking { thinking: String::new(), signature: "s".into() },
                ContentBlock::ToolUse { id: id.into(), name: "get_note".into(), input: serde_json::json!({}) },
            ],
        }
    }

    fn result(id: &str, size: usize) -> ChatMessage {
        ChatMessage { role: ChatRole::User, content: vec![ContentBlock::ToolResult { tool_use_id: id.into(), content: "x".repeat(size), is_error: false }] }
    }

    #[test]
    fn history_under_budget_is_untouched() {
        let turns = vec![(new_id(), vec![ChatMessage::user("hi"), call("a"), result("a", 10), ChatMessage::assistant("ok")])];
        let out = trim_history(turns.clone(), 1_000_000);
        assert_eq!(out, turns[0].1);
    }

    #[test]
    fn trimming_drops_old_turns_strips_thinking_and_cuts_old_results() {
        let turns: Vec<_> = (0..5)
            .map(|i| (new_id(), vec![ChatMessage::user(format!("q{i}")), call(&format!("c{i}")), result(&format!("c{i}"), 5_000), ChatMessage::assistant("a")]))
            .collect();
        let out = trim_history(turns, 12_000);
        assert_eq!(out[0], ChatMessage::user("q3"));
        assert!(out.iter().all(|m| m.content.iter().all(|b| !b.is_provider_internal())));
        let first_result = out.iter().find_map(|m| m.content.iter().find_map(|b| if let ContentBlock::ToolResult { content, .. } = b { Some(content.clone()) } else { None })).unwrap();
        assert!(first_result.ends_with("[cut to save space]"));
    }

    #[test]
    fn unanswered_tool_calls_get_a_result() {
        let turns = vec![(new_id(), vec![ChatMessage::user("hi"), call("a")]), (new_id(), vec![ChatMessage::user("again")])];
        let out = trim_history(turns, 1_000_000);
        assert_eq!(out.len(), 4);
        assert!(matches!(&out[2].content[0], ContentBlock::ToolResult { tool_use_id, is_error: true, .. } if tool_use_id == "a"));
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_title("\"Acme kickoff prep.\"\n").as_deref(), Some("Acme kickoff prep"));
        assert_eq!(fallback_title("  what did   Sam promise to send by Friday after the call"), "what did Sam promise to send");
    }
}
