# DeACT Architecture

This document is the formal architecture of DeACT. It records the
**decided target architecture** (Rust + Tauri 2 + Svelte + TypeScript +
Tokio + SQLite) and, separately, what the current codebase actually
implements today. The two are never mixed: anything described as a target
is a design decision to be built, not an existing feature. Section 2
states the implementation status precisely.

Decisions in this document are made by ChatGPT (architecture) and the
product owner (requirements). Implementation follows
`docs/DEVELOPMENT_PLAN.md`.

## 1. Decided technology stack

| Area | Decision |
|---|---|
| Desktop framework | Tauri 2 (see §3.3 for the minimum version and §3.5 for version locking) |
| Frontend | Svelte + TypeScript, plain CSS |
| Core language | Rust |
| Async runtime | Tokio, provided by Tauri 2; application background tasks run on it — no second independent Tokio runtime by default (§4) |
| Database | SQLite, WAL mode |
| Database access | Rust SQLite via the existing single-writer worker design (§9.3); migration cost to be evaluated in Phase 1 of the development plan |
| External networking | REST + WebSocket (exchange APIs, webhook signal source). The desktop UI does **not** use REST/WebSocket; it uses Tauri Commands and Events (§4) |
| Telegram | Rust MTProto client; the crate is **not** decided (§5.3) |
| Logging | `tracing` |
| Secret encryption | AES-256-GCM, key derived with Argon2id (§8) |
| Software architecture | Modular monolith, one process |
| Windows web runtime | The system WebView2 Runtime (Tauri 2 renders through WebView2 on Windows; see §3.4) |
| Data storage | `data/` directory beside the executable (§3) |

Explicitly not used: React or any second frontend framework, large
component libraries, extra state-management frameworks, microservices,
Docker, Kubernetes, Redis, Kafka, NATS, or any other infrastructure the
product does not need.

## 2. Current implementation status

The current branch (`feature/windows-app-foundation`, PR #1) contains
the **Phase 1 Tauri 2 foundation** described in
`docs/DEVELOPMENT_PLAN.md`: the eframe/egui foundation has been replaced
by the target stack. It is an engineering base, not the product:

Implemented in Phase 1:

- Tauri 2 application shell (Rust backend in `src-tauri/`) with a
  Svelte + TypeScript frontend: a six-page navigation skeleton
  (Overview, Trading, Copy Config, Signals, Accounts, Settings) in the
  §10 dark terminal style, with the overview page reading a backend
  status snapshot through a Tauri Command.
- Portable data-directory resolution from `current_exe()` with a
  writability probe and no fallback, creating the `data/` layout of
  §3.1 (`config/`, `logs/`, `cache/`, `backups/`, `sessions/telegram/`,
  `keys/`, `webview/`).
- Per-directory `appDirectoriesOverride` installed at runtime through
  `Context::config_mut()` (config → `data/config/`, data → `data/`,
  localData → `data/webview/`, cache → `data/cache/`, log →
  `data/logs/`), and the main window created in Rust with
  `WebviewWindowBuilder::data_directory` set to the absolute
  `data/webview/` path (§3.3).
- Minimal TOML configuration at `data/config/deact.toml` (application
  level, non-sensitive only; corrupt files are errors, never silently
  overwritten).
- `tracing` logging to console and file through a non-blocking writer.
- A SQLite single-writer worker (bounded channel, WAL, `busy_timeout`,
  `meta` table with `schema_version = 1`, typed operations only, strict
  initialisation rules — see §9.3), on its own synchronous thread.
- Background task supervision (bounded channel, ping health check) on
  the async runtime provided by Tauri; no second Tokio runtime.
- Windows GUI startup error feedback per §3.6: native `MessageBoxW`
  dialogs (pre-logging failures dialog-only, post-logging failures
  `tracing::error!` plus dialog), with fault injection available only
  in a separate `fault-injection` diagnostic build.
- A Windows GUI-subsystem release build (no console window), produced
  by Windows x64 CI (fmt, clippy, tests, frontend build, Tauri release
  build, artifact upload).

Not implemented: every business module in §4 (Signal Center, Account
Center, Copy Engine, Exchange Adapters, Recovery Manager), the §9 data
model beyond the `meta` table, key encryption, and any trading
functionality. Real-money automated trading stays disabled until the
product owner explicitly approves it. Phase 1's Windows 11
real-machine acceptance is a separate step and is not claimed by CI
success.

## 3. Portable delivery on Windows

### 3.1 Delivery form and layout

DeACT ships as a folder (distributed as an archive), not an installer:

```text
DeACT/
├── DeACT.exe          (the extracted folder initially contains only this)
└── data/              (created automatically on first run)
    ├── deact.db       (SQLite database, plus -wal / -shm while running)
    ├── sessions/
    │   └── telegram/
    ├── keys/
    ├── logs/
    ├── backups/
    └── webview/       (WebView2 user data folder)
```

### 3.2 Portability rules (hard requirements)

- Double-clicking `DeACT.exe` starts the application; first run creates
  `data/` automatically.
- **All** application data lives under `data/` beside the executable:
  the database and its WAL/SHM files, configuration, keys, Telegram
  sessions, logs, caches, backups, and the WebView2 user data folder.
- Nothing may be written to AppData, LocalAppData, Documents, the user
  home directory, the Registry, or any other system directory.
- Paths are resolved from `current_exe()` (or an equivalent reliable
  mechanism), never from the current working directory.
- If the executable's directory is not writable, startup shows a clear
  error and exits. Falling back to a system directory is forbidden.
- Portability is proven only by running the built executable on a real
  Windows machine and auditing every file written during a run
  (Phase 8 of the development plan). No configuration setting, by
  itself, counts as proof.

### 3.3 What Tauri 2 provides — verified facts

Researched against the Tauri 2 documentation and release notes on
2026-11-20:

- `app > appDirectoriesOverride` exists in **Tauri 2.12.0 and later**
  (added in 2.12.0, PR #14620). The migration must therefore use
  **Tauri ≥ 2.12.0**, with the actually used versions locked as §3.5
  specifies. The override can be set in `tauri.conf.json` or at
  runtime through `Context::config_mut`.
- The override replaces the directories returned by the `app_config_dir`,
  `app_data_dir`, `app_local_data_dir`, `app_cache_dir` and
  `app_log_dir` path APIs and the matching `$APPCONFIG`, `$APPDATA`,
  `$APPLOCALDATA`, `$APPCACHE` and `$APPLOG` variables. As a single root
  path, config/data/local-data resolve to the root itself, cache to
  `<root>/caches` and log to `<root>/logs`; a per-directory object is
  also possible. Relative paths resolve against the executable's
  directory.
- Everything that resolves paths through those APIs follows the
  override, including Tauri itself (the **default** webview data
  directory on Windows follows `app_local_data_dir`) and plugins that
  use the path APIs.
- **Exception:** a window's `dataDirectory` config is **not** affected
  by `appDirectoriesOverride`. In addition, the JSON `dataDirectory`
  value is resolved relative to LocalAppData, so it cannot by itself
  place the WebView2 user data folder beside the executable. The Rust
  builder API (`WebviewWindowBuilder::data_directory`, absolute
  `PathBuf`) can. **Decision:** DeACT creates its main window from Rust
  with `WebviewWindowBuilder::data_directory` set to the absolute path
  `<executable directory>/data/webview/`, computed from
  `current_exe()`; the JSON `dataDirectory` config is not used.
  `appDirectoriesOverride` is still used for the Tauri app-directory
  APIs. This decision does not remove the verification duty: Phases 1
  and 8 verify on a real Windows machine where WebView2 actually
  writes, including whether any WebView2 component writes outside the
  configured folder.
- The override only covers paths resolved through the Tauri path APIs.
  Code or third-party dependencies that resolve paths on their own
  (environment variables, `dirs`-style lookups, hardcoded system paths)
  are **not** covered and must be audited individually (§11 C).

### 3.4 WebView2 runtime

Tauri 2 on Windows renders through the Microsoft WebView2 Runtime; DeACT
is therefore **not** independent of WebView2. DeACT accepts the
WebView2 Runtime already present on the target Windows system and does
not bundle it. The WebView2 **user data folder** is application data and
must live at `data/webview/` per §3.2 — the runtime being a system
component does not exempt its data folder from the portability rules.
Behaviour when no WebView2 Runtime is present on the machine must be a
clear startup error, verified in Phase 8.

### 3.5 Version locking

Four different things are versioned, and they are locked separately:

- The Rust `tauri` crate and the whole Rust dependency tree are locked
  exactly by `Cargo.lock`, which is committed; builds use the locked
  versions.
- The frontend Tauri API package (`@tauri-apps/api`): `package.json`
  holds version *ranges*, which are not locks. The exact resolved
  versions are locked by the JavaScript lockfile of the package
  manager chosen in Phase 1 (for example `package-lock.json` or
  `pnpm-lock.yaml`). The lockfile is committed, the chosen manager is
  recorded in Phase 1's notes, and CI installs with that manager's
  frozen/immutable mode.
- The Tauri CLI is pinned as a frontend devDependency (so the same
  lockfile covers it) or, if installed through Cargo, at a recorded
  exact version. An unpinned, globally installed CLI must not produce
  release builds.
- The guarantee that matters: the lockfiles resolve Tauri to
  ≥ 2.12.0 — the versions that provide `appDirectoriesOverride` — and
  CI builds from the lockfiles, so the feature cannot silently
  disappear through drift or a downgrade.

Phase 1 locking outcome (2026-11-20): the package manager is **npm**
(`package-lock.json`, CI installs with `npm ci`). Rust direct
dependencies use exact `=` constraints in `src-tauri/Cargo.toml` and are
locked by the committed `src-tauri/Cargo.lock`: `tauri` 2.12.6,
`tauri-build` 2.7.5, `tokio` 1.53.3, `rusqlite` 0.43.0, `windows-sys`
0.61.2 (Windows only). npm dependencies are exact in `package.json`:
`@tauri-apps/api` 2.12.5, `svelte` 5.58.0; devDependencies
`@tauri-apps/cli` 2.12.5, `vite` 8.3.8, `@sveltejs/vite-plugin-svelte`
7.3.1, `typescript` 6.0.3, `svelte-check` 4.7.8, `@tsconfig/svelte`
5.0.9. TypeScript stays on 6.x because the Svelte language tooling
peers TypeScript ^5/^6 — TypeScript 7 does not yet ship the stable
programmatic API those tools embed. The Rust crate (2.12.6) and the JS
API (2.12.5) are on the same 2.12 minor, which is what Tauri's
build-time version check requires; patch versions are locked
independently, as decided for Phase 1.

### 3.6 Startup failure feedback (Windows GUI)

DeACT is a Windows GUI-subsystem application: a Release build has no
console, so startup failures must never rely on console output
(`eprintln!`, stdout) as the user-facing channel.

- A startup failure that happens **before** logging is initialised is
  shown to the user in a native Windows error dialog (for example via
  `MessageBoxW`).
- A startup failure that happens **after** logging is initialised is
  recorded with `tracing::error!` **and** shown in the same native
  error dialog.
- The message states which startup stage failed and what the user can
  do about it. It must never contain secrets — no keys, passwords,
  sessions or other sensitive material.
- After the failure is reported, the process exits as specified for
  that failure. It never ignores a startup failure silently, and it
  never falls back to AppData, LocalAppData or any other system
  directory to "keep going".

This is a requirement for the Tauri application, implemented in
Phase 1 of the development plan with its own acceptance cases; the
eframe foundation predates it.

## 4. Architecture layers

DeACT is a modular monolith. Core business logic lives in the Rust
backend. The modules are:

1. **Desktop Shell** — the Tauri application: window lifecycle, system
   integration, GUI-subsystem executable. No business logic.
2. **Frontend** — Svelte + TypeScript. Presentation and user interaction
   only.
3. **Application Layer** — application services, command handling,
   business coordination. Tauri Command handlers live here and stay
   thin: they validate input and call services; they never contain
   trading logic.
4. **Signal Center** — signal source management, receiving, parsing and
   normalisation (§5).
5. **Account Center** — account configuration, key management, exchange
   connections (§8).
6. **Copy Engine** — copy rules, quantity calculation, risk checks and
   execution coordination (§7).
7. **Exchange Adapters** — one unified exchange interface and its
   per-exchange implementations (§6).
8. **Storage** — SQLite access, transactions, persistence (§9).
9. **Recovery Manager** — startup recovery, order recovery, exchange
   reconciliation (§7.5).
10. **Logging** — `tracing` logs and audit records.

Runtime model: application background work (signal receiving, exchange
connections, supervision) runs as async tasks on the async runtime
provided by Tauri (Tokio). DeACT does not create a second, independent
Tokio runtime by default. The foundation's design ideas — bounded
channels with explicit overload behaviour, supervisor tasks, health
checks — are kept on top of that runtime. The one exception is storage:
SQLite runs on its own dedicated synchronous worker thread (§9.3), and
blocking database calls never run on Tokio worker threads.

Binding rules:

- The frontend must never access the database directly, sign orders
  directly, or reach an exchange except through Rust application
  services.
- Tauri Commands and Events are only the transport between the UI and
  the Application Layer.
- Signal sources never touch accounts or place orders (§5).
- The Copy Engine depends on the unified exchange interface, never on
  exchange-specific code (§6).

## 5. Signal source architecture

### 5.1 Unified signal sources

DeACT is not a Telegram-only product. Telegram is the **first** signal
source. All sources implement one unified `SignalSource` abstraction;
Discord, Webhook, web pages, custom HTTP endpoints and other messaging
platforms can be added later without changing the Copy Engine.

A signal source only receives and converts messages. It never operates
a trading account and never places an order.

### 5.2 Raw records, normalised signals and the Signal model

Every received message is persisted as a raw record in `signal_events`
(§9): source type, source instance ID, original message ID, original
message content, source timestamp, receive timestamp and parse status.
Parsing produces a normalised Signal, persisted in `parsed_signals`
and referencing its raw record. Handling and execution decisions are
audited in `signal_logs`. Signal source *configuration* lives in
`signal_sources`. The four tables have separate, non-overlapping
responsibilities (§9.1).

Normalised signal kinds (the Signal's `action`), at minimum:

- `Open`
- `Close`
- `Add`
- `Reduce`
- `LeverageChange`
- `Cancel`

A normalised Signal carries, as applicable:

- signal ID (assigned by DeACT);
- source type and source instance ID;
- trader ID (the signal provider's identity within the source);
- original message ID (joining back to the raw record);
- exchange;
- trading instrument / symbol;
- direction (long / short);
- action (one of the kinds above);
- quantity;
- quantity semantics — what the quantity means (base amount, quote
  amount, fraction of balance or of position), stated explicitly,
  never implied;
- target position **or** position delta — these are different
  instructions and are never conflated: a target position states the
  desired end position, a delta states a change relative to the
  current position; a Signal uses one or the other and the field used
  is explicit;
- leverage;
- entry price / price conditions;
- take profit;
- stop loss;
- source timestamp and receive timestamp;
- parse status.

Not every signal kind carries every field (a `Cancel` may carry only
its target reference), but every field's semantics are fixed by this
model. A message whose trading intent cannot be identified with
certainty is recorded with a failed or ambiguous parse status and is
never executed automatically; it is surfaced for review.

### 5.3 Telegram

Telegram access uses a Rust MTProto client with a persisted session
under `data/sessions/telegram/`. **The MTProto crate is not selected.**
Candidates such as ferogram, vaegram and grammers are candidates only;
the selection is researched and decided by ChatGPT in Phase 4 of the
development plan. This document deliberately records no choice.

## 6. Exchange architecture

All exchanges implement one unified Rust trait (working name
`Exchange`). The first adapters are **Hyperliquid** and **AsterDEX**.

An adapter is responsible for: API authentication and request signing,
account balances, position queries, order placement, order
cancellation, order status queries, fill queries, WebSocket market or
account events, reconnection and state resynchronisation.

The Copy Engine and the Recovery Manager depend only on the trait.
Exchange-specific behaviour (ID formats, precision, error codes) stays
inside the adapter. Actual API capabilities and permission models must
be verified against each exchange's official documentation when the
adapter is built (Phase 3/6), not assumed.

## 7. Trading safety and recovery

These requirements are part of the architecture from the start, not
future optimisations.

### 7.1 Order lifecycle

Every order follows an explicit state machine, at minimum:

`Received → Validated → RiskChecked → Submitted → Acknowledged →
PartiallyFilled → Filled`, with `Cancelled`, `Rejected`, and
`Unknown / PendingReconciliation` reachable whenever the exchange's
view of the order is not yet known.

A request timeout **never** means the order failed. On timeout or any
ambiguous result, the executor first queries the exchange for the
order's status and fills, and only then decides whether resubmission
is safe. This is the primary defence against duplicate orders.

### 7.2 Idempotency and reconciliation

- Each signal is processed at most once; duplicate deliveries are
  detected and dropped.
- Each execution intent carries a stable idempotency identifier,
  persisted before submission.
- Partial fills, cancellation confirmations and order state changes are
  applied to the local record as they arrive.
- Local orders, exchange orders and fills must be reconcilable at any
  time. The **exchange is the authority** for actual orders, fills and
  positions; the local database is the record for persistence,
  execution tracking and recovery.

### 7.3 Risk controls

Every automatic execution passes risk checks in the Rust backend
immediately before submission. The control set, at minimum:

- maximum notional value per order;
- maximum position per trading instrument;
- maximum total position (exposure) per account;
- maximum leverage;
- maximum slippage;
- minimum margin requirement;
- maximum daily loss (a breach halts new executions for the day);
- maximum signal latency (a signal older than the limit when it is
  about to be executed is not executed);
- API anomaly circuit breaker (repeated exchange API errors or
  inconsistent responses halt automatic execution until cleared);
- the global emergency stop (§7.4).

A failed check blocks the order and is recorded with the failed
control and the values involved; a blocked order is never silently
dropped. Phase 5 of the development plan implements these controls
and proves each one in Paper Trading.

### 7.4 Emergency stop

Emergency stop, by default, blocks opening new positions and adding to
existing ones. Cancelling orders and closing positions are separate,
explicit operations; emergency stop is never silently equivalent to an
unconditional market close-out.

### 7.5 Startup recovery

Startup order (target architecture):

1. Locate the executable and initialise the portable `data/` directory.
2. Initialise logging.
3. Check database integrity and schema version.
4. Initialise the database access layer.
5. Unlock account keys.
6. Connect to the exchanges.
7. The Recovery Manager inspects unfinished orders and reconciles
   against the exchanges.
8. Restore signal source connections.
9. Only after recovery completes may new signals be executed
   automatically.

Until recovery completes, opening new positions is forbidden.

### 7.6 Safety testing

Planned from the phase that introduces execution (development plan
Phases 2, 5 and 6): Paper Trading, duplicate-signal injection,
request-timeout simulation, partial fills, disconnect/reconnect,
process crash and restart recovery. Real-money automated trading is
forbidden during early development and stays off until the product
owner explicitly approves it.

## 8. Key and secret security

- Secrets at rest are encrypted with **AES-256-GCM**; the encryption
  key is derived from the user's master password with **Argon2id**,
  using a random salt; every encryption uses a random nonce.
- The master password itself is never stored.
- Secret material is cleared from memory with `zeroize`-style handling
  wherever practical.
- Private keys and API secrets must never appear in normal logs, error
  messages, or the UI.
- A failed unlock never leads to a state where trading can execute.
- Exchange API wallets/keys must be restricted to the minimum trading
  permissions and must not have withdrawal permission. What each
  exchange actually supports must be verified against its official API
  documentation (Phase 3).
- Storage layout (default design): encrypted key files live under
  `data/keys/`, one file per account secret. The `accounts` table
  stores only the key-file reference and metadata (which account,
  which exchange, file name, format version) — never the secret
  itself.
- Key file format: a versioned encrypted envelope holding the format
  version, the Argon2id parameters used, the salt, the nonce and the
  ciphertext, so a future format version is recognised and migrated
  instead of guessed.
- Backups: a backup is complete only if it contains the database, the
  key files, the Telegram sessions and the other runtime data needed
  to restore the application. The SQLite part must be produced with a
  WAL-consistent method (the SQLite Online Backup API or
  `VACUUM INTO`), never by copying `deact.db` alone while WAL is in
  use — the `-wal` file can hold committed transactions the main file
  does not yet contain.

## 9. SQLite data model plan

This section is a **plan**. Except for the `meta` table (§9.3), none of
these tables exist yet; creating them is Phase 2 work, not part of the
current foundation.

### 9.1 Planned entities

| Table | Purpose |
|---|---|
| `accounts` | Exchange accounts (no plaintext secrets; stores the `data/keys/` key-file reference and metadata, §8) |
| `signal_sources` | Signal source configuration and instances (type, instance identity, source-specific settings, enabled state) |
| `signal_events` | Raw messages and source events as received (the §5.2 raw-record fields) |
| `parsed_signals` | Normalised Signals (§5.2) parsed from `signal_events` |
| `copy_configs` | Copy-trading rules per source/account pair |
| `orders` | Local orders with full lifecycle state (§7.1) |
| `fills` | Individual executions, linked to orders |
| `positions` | Current known positions per account/instrument |
| `trades` | Completed trade records derived from fills |
| `signal_logs` | Audit of signal handling and execution decisions |
| `settings` | Application settings key/value |

### 9.2 Model rules

- **Versioning and migration:** the existing `meta.schema_version`
  mechanism is the version source. Migrations are ordered, one per
  version step, each in its own transaction, applied at startup before
  any business module runs. A database newer than the binary, or one
  failing the §9.3 integrity rules, is rejected with a clear error.
- **Keys and constraints:** every table has a stable primary key;
  uniqueness is enforced in the schema where the domain requires it
  (e.g. one local order per idempotency identifier; one fill per
  exchange fill ID).
- **Signal idempotency:** a raw message's idempotency identity is
  (source type, source instance, original message ID), enforced with a
  unique constraint on `signal_events` so a redelivered message cannot
  be processed twice.
- **Order/fill relations:** `fills.order_id` references the local
  order; each order stores both the local order ID and the exchange's
  order ID, so local records, exchange orders and fills can always be
  joined for reconciliation.
- **Transaction boundaries:** state transitions that must not tear
  (order state + fill application; signal + its parsed result +
  execution intent) are written in a single transaction through the
  single-writer path.
- **WAL:** WAL mode stays enabled; the `-wal`/`-shm` files live beside
  `deact.db` in `data/`, as §3.2 requires.
- **Single writer:** all writes go through one controlled single-writer
  path (the existing worker design, §9.3). No module opens its own
  write connection.
- **Recovery fields:** orders persist their lifecycle state, the
  idempotency identifier, the exchange order ID, submitted parameters,
  and last-known fill totals, so the Recovery Manager can resume and
  reconcile after any crash (§7.5).

### 9.3 Current storage implementation (status)

Implemented today: one `DbWorker` thread owning the single rusqlite
connection, fed by a bounded channel (capacity 64); WAL,
`busy_timeout = 5000`, `foreign_keys = ON`; typed operations only
(`schema_version`, `journal_mode`, `get_meta`, `set_meta`) with
parameter binding — no arbitrary SQL is exposed to callers.
Initialisation is strict: only a database with no user tables is
created fresh (meta table + `schema_version = 1` in one transaction);
an existing database is never re-seeded; a `meta` table missing its
version row reports `MissingSchemaVersion`; a database with tables but
no `meta` table is rejected as `IncompleteDatabase`. Whether this
worker design carries into the Tauri application unchanged, or is
adapted (e.g. owned by the Application Layer instead of the binary
entry point), is a Phase 1 evaluation (§11 C).

## 10. UI design specification

Professional dark trading-terminal style: low decoration, clear
numbers, reasonable information density.

Palette (exact values):

| Role | Value |
|---|---|
| App background | `#09090b` |
| Panel | `#121214` |
| Border | `#1f1f23` |
| Primary text | `#f4f4f5` |
| Secondary text | `#a1a1aa` |
| Accent | `#c8ccd4` |
| Success | `#22c55e` |
| Danger | `#ef4444` |
| Warning | `#f59e0b` |

Forbidden: purple, gold, gradients, large areas of saturated colour,
emoji, and icon clutter.

Pages:

1. Overview （总览）
2. Trading Center （交易中心）
3. Copy Configuration （跟单配置）
4. Signal Center （信号中心）
5. Account Configuration （账户配置）
6. System Settings （系统设置）

Rules: lists and tables must handle large data sets (sticky headers,
virtual scrolling); long operations must never block the UI (work runs
in the Rust backend; the UI shows progress); dangerous operations
require an explicit second confirmation.

## 11. Existing foundation: reuse, migrate, verify

Based on reading the actual code in PR #1 (not on file names).

### A. Reuse as-is or with minor adaptation

- `src/paths/` — the portable-path design: `current_exe()` resolution,
  exclusive-creation writability probe, probe-cleanup error reporting,
  and the no-fallback rule. (The concrete layout changes from
  `DeACT_Data/` to `data/` — see B.)
- `src/storage/` — the SQLite single-writer worker, WAL and
  `busy_timeout` configuration, the strict database initialisation
  rules, the `meta`/`schema_version` mechanism, typed operations with
  parameter binding, and its error taxonomy.
- `src/logging/` — `tracing` setup with the non-blocking file writer
  and the guard-based flush on shutdown.
- `src/config/` — the load-or-create TOML configuration approach and
  the rule that a corrupt file is an error and is never overwritten.
- `src/runtime/` — the design ideas: bounded channels with explicit
  overload behaviour, a supervisor task, a ping health check, orderly
  shutdown. The standalone runtime itself is not carried over (§11 B);
  the ideas are re-expressed on Tauri's async runtime (§4).
- All existing tests (`tests/paths_tests.rs`, `config_tests.rs`,
  `storage_tests.rs`, `runtime_tests.rs` and the in-module unit tests)
  — they keep covering the reused modules.
- The Windows GUI-subsystem fix in `src/main.rs` (the
  `windows_subsystem` attribute) as the record of why the release
  executable must not open a console window; Tauri's own template
  handles the subsystem, and Phase 8 re-verifies it on the Tauri build.
- The Windows x64 CI workflow (fmt, clippy, tests, release build,
  artifact) as the base for the Tauri build pipeline.

### B. Migrate or replace

- `src/app/` (eframe/egui window) — replaced by the Tauri Desktop Shell
  plus the Svelte frontend. egui is not part of the target stack.
- `src/main.rs` — replaced by the Tauri application entry point
  (`Builder`, generated context, runtime-set `appDirectoriesOverride`,
  and the main window created from Rust with
  `WebviewWindowBuilder::data_directory` per §3.3). The ordered
  startup sequence it implements is re-expressed in the Application
  Layer and extended into the §7.5 recovery sequence.
- `src/runtime/`'s standalone Tokio runtime — replaced by the async
  runtime provided by Tauri (§4). No second independent runtime is
  created by default.
- The data-directory layout — `DeACT_Data/` becomes `data/`, with the
  new subdirectories `sessions/telegram/`, `keys/` and `webview/`
  added; `paths` gains these locations.
- UI↔backend communication — the current `AppStatus` snapshot hand-off
  is replaced by Tauri Commands and Events (§4).
- The release/artifact pipeline — extended with the frontend build
  (Svelte/TypeScript) and the Tauri bundling step, while staying a
  portable folder distribution, not an installer (§3.1).

### C. To verify during migration (Phase 1 unless noted)

Resolved in Phase 1 (2026-11-20):

- Module integration and ownership: the `DbWorker` is owned by the
  startup flow in the library's `run()` and is shut down after
  `Builder::run` returns (and before exiting on a run failure); the
  supervisor lives in Tauri managed state for the app's lifetime;
  background async work runs on Tauri's async runtime (§4).
- Configuration: the TOML configuration stays, at
  `data/config/deact.toml`, holding application-level non-sensitive
  settings only. The formal role of the SQLite `settings` table is
  decided in Phase 2; TOML and SQLite must never both be the
  authoritative source for the same setting.
- Package manager and exact dependency versions: npm, recorded in §3.5.

Still to verify:

- The exact landing spot of the WebView2 user data folder on a real
  Windows machine under the decided builder `data_directory` (§3.3),
  including whether any WebView2 component writes outside it.
  (Phase 1 real-machine acceptance.)
- Every path a plugin or third-party dependency writes to (store,
  log, updater, HTTP caches, WebView2 caches) — anything resolving
  paths outside the Tauri path APIs escapes the override. Phase 1
  introduces no Tauri plugins, so this audit currently covers Tauri
  itself and WebView2; it must be repeated whenever a plugin is added.
- Windows x64 build of the full Tauri + Svelte stack and the exact
  dependency set it produces (WebView2 loader, redistributables).
- Telegram MTProto crate selection (Phase 4, decided by ChatGPT, §5.3).
- Exchange API capabilities and permission models (Phases 3 and 6,
  verified against official documentation, §6 and §8).

## 12. Engineering constraints

- One modular-monolith process; no microservices and none of the
  excluded infrastructure in §1.
- Stability, idempotency, security and error handling are built in
  from the earliest phases (see the development plan), not retrofitted.
- No new dependency is introduced without it being part of an approved
  phase task.
- This document and `docs/DEVELOPMENT_PLAN.md` are the planning source
  of truth; when implementation and this document disagree, the
  disagreement is reported, not silently resolved.
