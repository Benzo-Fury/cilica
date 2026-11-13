# Cilica

A private, local-first desktop notes app with an AI assistant and a meeting summariser. You write in Markdown, the assistant can read and edit every note, and calls are recorded without a bot, transcribed on-device and summarised into notes. Tauri 2 (Rust) + React 19.

Read these before writing code:

- `docs/spec.md`: what the app does (behaviour, data model, guardrails, build order).
- `docs/design-system/README.md` and its siblings (`motion.md`, `materials.md`, `sound-and-haptics.md`, `references.md`, `tokens.json`, `components/<Name>/README.md` + `preview.html`): how it looks, moves and speaks. `components/bundle.css` is the reference CSS for every component.
- `docs/architecture.md`: how the pieces fit, who owns which files, and the IPC contract.

## Commands

| What | Command (from the repo root) |
| --- | --- |
| Run the desktop app | `bun run dev` |
| Run the UI alone in a browser (mock backend) | `bun run dev:web` → http://localhost:1420 |
| Typecheck / lint / test the UI | `bun run typecheck`, `bun run --cwd apps/desktop lint`, `bun run --cwd apps/desktop test` |
| Check / test one crate | `cargo check -p cilica-<crate>`, `cargo test -p cilica-<crate>` |
| Regenerate `src/ipc/bindings.ts` | `bun run bindings` |
| Regenerate `tokens.css` from `tokens.json` | `bun run tokens` |
| Regenerate the README's import image (`docs/images/import-apps.svg`) after changing the import catalogue or its logos | `bun run import-apps` |
| Regenerate the disk image background (`src-tauri/dmg/background.tiff`) after changing the setup art or dark tokens (needs Chrome) | `bun run dmg-background` |

Package manager is **bun**. Rust edition 2024. Add dependencies with `cargo add` / `bun add` (never hand-type versions).

## Rules

- **Layering.** `crates/domain` is the shared contract and depends on nothing app-specific. Crates never depend on `tauri`; they talk to the UI through `cilica_domain::EventSink`. `apps/desktop/src-tauri` only wires crates to commands, events, windows and the tray. No `cfg(target_os)` outside `crates/audio`, `crates/detect` and `src-tauri`.
- **IPC is typed.** Commands are declared in `src-tauri/src/commands/*.rs` with `#[tauri::command] #[specta::specta]`, listed in `src-tauri/src/lib.rs`, and reach the UI only through the generated `apps/desktop/src/ipc/bindings.ts`. No hand-written `invoke("…")`. No `i64`/`u64` in IPC types.
- **UI layering.** `src/ui/` holds design-system components and is the only place raw styling lives. `src/features/<name>/` owns its components, hooks and state. UI components never call IPC; feature hooks do, through `src/ipc/`.
- **Tokens only.** No raw hex, px radii or ad-hoc shadows in components: use Tailwind utilities mapped in `src/styles/theme.css` or the `var(--token)` names. `tokens.css` and `bindings.ts` are generated; never edit them.
- **No inner light.** No inset top highlight or inner glow on any surface (`inset 0 1px 0 …`). Glass gets its edge from the 0.5px `glass-edge` and its shadow only (see `materials.md`).
- **Everything that moves, springs.** Use the presets in `docs/design-system/motion.md`. Animate transform and opacity only. Honour Reduce motion and Reduce transparency.
- **Copy.** Sentence case, plain verbs, no emoji, no "Success!", errors say what went wrong and what to do next (see Voice and copy in the design system README). The assistant is "Cilica", never "AI" (except the "AI" settings section).
- **Errors.** Every command returns `AppResult<T>` (`cilica_domain::AppError { code, message }`). Messages are written for people.
- **Tests.** Every crate is testable without a window: stores against in-memory SQLite, audio/STT against fixture WAVs, the agent against a mock provider. UI logic gets vitest tests.
- **Privacy.** Audio and transcripts never leave the machine unless the user picked a cloud engine. API keys live in the OS keychain (`keyring`), never in SQLite, logs or IPC payloads.
- **Git.** Do not commit, push or switch branches unless the user asks.
