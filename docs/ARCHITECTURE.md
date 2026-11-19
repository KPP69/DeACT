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
| Desktop framework | Tauri 2 (see §3.3 for the minimum version) |
| Frontend | Svelte + TypeScript, plain CSS |
| Core language | Rust |
| Async runtime | Tokio |
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

The current branch (`feature/windows-app-foundation`, PR #1) contains an
**eframe/egui foundation**, built before the Tauri decision. It is an
engineering base, not the product:

Implemented and tested today:

- Portable data-directory resolution from `current_exe()` with a
  writability probe and no fallback (currently creating `DeACT_Data/`).
- Minimal TOML configuration (corrupt files are errors, never silently
  overwritten).
- `tracing` logging to console and file through a non-blocking writer.
- A SQLite single-writer worker (bounded channel, WAL, `busy_timeout`,
  `meta` table with `schema_version = 1`, typed operations only, strict
  initialisation rules — see §9.3).
- A background Tokio runtime with a bounded command channel and a ping
  health check.
- A minimal eframe/egui status window (to be replaced, §11 B).
- A Windows GUI-subsystem release build (no console window).
- 20 automated tests, all passing; Windows x64 CI (fmt, clippy, tests,
  MSVC release build, artifact upload).

Not implemented: Tauri, the Svelte frontend, every business module in
§4 (Signal Center, Account Center, Copy Engine, Exchange Adapters,
Recovery Manager), the §9 data model beyond the `meta` table, key
encryption, and any trading functionality. Real-money automated trading
stays disabled until the product owner explicitly approves it.

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
  **Tauri ≥ 2.12.0**, pinned exactly in `Cargo.lock` / `package.json`
  at migration time. The override can be set in `tauri.conf.json` or at
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
  `PathBuf`) can. DeACT therefore plans to create its main window from
  Rust with an explicit `data_directory` of `data/webview/`, or to rely
  on the override-driven default — the choice is made and verified in
  Phase 1, on a real Windows machine, by inspecting where WebView2
  actually writes.
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
   reconciliation (§7.4).
10. **Logging** — `tracing` logs and audit records.

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

### 5.2 Normalised signals and raw records

Normalised signal kinds, at minimum:

- `Open`
- `Close`
- `Add`
- `Reduce`
- `LeverageChange`
- `Cancel`

Every raw signal is persisted with: source type, source instance ID,
original message ID, original message content, source timestamp,
receive timestamp, parse status, and the normalisation result.

A message whose trading intent cannot be identified with certainty must
not be guessed into an order. It is recorded and surfaced for review.

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

### 7.3 Emergency stop

Emergency stop, by default, blocks opening new positions and adding to
existing ones. Cancelling orders and closing positions are separate,
explicit operations; emergency stop is never silently equivalent to an
unconditional market close-out.

### 7.4 Startup recovery

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

### 7.5 Safety testing

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
- Encrypted key material lives under `data/keys/`.

## 9. SQLite data model plan

This section is a **plan**. Except for the `meta` table (§9.3), none of
these tables exist yet; creating them is Phase 2 work, not part of the
current foundation.

### 9.1 Planned entities

| Table | Purpose |
|---|---|
| `accounts` | Exchange accounts (no plaintext secrets; encrypted material in `data/keys/` or an encrypted column) |
| `signals` | Raw signals as received (§5.2 fields) |
| `signal_events` | Lifecycle events of a signal (received, parsed, rejected, executed, …) |
| `parsed_signals` | Normalised signals produced from raw signals |
| `copy_configs` | Copy-trading rules per source/account pair |
| `orders` | Local orders with full lifecycle state (§7.1) |
| `fills` | Individual executions, linked to orders |
| `positions` | Current known positions per account/instrument |
| `trades` | Completed trade records derived from fills |
| `signal_logs` | Audit log of signal handling decisions |
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
- **Signal idempotency:** a signal's idempotency identity is
  (source type, source instance, original message ID), enforced with a
  unique constraint so a redelivered message cannot be processed twice.
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
  reconcile after any crash (§7.4).

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
- `src/runtime/` — the pattern of a background Tokio runtime owned
  outside the UI, driven by a bounded channel with orderly shutdown.
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
  (`Builder`, generated context, runtime-set `appDirectoriesOverride`
  and/or explicit window `data_directory` per §3.3). The ordered
  startup sequence it implements is re-expressed in the Application
  Layer and extended into the §7.4 recovery sequence.
- The data-directory layout — `DeACT_Data/` becomes `data/`, with the
  new subdirectories `sessions/telegram/`, `keys/` and `webview/`
  added; `paths` gains these locations.
- UI↔backend communication — the current `AppStatus` snapshot hand-off
  is replaced by Tauri Commands and Events (§4).
- The release/artifact pipeline — extended with the frontend build
  (Svelte/TypeScript) and the Tauri bundling step, while staying a
  portable folder distribution, not an installer (§3.1).

### C. To verify during migration (Phase 1 unless noted)

- How the existing Rust modules integrate with the Tauri runtime
  (ownership of `DbWorker` and the Tokio runtime: Tauri also manages a
  runtime; the two must not fight — e.g. backend services on DeACT's
  own Tokio runtime, Tauri commands as thin adapters).
- Whether the SQLite worker needs changes for Tauri (thread ownership,
  shutdown ordering with window lifecycle).
- The exact landing spot of the WebView2 user data folder under
  `appDirectoriesOverride` on a real Windows machine, and whether the
  explicit builder `data_directory` is needed (§3.3).
- Every path a plugin or third-party dependency writes to (store,
  log, updater, HTTP caches, WebView2 caches) — anything resolving
  paths outside the Tauri path APIs escapes the override.
- Windows x64 build of the full Tauri + Svelte stack and the exact
  dependency set it produces (WebView2 loader, redistributables).
- Whether the TOML configuration stays, moves into the `settings`
  table, or both — decided in Phase 1 and recorded here afterwards.
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
