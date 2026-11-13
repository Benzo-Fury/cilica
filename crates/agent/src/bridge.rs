//! Chat turns through the experimental Claude Code bridge.
//!
//! The CLI runs its own tool loop and calls Cilica's MCP server (a second Cilica process on the same
//! database), which applies the same tiers and writes the audit rows. This side streams the text,
//! remembers the CLI session for `--resume`, and after each tool result reads the turn's new audit
//! rows and proposals from the database to emit the same Tool / Proposal events as a normal turn.

use std::collections::HashMap;

use futures::StreamExt;
use cilica_domain::{AiSettings, AppEvent, AssistantEvent, DataScope, Id, MessagePart, TurnEnd};
use cilica_llm::claude_code::{self, BridgeEvent, ClaudeCodeRequest};
use cilica_llm::{CancellationToken, ChatMessage, LlmError};

use crate::prompt::SYSTEM_PROMPT;
use crate::turn::{AssistantMessage, LoopState};
use crate::{Agent, tools};

impl Agent {
    pub(crate) async fn run_bridge_turn(&self, settings: &AiSettings, message: AssistantMessage, prompt_text: &str, cancel: CancellationToken) -> TurnEnd {
        let mut state = LoopState::default();
        let fail = |state: &mut LoopState, msg: String| {
            state.parts.push(MessagePart::Error { message: msg.clone() });
            TurnEnd::Error { message: msg }
        };
        let Some(config) = self.inner.providers.bridge() else {
            let end = fail(&mut state, "The Claude Code bridge is not available in this build.".into());
            self.save_assistant(&message, &state).await;
            return end;
        };
        let Some(binary) = config.binary.clone().or_else(claude_code::find_binary) else {
            let end = fail(&mut state, "Claude Code is not installed. Install it, sign in once in a terminal, then try again.".into());
            self.save_assistant(&message, &state).await;
            return end;
        };
        let resume = self.inner.db.thread_external_id(message.thread_id).await.ok().flatten();
        let mut mcp_args = config.mcp_args.clone();
        mcp_args.extend(["--thread".into(), message.thread_id.to_string(), "--turn".into(), message.turn_id.to_string()]);
        let request = ClaudeCodeRequest {
            binary,
            prompt: prompt_text.to_string(),
            system_append: SYSTEM_PROMPT.to_string(),
            resume,
            model: settings.model.starts_with("claude-").then(|| settings.model.clone()),
            mcp_command: config.mcp_command.clone(),
            mcp_args,
            tool_names: tools::names(),
            cwd: config.cwd.clone(),
        };

        let emit = |event: AssistantEvent| self.inner.events.emit(AppEvent::Assistant { event });
        let mut stream = claude_code::run(request, cancel.clone());
        let mut text = String::new();
        let mut seen: HashMap<Id, cilica_domain::AiActionStatus> = HashMap::new();
        let mut end = TurnEnd::Completed;
        while let Some(event) = stream.next().await {
            match event {
                Ok(BridgeEvent::Session { id }) => {
                    let _ = self.inner.db.set_thread_external_id(message.thread_id, Some(&id)).await;
                }
                Ok(BridgeEvent::TextDelta(delta)) => {
                    text.push_str(&delta);
                    if let Some(MessagePart::Text { text }) = state.parts.last_mut() {
                        text.push_str(&delta);
                    } else {
                        state.parts.push(MessagePart::Text { text: delta.clone() });
                    }
                    emit(AssistantEvent::TextDelta { thread_id: message.thread_id, turn_id: message.turn_id, message_id: message.id, delta });
                }
                Ok(BridgeEvent::ToolUse { .. }) => {}
                Ok(BridgeEvent::ToolResult { .. }) => {
                    self.sync_bridge_actions(&message, &mut state, &mut seen).await;
                }
                Ok(BridgeEvent::Done { session_id, is_error, result, .. }) => {
                    if let Some(id) = session_id {
                        let _ = self.inner.db.set_thread_external_id(message.thread_id, Some(&id)).await;
                    }
                    if is_error {
                        let msg = if result.trim().is_empty() { "Claude Code stopped with an error. Try again.".to_string() } else { format!("Claude Code stopped: {}", result.trim()) };
                        end = fail(&mut state, msg);
                    }
                }
                Err(LlmError::Cancelled) => {
                    end = TurnEnd::Cancelled;
                    break;
                }
                Err(e) => {
                    end = fail(&mut state, e.to_string());
                    break;
                }
            }
            self.save_bridge(&message, &state, &text).await;
        }
        self.sync_bridge_actions(&message, &mut state, &mut seen).await;
        self.save_bridge(&message, &state, &text).await;
        end
    }

    async fn save_bridge(&self, message: &AssistantMessage, state: &LoopState, text: &str) {
        // Raw history keeps the text only, so a later switch to an API provider still has context.
        let snapshot = LoopState { parts: state.parts.clone(), raw: vec![ChatMessage::assistant(text)], last_text: text.to_string() };
        self.save_assistant(message, &snapshot).await;
    }

    /// Emit Tool / Proposal events for audit rows the MCP process wrote for this turn.
    async fn sync_bridge_actions(&self, message: &AssistantMessage, state: &mut LoopState, seen: &mut HashMap<Id, cilica_domain::AiActionStatus>) {
        let Ok(actions) = self.inner.db.list_ai_actions_for_turn(message.turn_id).await else { return };
        let mut changed = false;
        for (action, _) in actions {
            if seen.get(&action.id) == Some(&action.status) {
                continue;
            }
            let first_sight = !seen.contains_key(&action.id);
            seen.insert(action.id, action.status);
            changed = true;
            match state.parts.iter_mut().find(|p| matches!(p, MessagePart::Tool { action: a } if a.id == action.id)) {
                Some(MessagePart::Tool { action: a }) => *a = action.clone(),
                _ => state.parts.push(MessagePart::Tool { action: action.clone() }),
            }
            self.inner.events.emit(AppEvent::Assistant {
                event: AssistantEvent::Tool { thread_id: message.thread_id, turn_id: message.turn_id, message_id: message.id, action: action.clone() },
            });
            if let (true, Some(pid)) = (first_sight || action.proposal_id.is_some(), action.proposal_id)
                && !state.parts.iter().any(|p| matches!(p, MessagePart::Proposal { proposal_id } if *proposal_id == pid))
                && let Ok(proposal) = self.inner.db.get_proposal(pid).await
            {
                state.parts.push(MessagePart::Proposal { proposal_id: pid });
                self.inner.events.emit(AppEvent::Assistant {
                    event: AssistantEvent::Proposal { thread_id: message.thread_id, turn_id: message.turn_id, message_id: message.id, proposal },
                });
            }
        }
        if changed {
            use DataScope::*;
            self.emit_changed(vec![Notes, Categories, Trash, Versions, Transcript, People, Tasks, Proposals, AiActions], None);
        }
    }
}
