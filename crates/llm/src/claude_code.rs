//! Experimental bridge to the user's installed `claude` CLI (opt-in, behind the settings switch).
//!
//! Margin never reads or stores Claude credentials: it only runs the CLI, which uses its own sign-in.
//! A turn runs `claude -p --output-format stream-json` with:
//! - `--tools ""` (no built-in tools at all: no Bash, no file edits, no web),
//! - `--strict-mcp-config --mcp-config <json>` pointing at Margin's own MCP server (`margin-desktop --mcp`),
//! - `--allowedTools mcp__margin__<tool> …` so only Margin's tools run, and nothing prompts,
//! - `--resume <session>` to continue a thread.
//!
//! The CLI runs its own tool loop and calls Margin's MCP server, which applies the same tiers and
//! guardrails as the in-app agent. This module parses the stream-json output into [`BridgeEvent`]s.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;

use futures::Stream;
use margin_domain::ClaudeCodeStatus;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::{CancellationToken, ChatRequest, LlmError, Usage};

/// The MCP server name Margin registers; tools appear to the CLI as `mcp__margin__<tool>`.
pub const MCP_SERVER_NAME: &str = "margin";

/// Find `claude`: PATH first, then the usual install locations (GUI apps get a short PATH).
pub fn find_binary() -> Option<PathBuf> {
    let exe = if std::env::consts::OS == "windows" { "claude.exe" } else { "claude" };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from) {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".claude/local"));
        dirs.push(home.join(".npm-global/bin"));
        dirs.push(home.join(".bun/bin"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Installed? Which version? Signed in? Runs `claude --version` and `claude auth status --json`.
pub async fn detect(binary: Option<PathBuf>) -> ClaudeCodeStatus {
    let Some(bin) = binary.or_else(find_binary) else {
        return ClaudeCodeStatus { installed: false, version: None, signed_in: false };
    };
    let version = run_capture(&bin, &["--version"]).await.and_then(|(ok, out)| ok.then(|| parse_version(&out)).flatten());
    let signed_in = match run_capture(&bin, &["auth", "status", "--json"]).await {
        Some((ok, out)) => parse_auth_status(ok, &out),
        None => false,
    };
    ClaudeCodeStatus { installed: true, version, signed_in }
}

async fn run_capture(bin: &Path, args: &[&str]) -> Option<(bool, String)> {
    let fut = Command::new(bin).args(args).stdin(Stdio::null()).kill_on_drop(true).output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(10), fut).await.ok()?.ok()?;
    Some((out.status.success(), String::from_utf8_lossy(&out.stdout).to_string()))
}

/// "2.1.321 (Claude Code)" → "2.1.321".
pub fn parse_version(out: &str) -> Option<String> {
    out.split_whitespace().next().filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit())).map(str::to_string)
}

/// Reads only the yes/no sign-in flag from `claude auth status --json`; never any token.
pub fn parse_auth_status(exit_ok: bool, out: &str) -> bool {
    match serde_json::from_str::<Value>(out) {
        Ok(v) => ["loggedIn", "logged_in", "authenticated", "signedIn"]
            .iter()
            .find_map(|k| v.get(*k).and_then(Value::as_bool))
            .unwrap_or(exit_ok),
        Err(_) => exit_ok,
    }
}

/// One bridged turn.
#[derive(Debug, Clone)]
pub struct ClaudeCodeRequest {
    pub binary: PathBuf,
    pub prompt: String,
    /// Appended to Claude Code's system prompt (Margin's voice and data rules).
    pub system_append: String,
    /// Claude Code session to continue (`threads.external_id`).
    pub resume: Option<String>,
    pub model: Option<String>,
    /// The MCP server command: Margin's own binary with `--mcp …`.
    pub mcp_command: PathBuf,
    pub mcp_args: Vec<String>,
    /// Tool names (without the `mcp__margin__` prefix) the CLI may call.
    pub tool_names: Vec<String>,
    /// Working directory for the CLI (an empty folder in Margin's data dir).
    pub cwd: PathBuf,
}

/// The command-line arguments for a turn (the prompt goes in on stdin).
pub fn build_args(req: &ClaudeCodeRequest) -> Vec<String> {
    let mcp = serde_json::json!({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "type": "stdio",
                "command": req.mcp_command.to_string_lossy(),
                "args": req.mcp_args,
            }
        }
    });
    let allowed: Vec<String> = req.tool_names.iter().map(|t| format!("mcp__{MCP_SERVER_NAME}__{t}")).collect();
    let mut args = vec![
        "-p".to_string(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--include-partial-messages".into(),
        // No built-in tools: no Bash, no file reads or edits, no web.
        "--tools".into(),
        String::new(),
        "--strict-mcp-config".into(),
        "--mcp-config".into(),
        mcp.to_string(),
        "--allowedTools".into(),
        allowed.join(","),
        "--disallowedTools".into(),
        "Bash,Edit,Write,MultiEdit,NotebookEdit,Read,Glob,Grep,WebFetch,WebSearch,Task".into(),
        // Anything that would prompt is denied; Margin's guardrails decide the rest.
        "--permission-prompts".into(),
        "none".into(),
        "--append-system-prompt".into(),
        req.system_append.clone(),
    ];
    if let Some(session) = &req.resume {
        args.push("--resume".into());
        args.push(session.clone());
    }
    if let Some(model) = &req.model {
        args.push("--model".into());
        args.push(model.clone());
    }
    args
}

/// What the CLI reported.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    /// The CLI's session id (store it as the thread's external id for `--resume`).
    Session { id: String },
    TextDelta(String),
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, is_error: bool, content: String },
    Done { session_id: Option<String>, usage: Usage, is_error: bool, result: String },
}

/// Line-by-line parser for `--output-format stream-json`.
#[derive(Debug, Default)]
pub struct StreamJsonParser {
    saw_partial_text: bool,
}

impl StreamJsonParser {
    pub fn feed(&mut self, line: &str) -> Vec<BridgeEvent> {
        let line = line.trim();
        if line.is_empty() {
            return vec![];
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
        let mut out = Vec::new();
        match v["type"].as_str().unwrap_or("") {
            "system" if v["subtype"] == "init" => {
                if let Some(id) = v["session_id"].as_str() {
                    out.push(BridgeEvent::Session { id: id.to_string() });
                }
            }
            "stream_event" => {
                let ev = &v["event"];
                if ev["type"] == "content_block_delta" && ev["delta"]["type"] == "text_delta" {
                    let text = ev["delta"]["text"].as_str().unwrap_or("");
                    if !text.is_empty() {
                        self.saw_partial_text = true;
                        out.push(BridgeEvent::TextDelta(text.to_string()));
                    }
                }
            }
            "assistant" => {
                for block in v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    match block["type"].as_str().unwrap_or("") {
                        // Full messages repeat what the partial stream already sent.
                        "text" if !self.saw_partial_text => {
                            if let Some(t) = block["text"].as_str().filter(|t| !t.is_empty()) {
                                out.push(BridgeEvent::TextDelta(t.to_string()));
                            }
                        }
                        "tool_use" => out.push(BridgeEvent::ToolUse {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: block["name"].as_str().unwrap_or("").to_string(),
                            input: block["input"].clone(),
                        }),
                        _ => {}
                    }
                }
            }
            "user" => {
                for block in v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    if block["type"] == "tool_result" {
                        let content = match &block["content"] {
                            Value::String(s) => s.clone(),
                            Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
                            _ => String::new(),
                        };
                        out.push(BridgeEvent::ToolResult {
                            tool_use_id: block["tool_use_id"].as_str().unwrap_or("").to_string(),
                            is_error: block["is_error"].as_bool().unwrap_or(false),
                            content,
                        });
                    }
                }
            }
            "result" => {
                let u = &v["usage"];
                let get = |k: &str| u[k].as_u64().unwrap_or(0).min(u64::from(u32::MAX)) as u32;
                out.push(BridgeEvent::Done {
                    session_id: v["session_id"].as_str().map(str::to_string),
                    usage: Usage {
                        input_tokens: get("input_tokens"),
                        output_tokens: get("output_tokens"),
                        cache_read_tokens: get("cache_read_input_tokens"),
                        cache_write_tokens: get("cache_creation_input_tokens"),
                    },
                    is_error: v["is_error"].as_bool().unwrap_or(false) || v["subtype"].as_str().is_some_and(|s| s != "success"),
                    result: v["result"].as_str().unwrap_or("").to_string(),
                });
            }
            _ => {}
        }
        out
    }
}

pub type BridgeStream = Pin<Box<dyn Stream<Item = Result<BridgeEvent, LlmError>> + Send>>;

/// Run one turn through the CLI. Cancelling kills the process.
pub fn run(req: ClaudeCodeRequest, cancel: CancellationToken) -> BridgeStream {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        if let Err(e) = run_inner(req, &cancel, &tx).await {
            let _ = tx.send(Err(e)).await;
        }
    });
    crate::receiver_stream(rx)
}

async fn run_inner(
    req: ClaudeCodeRequest,
    cancel: &CancellationToken,
    tx: &mpsc::Sender<Result<BridgeEvent, LlmError>>,
) -> Result<(), LlmError> {
    let _ = std::fs::create_dir_all(&req.cwd);
    let mut child = Command::new(&req.binary)
        .args(build_args(&req))
        .current_dir(&req.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| LlmError::Unavailable("Margin could not start Claude Code. Check that it is installed and try again.".into()))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(req.prompt.as_bytes()).await;
        drop(stdin);
    }
    let stdout = child.stdout.take().ok_or_else(|| LlmError::Protocol("no output from Claude Code".into()))?;
    let mut lines = BufReader::new(stdout).lines();
    let mut parser = StreamJsonParser::default();
    let mut finished = false;
    loop {
        let line = tokio::select! {
            l = lines.next_line() => l,
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(LlmError::Cancelled);
            }
        };
        let Ok(Some(line)) = line else { break };
        for ev in parser.feed(&line) {
            finished |= matches!(ev, BridgeEvent::Done { .. });
            let _ = tx.send(Ok(ev)).await;
        }
    }
    let status = child.wait().await.ok();
    if !finished {
        let ok = status.is_some_and(|s| s.success());
        return Err(LlmError::Unavailable(if ok {
            "Claude Code stopped before it answered. Try again.".into()
        } else {
            "Claude Code stopped with an error. Check that it is signed in (run `claude` once in a terminal) and try again.".into()
        }));
    }
    Ok(())
}

/// Text-only use of the CLI (meeting summaries, titles) for people who chose the bridge: no tools,
/// no MCP, no session saved. Implements [`Provider`]; tool specs are ignored.
pub struct ClaudeCodeTextProvider {
    pub binary: PathBuf,
    pub cwd: PathBuf,
}

impl ClaudeCodeTextProvider {
    /// Arguments for a one-shot text request.
    pub fn build_args(&self, req: &ChatRequest) -> Vec<String> {
        let mut args: Vec<String> = [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--tools",
            "",
            "--strict-mcp-config",
            "--no-session-persistence",
            "--permission-prompts",
            "none",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        if !req.system.is_empty() {
            args.push("--append-system-prompt".into());
            args.push(req.system.clone());
        }
        if req.model.starts_with("claude-") {
            args.push("--model".into());
            args.push(req.model.clone());
        }
        args
    }

    /// The conversation flattened into one prompt.
    pub fn flatten(req: &ChatRequest) -> String {
        req.messages
            .iter()
            .map(|m| match m.role {
                crate::ChatRole::User => m.text(),
                crate::ChatRole::Assistant => format!("(Your earlier reply:)\n{}", m.text()),
            })
            .filter(|t| !t.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[async_trait::async_trait]
impl crate::Provider for ClaudeCodeTextProvider {
    fn display_name(&self) -> &str {
        "Claude Code"
    }

    fn stream(&self, request: ChatRequest, cancel: CancellationToken) -> crate::ChatStream {
        let (tx, rx) = mpsc::channel(64);
        let binary = self.binary.clone();
        let cwd = self.cwd.clone();
        let args = self.build_args(&request);
        let prompt = Self::flatten(&request);
        tokio::spawn(async move {
            let result: Result<(), LlmError> = async {
                let _ = std::fs::create_dir_all(&cwd);
                let mut child = Command::new(&binary)
                    .args(&args)
                    .current_dir(&cwd)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|_| LlmError::Unavailable("Margin could not start Claude Code. Check that it is installed and try again.".into()))?;
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(prompt.as_bytes()).await;
                }
                let stdout = child.stdout.take().ok_or_else(|| LlmError::Protocol("no output from Claude Code".into()))?;
                let mut lines = BufReader::new(stdout).lines();
                let mut parser = StreamJsonParser::default();
                let mut text = String::new();
                loop {
                    let line = tokio::select! {
                        l = lines.next_line() => l,
                        _ = cancel.cancelled() => { let _ = child.kill().await; return Err(LlmError::Cancelled); }
                    };
                    let Ok(Some(line)) = line else { break };
                    for ev in parser.feed(&line) {
                        match ev {
                            BridgeEvent::TextDelta(d) => {
                                text.push_str(&d);
                                let _ = tx.send(Ok(crate::StreamEvent::TextDelta(d))).await;
                            }
                            BridgeEvent::Done { usage, is_error, result, .. } => {
                                if is_error {
                                    return Err(LlmError::Unavailable(format!("Claude Code stopped: {}", result.trim())));
                                }
                                let content = vec![crate::ContentBlock::Text { text: text.clone() }];
                                let done = crate::Completion { stop_reason: crate::StopReason::EndTurn, usage, content, notice: None };
                                let _ = tx.send(Ok(crate::StreamEvent::Done(done))).await;
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }
                Err(LlmError::Unavailable("Claude Code stopped before it answered. Check that it is signed in and try again.".into()))
            }
            .await;
            if let Err(e) = result {
                let _ = tx.send(Err(e)).await;
            }
        });
        crate::receiver_stream(rx)
    }

    async fn verify(&self) -> Result<(), LlmError> {
        let status = detect(Some(self.binary.clone())).await;
        if status.installed && status.signed_in {
            Ok(())
        } else {
            Err(LlmError::Unavailable("Claude Code is not signed in. Run `claude` once in a terminal to sign in.".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/claude_code_stream.jsonl");

    #[test]
    fn parses_stream_json_turn() {
        let mut parser = StreamJsonParser::default();
        let events: Vec<_> = FIXTURE.lines().flat_map(|l| parser.feed(l)).collect();
        assert_eq!(events[0], BridgeEvent::Session { id: "5f0c2c1e-1111-4a5b-9a1b-000000000001".into() });
        let text: String = events.iter().filter_map(|e| if let BridgeEvent::TextDelta(t) = e { Some(t.as_str()) } else { None }).collect();
        assert_eq!(text, "Let me search. Found 2 notes about Acme.");
        assert!(events.iter().any(|e| matches!(e, BridgeEvent::ToolUse { name, .. } if name == "mcp__margin__search_notes")));
        assert!(events.iter().any(|e| matches!(e, BridgeEvent::ToolResult { is_error: false, content, .. } if content.contains("Acme kickoff"))));
        let BridgeEvent::Done { session_id, usage, is_error, .. } = events.last().unwrap() else { panic!() };
        assert_eq!(session_id.as_deref(), Some("5f0c2c1e-1111-4a5b-9a1b-000000000001"));
        assert_eq!(usage.output_tokens, 64);
        assert!(!is_error);
    }

    #[test]
    fn args_lock_down_tools_and_resume() {
        let req = ClaudeCodeRequest {
            binary: "claude".into(),
            prompt: "hi".into(),
            system_append: "Be Margin.".into(),
            resume: Some("sess-1".into()),
            model: Some("claude-sonnet-5-5".into()),
            mcp_command: "/Applications/Margin.app/Contents/MacOS/margin-desktop".into(),
            mcp_args: vec!["--mcp".into()],
            tool_names: vec!["search_notes".into(), "trash_note".into()],
            cwd: "/tmp".into(),
        };
        let args = build_args(&req);
        let pos = |flag: &str| args.iter().position(|a| a == flag).unwrap();
        assert_eq!(args[pos("--tools") + 1], "");
        assert_eq!(args[pos("--allowedTools") + 1], "mcp__margin__search_notes,mcp__margin__trash_note");
        assert!(args[pos("--disallowedTools") + 1].contains("Bash"));
        assert_eq!(args[pos("--resume") + 1], "sess-1");
        let mcp: Value = serde_json::from_str(&args[pos("--mcp-config") + 1]).unwrap();
        assert_eq!(mcp["mcpServers"]["margin"]["args"][0], "--mcp");
        assert!(args.contains(&"--strict-mcp-config".to_string()));
    }

    #[test]
    fn text_provider_has_no_tools_and_no_session() {
        let p = ClaudeCodeTextProvider { binary: "claude".into(), cwd: "/tmp".into() };
        let mut req = ChatRequest::new("claude-sonnet-5-5", "Write notes.", vec![crate::ChatMessage::user("transcript here")]);
        req.tools = vec![crate::ToolSpec { name: "x".into(), description: "x".into(), input_schema: serde_json::json!({}) }];
        let args = p.build_args(&req);
        let pos = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[pos + 1], "");
        assert!(args.contains(&"--no-session-persistence".to_string()));
        assert!(!args.iter().any(|a| a == "--mcp-config"));
        assert_eq!(ClaudeCodeTextProvider::flatten(&req), "transcript here");
    }

    #[test]
    fn reads_version_and_sign_in_flag_only() {
        assert_eq!(parse_version("2.1.321 (Claude Code)\n").as_deref(), Some("2.1.321"));
        assert!(parse_auth_status(true, r#"{"loggedIn": true, "authMethod": "claude.ai"}"#));
        assert!(!parse_auth_status(true, r#"{"loggedIn": false}"#));
        assert!(!parse_auth_status(false, "not json"));
    }
}
