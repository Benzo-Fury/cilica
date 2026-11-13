# cilica-setup

Cilica Setup: Cilica's install, update and uninstall window on Windows. A small Tauri program that runs Tauri's own NSIS installer and uninstaller silently under a frameless window drawn by the desktop frontend (`?window=installer`). How it fits together: `docs/architecture.md` › Windows installer. How it is built: `docs/releasing.md` › Windows installer.

- `src/plan.rs`: modes, steps, folders and command lines, without I/O. Tested on every platform.
- `src/system/windows.rs`: the real thing. `src/system/simulated.rs`: the same steps with pauses, for macOS and Linux.
- `src/commands.rs`, `src/job.rs`: the IPC surface (`bun run bindings` writes `apps/desktop/src/ipc/installer/bindings.ts`) and the progress event.
- `build.rs`: builds in the NSIS installer named by `CILICA_SETUP_PAYLOAD`; without it, the copy can only remove Cilica.

Run it anywhere with `bun run setup` (add `-- --uninstall` for removal, `-- --update x.exe --version 9.9.9` for an update) while `bun run dev:web` is up. It must not depend on Cilica's crates beyond `cilica-domain`, and never touches Cilica's database.
