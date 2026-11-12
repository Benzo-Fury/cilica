//! Assistant threads, turns, proposals, audit log, providers. Owner: AI agent.
//!
//! Every command here is part of the IPC contract in `src/ipc/bindings.ts`. Keep names, arguments and
//! return types as they are unless the orchestrator agrees to a change; replace the stub bodies.
//!
//! API keys live only in the OS keychain (service `app.margin.desktop`, one entry per provider). They
//! are read on demand by the provider's key loader and never logged, stored in SQLite or sent over IPC.

use std::path::PathBuf;
use std::sync::Arc;

use margin_agent::{Agent, BridgeConfig, DbNoteStore, ProviderSource};
use margin_domain::*;
use margin_llm::anthropic::AnthropicProvider;
use margin_llm::claude_code::{self, ClaudeCodeTextProvider};
use margin_llm::openai::OpenAiCompatProvider;
use margin_llm::{KeyLoader, LlmError, Provider};
use margin_store::Db;
use tauri::{AppHandle, Manager, State};

use crate::core::Core;

const KEYCHAIN_SERVICE: &str = "app.margin.desktop";

// ── Keys ─────────────────────────────────────────────────────────────────

/// In-memory copy of keychain answers, so each provider's key is read from the keychain at most once
/// per launch (every read can raise a macOS access prompt).
///
/// Only definite answers are cached: a key, or "no entry". A failed read (keychain locked, access
/// denied) is not cached, so the next use asks again instead of staying keyless until relaunch.
/// The cache lives only in process memory; it is never persisted, logged or sent over IPC.
struct KeyCache {
    /// Account name → `Some(key)` or `None` for a confirmed missing entry.
    entries: std::sync::Mutex<std::collections::HashMap<&'static str, Option<String>>>,
}

impl KeyCache {
    /// An empty cache.
    fn new() -> Self {
        Self { entries: std::sync::Mutex::new(std::collections::HashMap::new()) }
    }

    /// Returns the cached answer for `account`, or calls `load` once and caches a definite result.
    /// `load` returns `Ok(Some)` for a key, `Ok(None)` for no entry and `Err` for a failed read.
    fn get_or_load(&self, account: &'static str, load: impl FnOnce() -> Result<Option<String>, ()>) -> Option<String> {
        // Serve a cached answer without touching the keychain
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = entries.get(account) {
            return cached.clone();
        }

        // Read once; cache only definite answers
        match load() {
            Ok(value) => {
                let value = value.filter(|k| !k.trim().is_empty());
                entries.insert(account, value.clone());
                value
            }
            Err(()) => None,
        }
    }

    /// Records a value the app just wrote (`Some`) or removed (`None`), so no re-read is needed.
    fn put(&self, account: &'static str, value: Option<String>) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).insert(account, value);
    }
}

/// The process-wide key cache behind [`KeyStore`].
static KEY_CACHE: std::sync::LazyLock<KeyCache> = std::sync::LazyLock::new(KeyCache::new);

/// The OS keychain, one entry per provider, read through [`KeyCache`].
pub struct KeyStore;

impl KeyStore {
    /// Keychain account for a provider, or `None` for providers that take no key.
    fn account(kind: ProviderKind) -> Option<&'static str> {
        match kind {
            ProviderKind::Anthropic => Some("anthropic"),
            ProviderKind::OpenaiCompat => Some("openai_compat"),
            ProviderKind::Ollama | ProviderKind::ClaudeCode => None,
        }
    }

    /// The provider's key, read from the keychain on first use and from memory after that.
    pub fn get(kind: ProviderKind) -> Option<String> {
        let account = Self::account(kind)?;
        KEY_CACHE.get_or_load(account, || {
            let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|_| ())?;
            match entry.get_password() {
                Ok(key) => Ok(Some(key)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(()),
            }
        })
    }

    /// Saves the key in the keychain and updates the cache.
    pub fn set(kind: ProviderKind, key: &str) -> AppResult<()> {
        let account = Self::account(kind).ok_or_else(|| AppError::invalid("This provider doesn't use a key."))?;
        keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .and_then(|e| e.set_password(key))
            .map_err(|_| AppError::unavailable("Margin could not save the key in your keychain. Check that the keychain is unlocked and try again."))?;
        KEY_CACHE.put(account, Some(key.to_string()).filter(|k| !k.trim().is_empty()));
        Ok(())
    }

    /// Removes the key from the keychain and the cache.
    pub fn clear(kind: ProviderKind) -> AppResult<()> {
        let Some(account) = Self::account(kind) else { return Ok(()) };
        match keyring::Entry::new(KEYCHAIN_SERVICE, account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => {
                KEY_CACHE.put(account, None);
                Ok(())
            }
            Err(_) => Err(AppError::unavailable("Margin could not remove the key from your keychain. Try again.")),
        }
    }

    fn loader(kind: ProviderKind) -> KeyLoader {
        Arc::new(move || KeyStore::get(kind))
    }
}

// ── Providers ────────────────────────────────────────────────────────────

/// Builds providers from settings and keychain keys; supplies the Claude Code bridge config.
pub struct AppProviders {
    db: Db,
    db_path: PathBuf,
    data_dir: PathBuf,
}

/// A readable name for an OpenAI-compatible server.
fn compat_name(base_url: &str) -> &'static str {
    let u = base_url.to_lowercase();
    if u.contains("openrouter") {
        "OpenRouter"
    } else if u.contains("groq") {
        "Groq"
    } else if u.contains("api.openai.com") {
        "OpenAI"
    } else if u.contains(":1234") {
        "LM Studio"
    } else {
        "Your provider"
    }
}

fn is_local(base_url: &str) -> bool {
    let u = base_url.to_lowercase();
    u.contains("localhost") || u.contains("127.0.0.1") || u.contains("[::1]")
}

impl AppProviders {
    fn build(&self, settings: &AiSettings) -> AppResult<Arc<dyn Provider>> {
        Ok(match settings.provider {
            ProviderKind::Anthropic => {
                // Fail fast with what to do, so turns, summaries and chips do not wait on the network.
                if KeyStore::get(ProviderKind::Anthropic).is_none() {
                    return Err(AppError::unavailable("Add your Anthropic API key in Settings › Margin to use Margin."));
                }
                Arc::new(AnthropicProvider::new(KeyStore::loader(ProviderKind::Anthropic)))
            }
            ProviderKind::OpenaiCompat => {
                let base = settings
                    .base_url
                    .clone()
                    .filter(|u| !u.trim().is_empty())
                    .ok_or_else(|| AppError::unavailable("Add the server address for your OpenAI-compatible provider in Settings › Margin."))?;
                // Local servers (LM Studio) work without a key; others need one.
                let has_key = KeyStore::get(ProviderKind::OpenaiCompat).is_some();
                if !has_key && !is_local(&base) {
                    return Err(AppError::unavailable(format!("Add your {} API key in Settings › Margin to use Margin.", compat_name(&base))));
                }
                let key = has_key.then(|| KeyStore::loader(ProviderKind::OpenaiCompat));
                Arc::new(OpenAiCompatProvider::new(compat_name(&base), base, key))
            }
            ProviderKind::Ollama => Arc::new(OpenAiCompatProvider::ollama(settings.base_url.clone().filter(|u| !u.trim().is_empty()))),
            ProviderKind::ClaudeCode => {
                let binary = claude_code::find_binary()
                    .ok_or_else(|| AppError::unavailable("Claude Code is not installed. Install it or pick another provider in Settings › Margin."))?;
                Arc::new(ClaudeCodeTextProvider { binary, cwd: self.data_dir.join("claude-bridge") })
            }
        })
    }
}

#[async_trait::async_trait]
impl ProviderSource for AppProviders {
    async fn ai_settings(&self) -> AiSettings {
        self.db.ai_settings().await.unwrap_or_default()
    }

    fn provider(&self, settings: &AiSettings) -> AppResult<Arc<dyn Provider>> {
        self.build(settings)
    }

    fn bridge(&self) -> Option<BridgeConfig> {
        let exe = std::env::current_exe().ok()?;
        Some(BridgeConfig {
            binary: None,
            mcp_command: exe,
            mcp_args: vec!["--mcp".into(), "--db".into(), self.db_path.to_string_lossy().into_owned()],
            cwd: self.data_dir.join("claude-bridge"),
        })
    }
}

/// Managed state for this area.
pub struct AssistantState {
    pub agent: Agent,
    providers: Arc<AppProviders>,
}

/// The end-of-meeting summariser. The recording pipeline (Audio) asks for it when a meeting ends,
/// so it is looked up lazily rather than at setup.
pub fn summarizer(app: &AppHandle) -> std::sync::Arc<dyn MeetingSummarizer> {
    if let Some(state) = app.try_state::<AssistantState>() {
        return state.agent.summarizer();
    }
    struct Unavailable;
    #[async_trait::async_trait]
    impl MeetingSummarizer for Unavailable {
        async fn summarize_meeting(&self, _note_id: Id) -> AppResult<()> {
            Err(AppError::unavailable("Margin is still starting. Try again in a moment."))
        }
    }
    std::sync::Arc::new(Unavailable)
}

/// The assistant service, for other areas (the scheduler).
pub fn agent(app: &AppHandle) -> Option<Agent> {
    app.try_state::<AssistantState>().map(|s| s.agent.clone())
}

/// Called once at startup, after the database is open. Create this area's services here and
/// register them with `app.manage(...)`.
pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    let providers = Arc::new(AppProviders { db: core.db.clone(), db_path: core.data_dir.join("margin.db"), data_dir: core.data_dir.clone() });
    let agent = Agent::new(core.db.clone(), Arc::new(DbNoteStore(core.db.clone())), core.events.clone(), providers.clone());
    // Settings › Margin › Delete conversations after: at launch, then every hour.
    let housekeeper = agent.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match housekeeper.delete_old_threads().await {
                Ok(0) => {}
                Ok(count) => tracing::info!(count, "deleted old conversations"),
                Err(error) => tracing::warn!(message = %error.message, "could not delete old conversations"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    });
    app.manage(AssistantState { agent, providers });
    Ok(())
}

/// `margin-desktop --mcp --db <path> …`: serve Margin's tools over stdio for the Claude Code bridge.
/// Returns the process exit code. Called from `main` before the app starts.
pub fn run_mcp_server() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match margin_mcp::parse_args(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("margin --mcp: {e}");
            return 2;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("margin --mcp: {e}");
            return 1;
        }
    };
    match runtime.block_on(margin_mcp::run_stdio(parsed)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("margin --mcp: {e}");
            1
        }
    }
}

// ── Commands ─────────────────────────────────────────────────────────────

/// Newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_threads(state: State<'_, AssistantState>) -> AppResult<Vec<Thread>> {
    state.agent.list_threads().await
}

#[tauri::command]
#[specta::specta]
pub async fn create_thread(state: State<'_, AssistantState>) -> AppResult<Thread> {
    state.agent.create_thread().await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_thread(state: State<'_, AssistantState>, thread_id: Id) -> AppResult<()> {
    state.agent.delete_thread(thread_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_thread_messages(state: State<'_, AssistantState>, thread_id: Id) -> AppResult<Vec<Message>> {
    state.agent.get_thread_messages(thread_id).await
}

/// Start an assistant turn. Returns the turn id at once; progress streams as Assistant events.
#[tauri::command]
#[specta::specta]
pub async fn send_message(state: State<'_, AssistantState>, thread_id: Id, text: String, view: ViewContext) -> AppResult<Id> {
    state.agent.send_message(thread_id, text, view).await
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_turn(state: State<'_, AssistantState>, turn_id: Id) -> AppResult<()> {
    state.agent.cancel_turn(turn_id);
    Ok(())
}

/// Three or four suggested prompts for the current view. Cached per note.
#[tauri::command]
#[specta::specta]
pub async fn get_prompt_chips(state: State<'_, AssistantState>, view: ViewContext) -> AppResult<Vec<PromptChip>> {
    state.agent.get_prompt_chips(&view).await
}

/// One short line about the user's week, or nothing. Ask at most once a day.
#[tauri::command]
#[specta::specta]
pub async fn get_week_line(state: State<'_, AssistantState>) -> AppResult<Option<String>> {
    state.agent.get_week_line().await
}

#[tauri::command]
#[specta::specta]
pub async fn list_proposals(state: State<'_, AssistantState>, status: Option<ProposalStatus>) -> AppResult<Vec<Proposal>> {
    state.agent.list_proposals(status).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_proposal(state: State<'_, AssistantState>, proposal_id: Id) -> AppResult<Proposal> {
    state.agent.get_proposal(proposal_id).await
}

/// Apply a pending proposal. For EditNote, `final_md` is the body after the user accepted only some hunks; `None` applies the whole change.
#[tauri::command]
#[specta::specta]
pub async fn apply_proposal(state: State<'_, AssistantState>, proposal_id: Id, final_md: Option<String>) -> AppResult<Proposal> {
    state.agent.apply_proposal(proposal_id, final_md).await
}

#[tauri::command]
#[specta::specta]
pub async fn dismiss_proposal(state: State<'_, AssistantState>, proposal_id: Id) -> AppResult<Proposal> {
    state.agent.dismiss_proposal(proposal_id).await
}

/// The audit log, newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_ai_actions(state: State<'_, AssistantState>, limit: u32, offset: u32) -> AppResult<Vec<AiAction>> {
    state.agent.list_ai_actions(limit, offset).await
}

#[tauri::command]
#[specta::specta]
pub async fn undo_ai_action(state: State<'_, AssistantState>, action_id: Id) -> AppResult<AiAction> {
    state.agent.undo_ai_action(action_id).await
}

/// Undo every applied action of a turn as one step. Returns how many were undone.
#[tauri::command]
#[specta::specta]
pub async fn undo_turn(state: State<'_, AssistantState>, turn_id: Id) -> AppResult<u32> {
    state.agent.undo_turn(turn_id).await
}

async fn status_for(providers: &AppProviders, kind: ProviderKind) -> ProviderStatus {
    let mut settings = providers.ai_settings().await;
    settings.provider = kind;
    let has_key = KeyStore::get(kind).is_some();
    let (ready, problem) = match kind {
        ProviderKind::Anthropic => {
            if has_key { (true, None) } else { (false, Some("Add your Anthropic API key to use Margin.".to_string())) }
        }
        ProviderKind::OpenaiCompat => match settings.base_url.as_deref().filter(|u| !u.trim().is_empty()) {
            None => (false, Some("Add the server address for your OpenAI-compatible provider.".to_string())),
            Some(url) if !has_key && !is_local(url) => (false, Some(format!("Add your {} API key to use Margin.", compat_name(url)))),
            Some(_) => (true, None),
        },
        ProviderKind::Ollama => {
            // Ready only when Ollama answers and has the chosen model.
            let manager = margin_llm::ollama::OllamaManager::new(Arc::new(NullSink));
            let root = margin_llm::ollama::root_url(settings.base_url.as_deref());
            match manager.installed(&root).await {
                Err(_) => (false, Some("Ollama is not running yet. Set it up below.".to_string())),
                Ok(tags) if !tags.iter().any(|t| t == &settings.model || *t == format!("{}:latest", settings.model)) => {
                    (false, Some("Download a local model below, then choose it.".to_string()))
                }
                Ok(_) => (true, None),
            }
        }
        ProviderKind::ClaudeCode => {
            let s = claude_code::detect(None).await;
            match (s.installed, s.signed_in) {
                (false, _) => (false, Some("Claude Code is not installed.".to_string())),
                (true, false) => (false, Some("Claude Code is not signed in. Run `claude` once in a terminal to sign in.".to_string())),
                (true, true) => (true, None),
            }
        }
    };
    ProviderStatus { kind, has_key, ready, problem }
}

/// Status of the provider chosen in settings.
#[tauri::command]
#[specta::specta]
pub async fn get_provider_status(state: State<'_, AssistantState>) -> AppResult<ProviderStatus> {
    let kind = state.providers.ai_settings().await.provider;
    Ok(status_for(&state.providers, kind).await)
}

/// Store an API key in the OS keychain and verify it.
#[tauri::command]
#[specta::specta]
pub async fn set_provider_key(state: State<'_, AssistantState>, kind: ProviderKind, key: String) -> AppResult<ProviderStatus> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::invalid("Paste a key first."));
    }
    KeyStore::set(kind, &key)?;
    let mut settings = state.providers.ai_settings().await;
    settings.provider = kind;
    let mut status = status_for(&state.providers, kind).await;
    if let Ok(provider) = state.providers.build(&settings) {
        match tokio::time::timeout(std::time::Duration::from_secs(15), provider.verify()).await {
            Ok(Ok(())) => {}
            Ok(Err(e @ LlmError::Auth { .. })) => {
                // A rejected key is not kept.
                KeyStore::clear(kind)?;
                return Err(e.into());
            }
            Ok(Err(e)) => status.problem = Some(format!("Saved. {}", e)),
            Err(_) => status.problem = Some("Saved. Margin could not check the key just now.".into()),
        }
    }
    Ok(status)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_provider_key(kind: ProviderKind) -> AppResult<()> {
    KeyStore::clear(kind)
}

#[tauri::command]
#[specta::specta]
pub async fn detect_claude_code() -> AppResult<ClaudeCodeStatus> {
    Ok(claude_code::detect(None).await)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::KeyCache;

    /// A found key is read once and then served from memory.
    #[test]
    fn reads_a_key_once() {
        let cache = KeyCache::new();
        let reads = Cell::new(0);
        let load = || {
            reads.set(reads.get() + 1);
            Ok(Some("sk-test".to_string()))
        };
        assert_eq!(cache.get_or_load("anthropic", load), Some("sk-test".into()));
        assert_eq!(cache.get_or_load("anthropic", load), Some("sk-test".into()));
        assert_eq!(reads.get(), 1);
    }

    /// A confirmed missing entry is cached too, so a keyless launch doesn't keep asking.
    #[test]
    fn caches_a_missing_entry() {
        let cache = KeyCache::new();
        let reads = Cell::new(0);
        let load = || {
            reads.set(reads.get() + 1);
            Ok(None)
        };
        assert_eq!(cache.get_or_load("anthropic", load), None);
        assert_eq!(cache.get_or_load("anthropic", load), None);
        assert_eq!(reads.get(), 1);
    }

    /// A failed read (denied, locked) is retried on next use instead of sticking.
    #[test]
    fn retries_after_a_failed_read() {
        let cache = KeyCache::new();
        assert_eq!(cache.get_or_load("anthropic", || Err(())), None);
        assert_eq!(cache.get_or_load("anthropic", || Ok(Some("sk-test".into()))), Some("sk-test".into()));
    }

    /// A blank key counts as no key.
    #[test]
    fn treats_a_blank_key_as_missing() {
        let cache = KeyCache::new();
        assert_eq!(cache.get_or_load("anthropic", || Ok(Some("  ".into()))), None);
    }

    /// Writes from Settings replace the cached answer without a keychain read.
    #[test]
    fn put_overrides_the_cached_answer() {
        let cache = KeyCache::new();
        assert_eq!(cache.get_or_load("anthropic", || Ok(None)), None);
        cache.put("anthropic", Some("sk-new".into()));
        assert_eq!(cache.get_or_load("anthropic", || panic!("should not read")), Some("sk-new".into()));
        cache.put("anthropic", None);
        assert_eq!(cache.get_or_load("anthropic", || panic!("should not read")), None);
    }

    /// Accounts are cached independently.
    #[test]
    fn keeps_accounts_apart() {
        let cache = KeyCache::new();
        cache.put("anthropic", Some("sk-a".into()));
        assert_eq!(cache.get_or_load("openai_compat", || Ok(Some("sk-o".into()))), Some("sk-o".into()));
        assert_eq!(cache.get_or_load("anthropic", || panic!("should not read")), Some("sk-a".into()));
    }
}
