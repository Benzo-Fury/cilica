# margin-mcp

Margin's tool registry over MCP (stdio, newline-delimited JSON-RPC: `initialize`, `ping`, `tools/list`, `tools/call`) for the Claude Code bridge. Calls go through the agent's `ToolRunner`, so tiers, proposals, limits and the audit log are identical to in-app chat.

Launched as `margin-desktop --mcp --db <path> [--thread <id>] [--turn <id>] [--task-run <id>]` (see `main.rs`); it opens the same SQLite database, and the app picks up the audit rows and proposals it writes.

Must not depend on tauri. Test with `cargo test -p margin-mcp`.
