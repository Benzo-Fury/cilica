# Cilica Server: build plan

How to build the Cilica server and adapt the desktop app to it. This is a high-level plan: it fixes the stack, the shape of each system and the order of work, and leaves file layout and code structure to whoever builds it.

## 1. Goals

The server adds what a single computer can't do on its own:

1. **Sync** across devices, including voiceprints, and access from anywhere through the **web editor** at cilica.si (plan holders only).
2. **Encrypted backup** of everything.
3. **Sharing and realtime collaboration**: a Share button with "Anyone with the link" or named people. People you share with don't need to pay.
4. **Cloud transcription** through Cilica (forwarded to Soniox or whichever provider is chosen).
5. **Cilica's AI**, billed by usage or as an add-on, not part of the base plan.
6. **Remote scheduled tasks** that run while the user's computer is off.
7. A **built-in admin dashboard** for whoever runs the server.
8. A **management API** so other services can create users, manage organisations and set plans without changing the server.

These rules hold throughout:

- **Local-first stays.** The app works fully without a server. The local database stays the source of truth on each device, and the server is a sync peer and a service provider, never a requirement.
- **End-to-end encrypted by default.** The server stores ciphertext. Content keys exist only on people's devices. The only exceptions are features the user switches on that need the server to read something (cloud AI, cloud transcription, remote tasks). Those read in memory for the one job and never store plaintext. Whatever the server keeps for itself that is secret (provider keys, webhook secrets, two-factor secrets) is encrypted at rest too.
- **One codebase, every scale.** Cilica Cloud and a self-hosted server run the **exact same build**, unmodified. Cilica Cloud has to serve a large and growing user base. A self-hoster has to be able to run it for one person on a small machine. Every design choice has to work at both ends (§15).
- **No Cilica-only code in the server.** Anything specific to Cilica's business, payments above all, lives in separate services that talk to the server through its management API (§14). The server only knows about generic plans and entitlements.
- **Source-available, not open source.** The server is under the Cilica Server License: free for personal use and organisations of up to 10 people, paid for bigger organisations, and never resold as a hosted service. It lives in its own repository.

## 2. Stack

### Bun first

The server uses what Bun ships with before reaching for a package. Fewer dependencies means a smaller attack surface, fewer supply-chain risks, faster installs and a smaller single binary.

| Need | Bun built-in | Replaces |
| --- | --- | --- |
| HTTP server, WebSockets and topic pub/sub | `Bun.serve` (routes, `websocket`, `server.publish`) | ws, socket.io |
| PostgreSQL client | `Bun.sql` (Drizzle has a Bun SQL driver) | pg, postgres.js |
| Valkey/Redis client | `Bun.redis` | ioredis, node-redis |
| S3-compatible storage, local files | `Bun.s3`, `Bun.file` | AWS SDK |
| Password hashing | `Bun.password` (Argon2id), plugged into Better Auth's hash and verify hooks | bcrypt, argon2 |
| Hashes, HMAC, random tokens, AES-GCM, Ed25519 verify | Web Crypto (`crypto.subtle`, `crypto.getRandomValues`) and `Bun.CryptoHasher` | libsodium, node-forge |
| Bundling the admin dashboard | Bun's bundler and HTML imports | Vite, webpack |
| Single binary with the dashboard embedded | `bun build --compile` | pkg, Docker-only installs |
| Tests | `bun test` | Jest, Vitest |
| Scripts, `.env` loading | `Bun.$`, built-in `.env` support | zx, dotenv |
| Dependency audit | `bun audit` in CI | |

**Why the server needs no crypto library.** End-to-end encryption happens in the clients, in the Rust core (§2, Rust shared core). The server only stores and relays ciphertext. Its own cryptography is ordinary: hashing passwords and API keys, signing webhooks, generating tokens, and encrypting its own secrets at rest with AES-256-GCM under a master key. Web Crypto and `Bun.password` cover all of it. The one place that opens task keys is the Rust task runner (§12), which uses the same Rust crypto core as the app.

**The dependency rule.** A new package needs a stated reason that a built-in can't meet. Versions are pinned, the lockfile is committed and `bun audit` runs in CI. Check each built-in above against the Bun version the project pins, since a few are recent.

### The full stack

| Concern | Choice | Notes |
| --- | --- | --- |
| Runtime and package manager | **Bun** | See above |
| Language | TypeScript (strict) | |
| HTTP framework | **Hono** on `Bun.serve` | Small and dependency-free. It gives middleware, request validation and **OpenAPI generation** for the management API. Better Auth mounts into it directly. WebSockets use `Bun.serve` directly |
| Validation | **Zod** | Better Auth already depends on it, so it adds nothing. One schema drives validation and the OpenAPI document |
| Auth | **Better Auth** | Plugins in §5 |
| Database | **PostgreSQL** | The only required service, for Cloud and self-host alike |
| ORM and migrations | **Drizzle** (+ drizzle-kit) on `Bun.sql` | Better Auth's Drizzle adapter keeps auth tables in the same schema and migrations |
| Background jobs | A small **Postgres-backed queue** built in the server on `Bun.sql` (`FOR UPDATE SKIP LOCKED`) | Replaces pg-boss and its `pg` dependency. Delayed jobs, retries with backoff, cron schedules, per-account fairness |
| Realtime | `Bun.serve` WebSockets | In-process fan-out on one node. **Valkey** pub/sub through `Bun.redis` when there's more than one node |
| Cache and rate limits | In memory on one node, **Valkey** when there's more than one | Also used as Better Auth's secondary storage for sessions at scale |
| Blob storage | **S3-compatible** through `Bun.s3` (Cloudflare R2 or similar for Cloud), or **local disk** for self-host | One storage interface, two drivers |
| Email | SMTP, or an HTTP provider (Resend, Postmark) through `fetch` | Bun has no SMTP client, so SMTP is the one place a small mail package is allowed |
| Wire protocol | **Protobuf** (Buf, `@bufbuild/protobuf`) over HTTPS and binary WebSocket frames | One schema generates TypeScript types for the server and web client and Rust types for the desktop app. Ciphertext is binary, so binary framing saves base64 overhead. The management API is plain JSON with an OpenAPI document instead, since other services call it |
| Client crypto | libsodium primitives, implemented in the Rust core | X25519 for key exchange and sealed boxes, Ed25519 for signatures, XChaCha20-Poly1305 for content, Argon2id for passphrases, BLAKE2b for hashes |
| CRDT | **Yjs** (Rust: `yrs`) | The editor is TipTap, and TipTap's collaboration support is built on Yjs. `yrs` lets the Rust side (assistant, Markdown, import) read and write the same documents |
| Admin dashboard | **React**, bundled by Bun and embedded in the server | §13 |
| Observability | OpenTelemetry traces and metrics, structured logs | Content, keys and tokens are never logged. That rule gets a test |
| Packaging | One build: a Docker image, a compose file (server and Postgres), and a single binary | Configured by environment variables, the admin dashboard, or both. Includes an admin CLI |

**Rust shared core.** Crypto, the CRDT note model and Markdown conversion are written once in Rust. The desktop app uses them natively, and the web editor uses them compiled to **WebAssembly**. The TypeScript server never needs to decrypt or parse notes, except in the remote task runner (§12), which is a Rust worker. That means one implementation of the security-critical code and one Markdown conversion, so a note looks the same everywhere.

## 3. System overview

```
 Desktop app (Tauri)                      Web editor (cilica.si/app)
 ├─ local SQLite (source of truth)        ├─ IndexedDB (local cache)
 ├─ crypto core (Rust)                    ├─ crypto core (WASM)
 ├─ sync engine (Rust)                    ├─ sync engine (WASM / TS)
 └─ keychain: session token, device keys  └─ non-extractable keys in IndexedDB
            │  HTTPS + WebSocket (protobuf)          │
            └──────────────────┬─────────────────────┘
                               │                    Other services (e.g. Cilica's
                               │                    billing service, Team portal)
                               │                          │ Management API (JSON, OpenAPI)
                               │                          │ ▲ signed webhooks
                     Cilica Server (Bun + Hono)           │ │
 ┌───────────┬──────────┬──────────┬──────────┬───────────┴─┴──┬─────────────┐
 │ Auth      │ Keys and │ Sync and │ Sharing  │ Management API │ Admin       │
 │ (Better   │ devices  │ realtime │ and ACL  │ + webhooks     │ dashboard   │
 │ Auth)     │          │ relay    │          │                │             │
 ├───────────┴──────────┼──────────┴──────────┼────────────────┴─────────────┤
 │ Transcription proxy  │ AI gateway + metering│ Plans and entitlements      │
 ├──────────────────────┴─────────────────────┴──────────────────────────────┤
 │ Job queue · Capabilities · Audit log                                      │
 └──────────────┬─────────────────────┬──────────────────┬──────────────────┬┘
            PostgreSQL        Valkey (optional)   Blob storage (S3 / disk)  Task runner
                                                                            (Rust worker)
```

The same binary runs in different **roles** (§15): all of them in one process for a self-hoster, or separate fleets of API, realtime, worker and task-runner nodes for Cilica Cloud.

## 4. Servers, capabilities and onboarding

### Capabilities

Every server publishes a public **capabilities document** at a well-known address:

- server name, version and **protocol version** (the client refuses a server it can't speak to and says why)
- the sign-in methods it offers (email and password, magic link, passkeys, which social providers) and its sign-up policy (open, invite only, or closed)
- the features it offers: sync, backup, web editor, sharing, link sharing, collaboration, cloud transcription (and which provider), AI (which models), remote tasks
- the plans it offers, by name and description, if any
- optional **operator links** set by the admin: where to upgrade, where to manage billing, the privacy page, support. Cilica Cloud points these at Cilica's billing service. A self-hosted server usually leaves them empty, and the app then shows no upgrade buttons at all.
- limits: largest upload, storage quota policy

Once signed in, the client also fetches the person's **entitlements**: which of those features their account can use right now, with their quotas and usage. The client builds its UI from capabilities plus entitlements, never from a hardcoded idea of what a plan includes. Entitlement changes are pushed over the WebSocket, so an upgrade shows up in the app within seconds.

### Choosing a mode in onboarding

Onboarding gains one step, "How do you want to use Cilica?":

1. **Just this computer** (the default). Nothing changes from today.
2. **Sign in to Cilica.** Uses Cilica Cloud. After sign-in, the app offers the plan's defaults: turn on sync, use Cilica Cloud for transcription (while showing that local Whisper stays available), and use Cilica's AI if the person has it. Each default is a choice, not a silent switch.
3. **Use my own server.** The person enters an address, and the app fetches its capabilities, shows the server name and what it offers, then signs in.

Settings gets an **Account and sync** page with the server, the account, the plan and usage, devices, sign out, change server (moving is an export and import, see §14), and the recovery key. Feature pages (Transcription, AI, Scheduled tasks) gain a server-backed option wherever the server offers one.

A device is connected to **one server at a time**. Multiple accounts are out of scope.

## 5. Accounts and authentication (Better Auth)

**Plugins and options**

- Email and password, **magic link**, **passkeys**, and **two-factor** (TOTP). Passwords are hashed with `Bun.password` (Argon2id).
- Social sign-in (Google, Apple, GitHub) on Cilica Cloud. Self-hosters set up their own or turn it off.
- **Device authorization** for the desktop app (below).
- **Bearer** tokens for desktop and API calls, and **JWT** for short-lived WebSocket and service tokens that the realtime and transcription paths check without a database lookup.
- **Organization**, for teams (organisations over 10 people, per the licence).
- **Admin**, for roles (owner, admin, support, billing), suspending accounts and session control. It backs the admin dashboard (§13).
- **API key**, for the management API's service accounts (§14).
- **OIDC provider**, so the server can act as "Sign in with Cilica" for other sites run by the same operator, such as Cilica's billing site. A self-hoster can use it for their own portals too.
- **Anonymous**, so a person who opens an "Anyone with the link" note that allows editing can join with a display name and no account.
- **Secondary storage** in Valkey when there's more than one node, so session checks don't all hit Postgres.

There is no payment plugin in the server. Payments live outside it (§14).

**Desktop sign-in.** The app starts a device-authorization request, opens the server's sign-in page in the system browser with the code filled in, and waits. The person signs in there, with passkeys and social providers working as usual, and approves "Sign in to Cilica on Neo's MacBook". The app receives a session token and stores it in the OS keychain. It never appears in SQLite, logs or IPC payloads. This flow works the same for self-hosted servers and needs no deep-link registration. A `cilica://` deep link can be added later as a shortcut.

**Signing in is not unlocking.** The account password goes to the server, so it must never be able to decrypt anything. Encryption keys are separate (§6). Signing in on a new device gets you an account. Unlocking your notes there takes an approval from another device or your recovery key.

**Account lifecycle:** email verification, password reset (which never touches encryption keys), session list and revocation, account deletion (deletes everything on the server after a short grace period), and data export.

## 6. Encryption and keys

### Key hierarchy

- **Account keys.** Each account has an identity key pair (Ed25519 for signing, X25519 for encryption), created on the first device. Its public half is published in the server's key directory.
- **Device keys.** Each device has its own key pair. The account's private keys are kept on each device, wrapped for that device and stored in the keychain (desktop) or as non-extractable WebCrypto keys in IndexedDB (web).
- **Space keys.** A **space** is a set of content shared by the same people. Each person has a **personal space** for everything that isn't shared, and each shared note (or shared category, later) is its own space. A space has a symmetric key, versioned by **epoch**. The key is wrapped for each member's public key, and the server stores only the wrapped copies.
- **Content.** Every document update, record and blob is encrypted with its space's current epoch key, and its header carries the space and epoch. Blobs get a per-blob key that is wrapped with the space key, so a large file never has to be encrypted again when a key changes.

Keys are **always created on people's devices**. Nothing on the server side, including the admin dashboard and the management API, can create, read or reset them.

### Devices and recovery

- **Adding a device:** the new device shows a short code. An existing device shows "Allow Neo's iPad?" with the same code, and when approved it sends the account keys over the server, encrypted to the new device's key. The server only relays.
- **Recovery key:** created at encryption setup and shown once ("Save this somewhere safe. Without it and without your devices, your notes can't be recovered."). The server stores the account keys wrapped by it. Entering it on a new device unlocks the account.
- **Optional vault passphrase:** lets the web editor unlock without another device. The account keys are wrapped with an Argon2id-derived key and stored on the server. Settings warns that it should be long, because the server holds something that could be guessed offline.
- **Removing a device** revokes its session, deletes its wrapped keys and rotates the personal space key. Old content stays readable to remaining devices. A device that is gone can no longer get new updates.
- **Losing everything** (every device, the recovery key and the passphrase) means the encrypted data can't be read. The server can still delete the account. Onboarding says this plainly, and so does the admin dashboard when support looks at an account.

### Trust

- Every update and every key-wrap is **signed** by its author's device key, and clients reject anything that isn't signed by a current member. The server can withhold or replay data, but it can't forge it.
- The key directory is the weak point: a malicious server could hand out a fake public key when someone shares with a person. Clients pin the keys they have already seen and warn when a contact's key changes, and a person's page in sharing can show a **safety code** to compare. Full key transparency is a later option.

### The server's own secrets

Things the server must be able to use itself can't be end-to-end encrypted, but they are still encrypted at rest: provider API keys, SMTP credentials, webhook signing secrets, two-factor secrets, OAuth tokens and service-account key material. They use envelope encryption (AES-256-GCM through Web Crypto) under a **master key** from the environment or a KMS, with rotation. API keys and tokens are stored only as hashes. Postgres, Valkey and blob storage run on encrypted volumes, and every connection between them uses TLS on Cilica Cloud.

### What the server can see

The server sees accounts, emails, device names, space membership, document and blob IDs, sizes, timestamps and access patterns. It can't see note titles, bodies, category names, people, voiceprints, transcripts, attachments or settings. The privacy page lists exactly this, and so does the admin dashboard, so operators know what they can and can't help with.

## 7. The app's data model: notes become CRDTs

This is the largest change to the client, and the one to prove first.

- Each note body becomes a **Yjs document** in ProseMirror form, the shape TipTap's collaboration extension uses. The editor binds to the Yjs document instead of setting content.
- The **Markdown** stays as a derived copy, updated from the document and kept in SQLite for search, the graph, links, export and the assistant's reading. Conversion runs in Rust through `yrs`, so the desktop app, the web editor (through WASM) and the task runner give identical output.
- The **assistant's edits** become CRDT operations, applied to the document as transactions by the Rust side. They then merge with someone else's live typing instead of overwriting it. Proposals and diffs keep working against the derived Markdown.
- **Everything else** (categories, people and voiceprints, transcripts and speaker assignments, tasks, links, attachments metadata, settings meant to follow the person) syncs as **encrypted records** (§8), not as rich-text documents.
- **Migration:** on the first launch of the new version, each note's Markdown converts into a Yjs document, locally, once, with a backup taken first. A round-trip test runs every existing note through Markdown, then Yjs, then back to Markdown and reports any note that changed.

**Spike first:** prove that Markdown, ProseMirror JSON and Yjs convert cleanly in both directions in Rust and in JS, with every node and mark the editor supports (tables, task lists, images, code blocks, links, speaker lines, sub- and superscript).

## 8. Sync and backup

Sync and backup are the same system. Once a device has synced, its data is on the server encrypted, and backup is just sync with no other devices.

### Two kinds of synced data

1. **Documents** (note bodies, Yjs). The server keeps, for each document, an append-only **log of encrypted updates** with a server-assigned sequence number, plus the latest **encrypted snapshot**. Clients occasionally compact: they merge the log into a new snapshot, upload it, and the server drops the log entries it covers. This follows the pattern used by Secsync and similar encrypted-CRDT systems. Snapshots and large updates live in blob storage, and Postgres keeps only small updates and pointers (§15).
2. **Records** (everything else). Each record is an encrypted blob with a collection, an ID, a version and a hybrid logical clock. Conflicts are resolved per field, last write wins, inside the encrypted payload (the client decides). Deletes are tombstones, kept long enough for every device to see them.

Blobs (attachments, images, and recordings if enabled) are uploaded encrypted, straight to storage through pre-signed URLs, and are referenced by records.

### The client's sync engine

- A new Rust component on the desktop app (with a WASM or TS counterpart on the web) with an **outbox** of local changes, a **cursor** per space for what it has pulled, and a persistent WebSocket for live changes. It retries with backoff and jitter, and keeps working offline.
- Local writes go to SQLite first, as now, and then to the outbox. Nothing in the UI waits on the network.
- First sync on a device that already has data uploads everything. First sync on a new device downloads and decrypts everything, with progress shown.
- A sync status lives in one quiet place in the UI (synced, syncing, offline, or a problem with what to do next), not as a popup.
- The client obeys the server's **back-off signals** (retry-after on rate limits and during incidents), so a server under strain isn't hit harder by reconnecting clients.

### What syncs

Notes, categories, links, attachments, people and **voiceprints**, transcripts and speaker edits, scheduled tasks and their proposals, AI threads, and the settings that belong to the person rather than the computer (summary preferences, but not audio devices, the model download or window positions). **Recordings** are off by default ("Back up recordings" in Settings) because they are large and count toward storage. Local model files never sync.

### Quotas

Storage is counted on ciphertext size, per account, with a plan limit. Shared spaces count toward their owner. At the limit, sync of new blobs pauses with a clear message, and text keeps syncing.

## 9. Sharing and realtime collaboration

### The share dialog

It follows the pattern people know from Claude artifacts and Google Docs:

- **General access:** *Only people added* or *Anyone with the link*, and for the link: *can view* or *can edit* (*can comment* once comments exist).
- **People:** add by email, each with a role (viewer or editor; the owner can transfer ownership), and remove.
- **Copy link.**

### How it works

- Sharing a note moves it into a **new space**. The sharer's client creates a space key, re-encrypts the note's snapshot under it, and wraps the key for every person added.
- **Adding someone with an account:** the client wraps the space key for their published key, and they see the note under *Shared with me* on all their devices.
- **Adding someone without an account:** the invite email's link carries a one-time **invite secret in the URL fragment** (never sent to the server). That secret unlocks a copy of the space key held by the server. When the person signs up and accepts, their client re-wraps the key for their own account and the invite copy is deleted. The sharer doesn't need to be online.
- **Anyone with the link:** the link is `cilica.si/s/<id>#<key>`. The fragment holds the space key, so the server never sees it. Anyone with the link opens the note in the web editor or the app. The server still checks the link's role, and it only relays updates from editors.
- **Roles** are enforced twice: the server's ACL refuses writes from viewers, and clients verify that each update is signed by an editor.
- **Removing someone, or turning off the link,** rotates the space key to a new epoch. They keep what they already had, which is the honest limit of any system like this, and they get nothing new.
- **Guests don't pay.** A free account can open, edit and keep what's shared with it, and nothing else. The owner's plan covers the shared space.

### Realtime

- Each open document joins a **room** on the server over the WebSocket. Updates are encrypted and signed, sent to the room, appended to the log, and passed to the other members.
- **Presence** (who's here, cursors, selections) uses Yjs awareness, encrypted the same way and never stored.
- The editor shows collaborators' cursors in their colours, avatars in the note header, and "Sam is editing" where it's quiet but noticeable. Motion and styling follow the design system.

### What a shared note includes

The body, attachments, and for a meeting note the summary and transcript. The recording is only included if the sharer chooses to. **Saved people and voiceprints are never shared**: they stay personal. Speaker names show as plain text to others. Links from a shared note to notes that aren't shared show as unavailable to others. The recipient's own Cilica can read a shared note, and can edit it only if they are an editor.

## 10. Plans and entitlements

The server knows **plans** and **entitlements**, and nothing about money.

- A **plan** is a named bundle: which features are on, and the quotas that go with them (storage, transcription hours, AI tokens or credits, seats, remote tasks). Plans are defined in the admin dashboard or through the management API, so Cilica can add or change one without a release.
- An **entitlement** gives an account or organisation a plan, with optional **overrides** (extra storage, a trial feature), a **valid-until** date and a **grace period**. Add-ons, such as Cilica's AI, are entitlements stacked on top of the base plan.
- Whoever sets the entitlement keeps it current. On Cilica Cloud, that's the billing service extending valid-until each billing period. If it ever fails to, accounts fall into the grace period rather than losing access at once.
- **Metering** is one ledger for transcription seconds, AI tokens and storage, kept per account and per period. Quotas are checked against it, the app shows it, and it's exposed through the management API so a billing service can charge for it.
- A **self-hosted** server ships with one default plan that has every configured feature on, assigned to everyone. The admin can add plans and limits if they want them, and doesn't have to.
- **Losing a plan** never deletes data: sync becomes read-only (download and export still work) for a set period, then the server data is deleted after clear warnings. Local data is untouched.

## 11. Server-side services

### Cloud transcription

- A new engine in the existing transcriber abstraction: **Cilica Cloud**. The app streams audio over an authenticated WebSocket. The server checks the entitlement and quota, opens a stream to the provider with the server's key, sends results back as they arrive, and meters the seconds.
- Audio and text pass through memory only: nothing is written, cached or logged. Choosing it shows the same confirmation the other cloud engines show, naming the provider behind it.
- Self-hosted servers set their own provider key in the admin dashboard, or leave transcription off.
- Speaker labelling by the service works as it does with the person's own Soniox key. Local voiceprints and recognition stay on the device.
- Concurrent streams are capped per account, and the server stops taking new streams before the provider's own limits are hit.

### AI gateway

- The **agent keeps running on the client**, exactly as now, with its tools working on local notes. The server is only a **model gateway**: a new provider, **Cilica**, alongside Anthropic, Ollama and OpenAI-compatible.
- The gateway speaks an Anthropic-compatible Messages API, so the client's provider code barely changes. It checks the entitlement, applies limits, forwards to the upstream model with the provider's zero-data-retention terms, streams back, and meters input and output tokens per request.
- Nothing from prompts or responses is stored. The usage record holds the model, tokens, cost and time, and no content.
- The AI settings page shows usage this period, and a soft warning comes before the limit, never a surprise cutoff in the middle of a reply.
- Upstream providers, models, keys and limits are set in the admin dashboard, the same way on Cloud and self-hosted.

## 12. Remote scheduled tasks

Tasks keep their current behaviour, and each one gains **"Run on: this computer / Cilica server"**.

- **Turning on server runs** shows what it means: "The server will be able to read *Work* notes while this task runs. Nothing it reads is kept." The client then creates a **task key**: the keys for exactly the task's scope (for example one category, or all notes), wrapped for the task runner's public key. Only the task runner holds the matching private key, sealed under the server's master key.
- **Scope** is enforced by what the key opens, not just by a setting.
- **Running:** the job queue starts the job on schedule, and a **Rust task-runner worker** (built from the app's agent, store and Markdown crates, running against an in-memory database) does the work. It pulls the scope's encrypted documents, decrypts them in memory, runs the agent through the AI gateway with the task's prompt and the same tools, and writes the **proposals** as encrypted records for the owner. Proposals still wait for a person, as now. Then it throws everything away.
- **Turning the task off, deleting it, or changing its scope** destroys the task key, and the next run needs a new one.
- If the person's computer is also on, the task runs on the server only, so it never runs twice.
- AI usage from tasks counts toward the account's AI usage.
- Schedules are spread with a little jitter, so thousands of "every day at 9:00" tasks don't all start in the same second (§15).

## 13. Admin dashboard

A professional admin dashboard is part of the server, not an add-on. It's what Cilica's team uses to run Cilica Cloud, and what a self-hoster uses to run theirs: the same pages, with Cloud simply having more data in them.

### How it's built

- A **React** app inside the server repository, bundled by Bun and **embedded in the binary**, so it works with nothing else to deploy. It's served at a configurable path, can be turned off, and can be bound to a separate internal port so Cilica Cloud can keep it off the public internet.
- It follows Cilica's **design system** (tokens, type, motion and voice), adapted for dense operational screens: tables, filters, detail panes, charts. Desktop first, usable on a tablet.
- It is a client of the **same service layer as the management API** (§14). Every action in the dashboard exists in the API, and every action in the API shows up in the dashboard and the audit log. Nothing is dashboard-only.
- Settings can come from environment variables or the dashboard. A setting fixed by the environment shows as locked, with a note saying so, which suits operators who manage config as code.

### Pages

- **Overview:** health, active accounts, sign-ups, storage, connections, queue depth, error rates, usage this period, and anything needing attention.
- **Accounts:** search, then an account's profile, plan and entitlements, usage, storage, devices, sessions, organisations and audit history. Actions: suspend, restore, sign out everywhere, reset two-factor, resend verification, change plan or overrides, export metadata, delete. Content is never shown, because the server doesn't have it.
- **Organisations:** members, roles, seats, plan, usage.
- **Plans:** define plans, features, quotas and add-ons (§10).
- **Usage:** metering by feature, account and period, with export.
- **Sharing and reports:** shared links and spaces by metadata, the abuse report queue, and revoking a reported link.
- **Jobs:** queue depth, failed jobs with retry, scheduled tasks and task-runner status.
- **Integrations:** service accounts and API keys, webhooks with their delivery log and replay (§14).
- **Settings:** sign-up policy (open, invite only, allowed email domains, closed), sign-in methods, email, storage, transcription and AI providers with their models and limits, operator links, server name and branding.
- **Admins and roles:** who can do what (owner, admin, support, billing).
- **Audit log:** every admin and API action, searchable and exportable.
- **System:** version, protocol version, migrations, background health, backup status, and the licence notice (user count next to the 10-person limit for self-hosters, and where to get a licence).

### Security

- Admin access needs a **passkey or two-factor**. Destructive actions (deleting an account, rotating secrets) ask to confirm identity again.
- Roles limit what each admin sees and does. *Support* is read-only on accounts and can't change plans. *Billing* can change plans and see usage, and nothing else.
- **No impersonation.** It couldn't show content anyway, and it would be a risk. Support works from metadata and the person's own description.
- Admin sessions are short. Every action goes into an append-only **audit log** with who, what, when, from where, and the before and after values.
- First run on a fresh self-hosted server opens a **setup page** that creates the owner account, protected by a one-time code printed in the server's log.

## 14. Management API, webhooks and outside services

### The management API

A secure, versioned API that lets other services run the server without changing it. It's how Cilica's own billing service works, and anyone else can use it the same way.

- **Service accounts** each have one or more **API keys** with **scopes**, for example: accounts read or write, organisations read or write, entitlements write, usage read, sharing moderation, webhooks manage. Keys are shown once, stored as hashes, can expire, can be restricted to IP ranges, and can be rotated with an overlap so nothing breaks.
- **Requests can be signed** (HMAC with a timestamp) on top of the key, which protects against replay and leaked logs. Running it behind mutual TLS at the proxy is documented for operators who want it.
- **What it can do:** create, find, update, suspend and delete accounts; send sign-up, verification and sign-in emails; create and manage organisations, members and seats; define plans and set entitlements; read usage and metering; moderate shared links; manage webhooks.
- **What it can't do:** anything with content or keys. There is no endpoint that could return a note or reset someone's encryption, because the server has neither. Creating an account through the API makes the account, and the person's own device creates their keys on first sign-in.
- **Built for machines:** JSON, an **OpenAPI document** generated from the same schemas that validate requests, cursor pagination, **idempotency keys** on every write so retries are safe, consistent error codes, per-key rate limits, and a stable version prefix with documented deprecations.
- Every call is written to the audit log with the service account and key that made it.

### Webhooks

The server tells other services what happened, so they don't have to poll.

- Events such as: account created, verified, suspended or deleted; organisation member added or removed; entitlement changed or expiring; usage threshold reached; usage period closed; link reported.
- Each delivery is **signed** (HMAC with a timestamp, following the Standard Webhooks format), retried with backoff through the job queue, and logged. Failed deliveries can be replayed from the dashboard or the API.
- Payloads carry IDs and metadata, never content.

### How Cilica's billing service uses it

Cilica runs a **separate billing service** (its own repository, its own deploy). It handles Stripe Checkout, the customer portal, invoices and Stripe's webhooks, and it changes the server only through the management API. The server repository contains **no payment code**, and Cilica Cloud runs the public server build exactly as self-hosters do.

- **Upgrade from the app:** the app's Upgrade button opens the server's operator link. The billing site signs the person in with **"Sign in with Cilica"** (the server's OIDC provider), so it knows which account is paying. After payment, the billing service sets the entitlement through the API, and the app updates within seconds over the WebSocket.
- **Buy first, on cilica.si:** the billing service creates the account through the API, which sends a welcome email with a sign-in link. The person signs in to the app and their device creates their keys.
- **Renewals and cancellations:** Stripe's events drive the billing service, which extends valid-until or lets it lapse into the grace period.
- **Usage billing:** the billing service reads metering at the end of each period (or on the usage period closed webhook) and charges AI and extra transcription through Stripe's metered billing.
- **Teams:** the billing service creates the organisation, sets seats, and invites the team's admin.

The same pattern works for anything else Cilica builds later (a team portal, a reseller integration, support tooling), and for self-hosters who want to plug in their own systems.

### Plans on Cilica Cloud

| | Free account | Cilica plan ($4.65/month) | AI | Team | Self-hosted |
| --- | --- | --- | --- | --- | --- |
| Use the app locally | yes | yes | | yes | yes |
| Open and edit what's shared with you | yes | yes | | yes | yes |
| Sync, backup, voiceprint sync | | yes | | yes | yes |
| Web editor | | yes | | yes | yes |
| Share and collaborate | | yes | | yes | yes |
| Cloud transcription | | monthly allowance | | yes | with own key |
| Remote tasks | | yes | | yes | if runner set up |
| Cilica's AI | | | usage or add-on | add-on | with own key |

These are rows of plan data on Cilica Cloud's server, set by Cilica, not rules in the code.

- **Self-hosted:** no billing, everything configured is on. There's no licence server and no phoning home. The licence is a legal agreement, not DRM.
- **Moving between servers:** export everything (decrypted locally, as the app's own export format) and import it into the other server's account. Encrypted data never moves directly between servers.

## 15. Scale: Cilica Cloud and self-hosting from one build

Cilica Cloud has to grow to a large user base without a rewrite, and a self-hoster has to run the same build on one small machine. The design gets there by keeping every node stateless and swapping a few adapters by configuration, not by keeping two versions of anything.

### Roles

The binary runs as one or more **roles**:

- **api:** HTTP, auth, the management API and the admin dashboard
- **realtime:** WebSocket connections, rooms and presence
- **worker:** the job queue (emails, webhooks, compaction, cleanup, metering roll-ups)
- **task runner:** the Rust worker for remote tasks, a separate binary

A self-hoster runs **all** roles in one process, plus Postgres, and the task runner only if they want remote tasks. Cilica Cloud runs each role as its own fleet and scales them separately: realtime on connection count, workers on queue depth, task runners on scheduled load.

### Adapters

| Concern | Single node (self-host default) | Many nodes (Cilica Cloud) |
| --- | --- | --- |
| Realtime fan-out | In-process (`server.publish`) | Valkey pub/sub |
| Cache, rate limits, session lookups | In memory | Valkey |
| Blob storage | Local disk | S3-compatible with a CDN in front |
| Database | One Postgres | Postgres with a connection pooler and read replicas |

Each adapter pair passes the same conformance tests, so the self-hosted path is never the untested one. Postgres is the only service a self-hoster needs.

### What keeps it scaling

- **Stateless nodes.** Apart from open WebSockets, nothing lives in a node's memory that another node needs. Any API node can serve any request, so nodes can be added or removed freely behind a load balancer.
- **Cheap auth on hot paths.** Realtime, sync and the gateways check short-lived JWTs without a database call. Session checks elsewhere go through the cache.
- **Postgres kept lean.** Bulk bytes (snapshots, large updates, blobs) go to blob storage, and Postgres holds metadata, small updates and pointers. Compaction keeps update logs short. The largest tables (update logs, records, metering) are **partitioned**, by hash of space for logs and by time for metering, so they can be maintained and archived piece by piece. Reads that can be slightly stale (capabilities, plan definitions, usage displays) go to replicas or cache. Every query has a timeout.
- **Online migrations.** Schema changes follow expand, migrate, contract, with no long locks, so Cilica Cloud deploys without downtime. Self-hosted servers run migrations on start. Cloud runs them as their own deploy step before new nodes roll out.
- **Bytes skip the API.** Uploads and downloads go straight to storage through pre-signed URLs, and ciphertext is safe to cache at a CDN.
- **Realtime that survives node loss.** The update log is the source of truth and pub/sub is only a fast hint. A client that loses its node reconnects to any other and resumes from its cursor, losing nothing. Connections have heartbeats and backpressure, and nodes drain gracefully on deploy.
- **A fair job queue.** Jobs are claimed with `SKIP LOCKED`, queues are separate by kind, one account can't starve the rest, and scheduled tasks are spread with jitter.
- **Limits everywhere.** Rate limits per IP, account and API key. Concurrency caps on transcription streams and AI requests per account. Circuit breakers on upstream providers, so an outage at one provider becomes a clear error rather than a pile-up. Size limits on every request.
- **Housekeeping as jobs.** Tombstone cleanup, orphaned blob cleanup, expired invites, deleted-account purges and metering roll-ups all run continuously in small batches, never as one large sweep.
- **Tenant isolation.** Every query is scoped by account or space, and a test suite tries to read across accounts through every route.
- **Region-ready.** Cilica Cloud starts in one region. Accounts carry a home region from day one, so moving to several regions later is a deployment change, not a data-model change.

### Self-hosting stays simple

- Minimum: a small VPS (around 1 vCPU and 1 GB of RAM) plus Postgres, or the compose file that bundles both.
- No Valkey, S3, Kubernetes or external queue needed. They're options for when a self-hoster grows.
- Health and readiness endpoints, a backup command, and one-step upgrades.
- The same admin dashboard, with the setup page on first run.

### Observability

OpenTelemetry traces and metrics from every role, with rate, errors and duration per route, queue depth, connection counts, and upstream latency. Cilica Cloud sets **service-level objectives** for sync latency, availability and realtime delivery, with alerts on them. The admin dashboard's overview shows the essentials for operators without a monitoring stack.

## 16. Running it: security and operations

- **Rate limits** on auth, sharing, invites and the management API, and size limits on updates and uploads (§15).
- **Abuse:** content is encrypted, so moderation is limited to metadata and reports. A "Report this link" action on shared pages lets someone send the key along with their report, and the report lands in the admin dashboard. Accounts can be suspended.
- **Logs and traces** never contain content, keys, tokens or emails in plain form. A test sends known marker strings through every path and fails if they show up in any log.
- **Backups** of Postgres and blob storage contain only ciphertext and metadata, and restores are tested on a schedule.
- **Secrets:** the master key comes from the environment or a KMS, never the database, and can be rotated (§6).
- **Region:** pick the hosting region with the privacy page and the transcription provider in mind (EU is the natural fit), and state it.
- **Privacy and GDPR:** data export, account deletion, a list of subprocessors (hosting, Stripe through the billing service, email, transcription provider, model provider), and the exact list of what the server can see (§6).
- **Self-hosting docs:** compose quick start, environment reference, the admin dashboard, backups, upgrades, SMTP, S3, enabling transcription, AI and the task runner, using the management API, and putting it behind a reverse proxy with TLS.
- **Compatibility:** the client protocol and the management API are versioned separately. The server supports the current and previous client protocol versions, and the client tells the person when it or the server needs an update.

## 17. Testing

- **Server:** `bun test` against a real Postgres (in a container in CI), covering auth flows, ACL decisions, the sync log and compaction, quotas, plans and entitlements, metering and the job queue.
- **Management API:** contract tests generated from the OpenAPI document, scope checks for every endpoint (a key without a scope is refused), idempotency, request signing and replay protection, and webhook signing and retries.
- **Admin dashboard:** browser tests for the main flows, and role tests that each role sees and does only what it should.
- **Adapters:** every single-node and many-node adapter pair passes the same conformance suite.
- **Crypto core:** test vectors, round trips, tamper detection and signature failures, run on both the native and WASM builds so they agree byte for byte.
- **CRDT and Markdown:** the round-trip suite from §7, plus property tests that merge concurrent edits, including assistant edits mixed with typing.
- **Protocol conformance:** one suite drives the real server through the generated client in Rust and TypeScript, so the desktop app and the web editor can't drift apart.
- **End to end:** two simulated devices and a guest editing the same note through the server, a device going offline and coming back, key rotation after removing a member, recovery on a fresh device, and an upgrade made through the management API reaching the app.
- **Client:** every new crate stays testable without a window, with the sync engine against an in-process server or a mock. UI logic gets vitest tests, as now.
- **Load and failure:** load tests at Cilica Cloud's targets (§19) for sign-ins, sync, realtime rooms, large documents and the job queue, plus killing nodes mid-session to prove clients resume without losing anything.
- **Single-node:** the same end-to-end suite runs against one all-roles process on a small machine, so self-hosting can't quietly break.

## 18. Build order

Each phase ends in something usable and tested. **Every phase ships its own admin dashboard pages, management API endpoints and webhook events** alongside the feature, rather than leaving them to the end.

**Phase 0: spikes (do first, in parallel)**
- CRDT round trip: Markdown, ProseMirror, Yjs and back, in Rust and JS, for every node type (§7).
- The crypto core in Rust, compiled to WASM, with test vectors.
- Better Auth device authorization from the desktop app against a local Bun server, with Drizzle on `Bun.sql`.
- Exit: written results, and a go or change of plan on Yjs and the key design.

**Phase 1: server foundation, accounts and the admin shell**
- The server repository with Bun, Hono, Drizzle, Postgres, Better Auth, roles and adapters, the job queue, the capabilities document, the audit log, the admin CLI and the compose file.
- The admin dashboard shell: first-run setup, admin sign-in with passkeys, roles, Accounts, Settings and System.
- In the client: the onboarding step, Account and sync settings, sign-in through the browser, and the token in the keychain.
- Exit: someone can set up a fresh server, sign in to it from the app, and see their account in the dashboard.

**Phase 2: keys and devices**
- Account, device and space keys, recovery key, adding and removing devices, and the key directory.
- Exit: two devices on one account share the account keys, and a fresh device recovers with the recovery key.

**Phase 3: notes as CRDTs (client only; can run alongside phases 1 and 2)**
- The editor on Yjs, derived Markdown, assistant edits as CRDT operations, and the one-time migration.
- Exit: the app works exactly as before, every existing note round-trips, and nothing touches the network.

**Phase 4: sync and backup**
- The document log and snapshots, encrypted records, blobs, the sync engine, sync status, and storage quotas.
- Exit: two computers stay in sync, offline edits merge, and a new device restores everything, voiceprints included.

**Phase 5: plans, the management API and Cilica's billing service**
- Plans, entitlements and metering in the server. The management API with service accounts, scopes, signing and idempotency, the OpenAPI document, and webhooks. The OIDC provider.
- In a separate repository: Cilica's billing service with Stripe, using only the management API.
- Exit: someone can upgrade from the app, pay, and see sync turn on within seconds, and cancelling behaves as written, all on an unmodified server build.

**Phase 6: cloud transcription and the AI gateway** (small and independent, so this can move earlier to start earning)
- The Cilica Cloud transcriber, the Cilica AI provider and the AI add-on, with providers set up in the dashboard.
- Exit: a meeting transcribes through Cilica Cloud, the assistant runs on Cilica's AI, and both show usage in the app, the dashboard and the API.

**Phase 7: web editor**
- The web backend, WASM core, unlock flows, the capability-trimmed UI, and hardening.
- Exit: a plan holder signs in at cilica.si, unlocks, and edits notes that show up on their desktop.

**Phase 8: sharing and realtime collaboration**
- Spaces per shared note, the share dialog, invites, link sharing, rooms, presence, key rotation, and the report queue.
- Exit: the owner, a free-account invitee and an anonymous link editor all edit one note live, and removing one stops their access to new changes.

**Phase 9: remote scheduled tasks**
- Task keys, the Rust task-runner worker, scheduling with jitter, and proposals written back.
- Exit: a task runs while the computer is off and its proposals wait when the app next opens.

**Phase 10: scale and launch hardening**
- The many-node setup for Cilica Cloud (Valkey, replicas, partitioning, CDN), load and failure tests at the agreed targets, SLOs and alerts.
- Security review (an external one if the budget allows), self-hosting docs, the privacy page, and updates to the spec and architecture docs (the spec currently lists cloud sync and sharing as out of v1).

## 19. Decisions still open

1. **AI pricing:** prepaid credits, a monthly add-on with a cap, or pure usage.
2. **Transcription allowance** in the base plan (hours per month) and the price beyond it.
3. **Storage quota** per plan, and whether recordings count fully.
4. **Vault passphrase:** offer it at launch, or require device approval or the recovery key for the web editor at first.
5. **Comments** on shared notes: a new feature, and needed before *can comment* exists.
6. **Shared categories** (sharing a whole folder as one space) at launch or after.
7. **Hosting provider and region** for Cilica Cloud.
8. **Transcription provider** behind Cilica Cloud (Soniox is the current assumption).
9. **Scale targets** for launch load tests, for example 1 million accounts and 100,000 concurrent connections, so the many-node setup is sized against real numbers.
10. **The billing service's licence:** keep it private, or publish it as a reference for self-hosters who want to charge for their own servers.
11. **Admin dashboard exposure on Cilica Cloud:** internal network or VPN only, or public with passkeys and an IP allowlist.
