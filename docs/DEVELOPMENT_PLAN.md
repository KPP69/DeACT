# DeACT Development Plan

This plan implements the architecture in `docs/ARCHITECTURE.md`. It
replaces the earlier seven-phase outline. There are eight phases; each
phase lists its goal, scope, prerequisites, test requirements and
acceptance criteria. A phase is complete only when its acceptance
criteria are met and reviewed — writing the code is not enough.

Standing principles for every phase:

- Stability, idempotency, security and error handling are built in from
  the earliest phases, never deferred to a later cleanup phase.
- Real-money automated trading stays disabled until the product owner
  explicitly approves it, regardless of phase.
- No phase introduces dependencies or infrastructure outside the
  architecture document (§1 and §12 there).
- Each phase ends with: tests green locally and in Windows CI, the
  architecture document updated if the phase changed any design fact,
  and a review by ChatGPT before the next phase starts.

Pre-stage (already done, not a phase): the PR #1 eframe/egui
foundation — portable paths, TOML config, `tracing` logging, the SQLite
single-writer worker, the background Tokio runtime, 20 passing tests,
Windows x64 CI. Its reusable parts are listed in ARCHITECTURE.md §11 A;
Phase 1 builds on them instead of starting over.

## Phase 1 — Tauri 2 project, portable directory, SQLite, logging, basic UI

**Goal.** A running Tauri 2 + Svelte + TypeScript desktop application
on Windows x64 that reuses the foundation's Rust core and keeps every
byte of its data under `data/` beside the executable.

**Scope.**

- Scaffold the Tauri 2 application (Tauri ≥ 2.12.0; versions locked
  per ARCHITECTURE.md §3.5) with a Svelte + TypeScript frontend and
  plain CSS; no component library, no extra state framework.
- Set `app > appDirectoriesOverride` (config or runtime via
  `Context::config_mut`) to the `data/` root for the Tauri
  app-directory APIs; create the main window from Rust with
  `WebviewWindowBuilder::data_directory` set to the absolute
  `<executable directory>/data/webview/` path (§3.3).
- Rename the portable layout from `DeACT_Data/` to `data/` with
  `sessions/`, `keys/`, `logs/`, `backups/`, `webview/` subdirectories;
  keep the `current_exe()` + writability-probe + no-fallback behaviour.
- Move the existing `paths`, `storage`, `logging` and `config` modules
  into the Tauri backend (Application Layer owns them; Tauri commands
  stay thin). Backend async work runs on Tauri's async runtime — no
  second Tokio runtime is created; the foundation runtime's
  bounded-channel, supervisor and health-check patterns are
  re-expressed there, and the SQLite worker remains a dedicated
  synchronous thread.
- Lock versions per ARCHITECTURE.md §3.5: the Rust side in
  `Cargo.lock`; `@tauri-apps/api` and the Tauri CLI in the frontend
  lockfile of the package manager chosen in this phase (the choice is
  recorded); CI installs from both lockfiles in frozen/immutable mode.
- Basic UI shell: the six-page navigation skeleton (ARCHITECTURE.md
  §10 palette) with a status page showing data directory, schema
  version and runtime state through Tauri Commands/Events.
- Startup failure feedback for the GUI build (ARCHITECTURE.md §3.6):
  a native Windows error dialog (for example `MessageBoxW`) for
  failures before logging is initialised; `tracing::error!` plus the
  dialog for failures afterwards. Messages name the failed stage and
  the remedy, contain no secrets, and the process then exits without
  any directory fallback.
- Decide and record: TOML config vs the `settings` table (§11 C).
- Extend CI: frontend build + Tauri release build + artifact upload.

**Prerequisites.** PR #1 foundation complete (done); the §11 A/B/C
lists agreed.

**Test requirements.** All reused-module tests keep passing; new tests
for the `data/` layout creation; a scripted Windows run that lists
every file created outside `data/` (must be none attributable to
DeACT); startup-failure drills on Windows covering the acceptance
cases below (unwritable data directory, pre-logging failure,
post-logging failure, secret-free messages).

**Acceptance criteria.** Double-clicking the built `DeACT.exe` on a
real Windows 11 machine opens the Tauri window with no console window;
`data/` is created beside the exe; the WebView2 user data folder is
verified to be inside `data/`; no DeACT writes appear in AppData,
LocalAppData, Documents or the Registry during a run; CI is green.
Startup failure feedback (§3.6) is demonstrated on the same machine:
(1) with the data directory made unwritable, startup shows the native
error dialog and the process exits; (2) a simulated failure before
logging initialisation still shows the dialog; (3) a simulated failure
after logging initialisation shows the dialog and leaves a
`tracing::error!` record; (4) neither the dialog nor the log contains
any secret material; (5) the Release GUI build reports these failures
without any console output.

**Implementation record (2026-11-20).** Phase 1 was implemented on the
existing branch as planned: the eframe/egui foundation was replaced by
the Tauri 2 + Svelte stack in the standard layout (`src-tauri/` backend,
frontend at the repository root); `paths`, `storage`, `logging` and
`config` were migrated with their behaviour and tests intact (the data
layout changed to `data/` with the configuration at
`data/config/deact.toml`); the standalone Tokio runtime was replaced by
a supervisor on Tauri's async runtime; the §3.6 error feedback is
implemented with `MessageBoxW` behind a dedicated module, and fault
injection exists only in the separate `fault-injection` diagnostic
build. Locked versions and the npm decision are recorded in
ARCHITECTURE.md §3.5. Code-level tests and the Windows x64 CI build are
part of the delivery; the real-machine acceptance above remains the
product owner's step and is not replaced by CI success.

## Phase 2 — Core data model, normalised signals, order state machine, recovery mechanism

**Goal.** The persistent core of the trading system exists and is
provably consistent before any exchange or signal source is connected.

**Scope.**

- Implement the ARCHITECTURE.md §9 schema (accounts, signal_sources,
  signal_events, parsed_signals, copy_configs, orders, fills,
  positions, trades, signal_logs, settings) through ordered,
  transactional schema migrations on top of `meta.schema_version`.
- Implement the normalised signal model (§5.2) and the raw record in
  `signal_events` with its idempotency identity and unique constraint.
- Implement the order lifecycle state machine (§7.1) as pure,
  testable transitions in the Copy Engine core, with the
  timeout → query-first rule encoded in the executor interface.
- Implement the Recovery Manager's local half: detecting unfinished
  orders at startup, the recovery gate that blocks new executions
  until recovery completes (§7.5), and reconciliation data structures
  (the exchange side is stubbed behind the `Exchange` trait).

**Prerequisites.** Phase 1 accepted.

**Test requirements.** Migration tests (fresh database, upgrade from
each earlier version, corrupt/too-new database rejected); state-machine
transition tests including illegal transitions; duplicate-signal
injection proving single processing; crash-during-write recovery
tests using the single-writer store.

**Acceptance criteria.** A database can be created, migrated and
reopened with full integrity; the same signal delivered twice produces
exactly one execution intent; every order state transition is covered
by tests; startup with unfinished orders enters recovery mode and
refuses new executions until recovery is marked complete.

## Phase 3 — Account management, key encryption, exchange connections

**Goal.** Users can add exchange accounts whose secrets are encrypted
at rest, and the application can authenticate to Hyperliquid and
AsterDEX read-only.

**Scope.**

- Account Center: account CRUD (ARCHITECTURE.md §4), stored per §9.
- Key vault: AES-256-GCM + Argon2id (random salt and nonce), master
  password set/unlock flow, `zeroize`-style clearing, unlock-failure
  lockout of any execution state (§8). Key files use the versioned
  envelope format of §8 (format version, Argon2id parameters, salt,
  nonce, ciphertext) and live in `data/keys/`; `accounts` stores only
  the key-file reference and metadata.
- Exchange adapters (read scope): authentication/signing, balance and
  position queries, order/fill status queries for Hyperliquid and
  AsterDEX, behind the unified `Exchange` trait (§6). No order
  placement in this phase.
- Verify each exchange's API permission model against its official
  documentation; record that API keys must not have withdrawal
  permission, and surface that in the account UI.

**Prerequisites.** Phase 2 accepted.

**Test requirements.** Encryption round-trip and wrong-password tests;
tests proving secrets never appear in logs (log capture assertion);
adapter tests against exchange testnets or recorded fixtures for
signing, balances, positions and status queries; reconnect tests.

**Acceptance criteria.** Secrets exist on disk only encrypted; a wrong
master password cannot unlock or trade; both exchanges return real
balances/positions for a test account; no secret material is found in
`data/logs/` after a full session.

## Phase 4 — Telegram MTProto, session, message receiving and parsing

**Goal.** Telegram channels/groups are received live, persisted raw,
and parsed into normalised signals — without any trading execution.

**Scope.**

- MTProto crate selection is researched and decided by ChatGPT at the
  start of this phase (ARCHITECTURE.md §5.3); only then is the crate
  adopted.
- Telegram source implementing the unified `SignalSource` abstraction:
  login, session persistence under `data/sessions/telegram/`,
  reconnect, per-source configuration stored in `signal_sources`.
- Raw message persistence in `signal_events` (§5.2 fields) and the
  parser producing `parsed_signals` for the six normalised kinds;
  unrecognisable messages are recorded as unparseable, never guessed.
- Signal Center UI wiring (page exists from Phase 1): source list,
  raw signal list, parse results.

**Prerequisites.** Phase 3 accepted (accounts exist for later binding;
parsing itself needs no account).

**Test requirements.** Parser tests on a fixture corpus covering all
six signal kinds plus malformed/ambiguous messages; duplicate message
delivery proving idempotent storage; session restore after restart;
long-run receive stability test.

**Acceptance criteria.** A test channel's messages appear in the
Signal Center in real time, survive restart via the persisted session,
parse to the expected normalised signals on the fixture corpus, and
duplicates are stored once.

## Phase 5 — Copy configuration, risk controls, Paper Trading

**Goal.** The full copy-trading pipeline runs end to end against
simulated execution, with risk controls enforced.

**Scope.**

- Copy configuration per source/account pair (`copy_configs`):
  quantity modes and calculation, per-account and per-symbol limits.
- Risk controls (ARCHITECTURE.md §7.3) implemented in the Copy Engine:
  maximum notional value per order, maximum position per instrument,
  maximum total account exposure, maximum leverage, maximum slippage,
  minimum margin requirement, maximum daily loss, maximum signal
  latency, the API anomaly circuit breaker, and the global emergency
  stop (§7.4) — blocks opens/adds, with cancel/close as separate
  explicit operations.
- Paper Trading executor implementing the `Exchange` trait interface
  used by the Copy Engine: simulated fills including partial fills,
  latency and timeout simulation hooks.
- Signal → intent → paper order → fill → position pipeline with full
  audit records (`signal_logs`, `signal_events`).

**Prerequisites.** Phase 4 accepted.

**Test requirements.** The §7.6 safety suite against Paper Trading:
duplicate signals, request timeouts (query-first, no duplicate
orders), partial fills, disconnect/reconnect, crash and restart
recovery mid-position; risk-limit tests proving blocked orders are
recorded with reasons. Plus one Paper Trading test per §7.3 control:
an order breaching each limit is blocked and recorded with the failed
control; a signal older than the latency limit is not executed;
simulated repeated API anomalies trip the circuit breaker and halt
execution; reaching the daily-loss limit halts new executions for the
day.

**Acceptance criteria.** The pipeline runs a full day of recorded
signal fixtures with zero duplicate executions, zero risk-limit
breaches, and a complete audit trail from raw signal to paper fill;
every §7.3 risk control demonstrably blocks its breaching scenario in
Paper Trading with an audit record; emergency stop demonstrably
blocks new opens while leaving cancel and close available.

## Phase 6 — Real exchange execution, order reconciliation, position recovery

**Goal.** The Copy Engine executes on Hyperliquid and AsterDEX for
real, with reconciliation and recovery proven under failure.

**Scope.**

- Order placement/cancellation in both adapters (the write scope of
  the `Exchange` trait), with the §7.1 state machine and §7.2
  idempotency identifiers wired end to end.
- WebSocket account/order event streams with reconnect and
  resynchronisation; fills applied transactionally.
- Recovery Manager completed: startup reconciliation against both
  exchanges (§7.5 sequence), `Unknown / PendingReconciliation`
  resolution, position recovery.
- Small-value live-fire procedure defined with the product owner;
  real-money execution remains off by default and is enabled only by
  explicit owner approval for the agreed test amounts.

**Prerequisites.** Phase 5 accepted, including its full safety suite.

**Test requirements.** Testnet end-to-end execution first; then the
§7.6 suite repeated against testnet (timeouts, partial fills,
disconnects, crash/restart during an open order); reconciliation test
with deliberately diverged local state proving the exchange wins.

**Acceptance criteria.** On testnet: orders traverse the full state
machine to Filled/Cancelled with matching fills; killing the process
mid-order and restarting reconciles to the exchange's state with no
duplicate order; local and exchange positions agree after recovery.
Live trading stays disabled pending the owner's separate approval.

## Phase 7 — Complete UI: overview, trading center, logs, settings

**Goal.** All six pages (ARCHITECTURE.md §10) are fully functional,
with the performance and safety interaction rules implemented.

**Scope.**

- Overview: account balances, positions, copy status, recovery state.
- Trading Center: orders, fills, positions with live updates via
  Tauri Events; manual cancel/close as explicit confirmed operations.
- Copy Configuration and Signal Center completed (building on
  Phases 4–5 wiring).
- Account Configuration completed (Phase 3 flows) and System Settings:
  data directory info, log access, backup/restore of `data/`
  (WAL-consistent database backup per ARCHITECTURE.md §8, including
  key files and Telegram sessions).
- Table performance: sticky headers, virtual scrolling for large
  lists; long operations run in the backend with progress, never
  blocking the UI; dangerous operations require second confirmation.
- Audit/log viewer over `signal_logs` and the `tracing` log files.

**Prerequisites.** Phase 6 accepted.

**Test requirements.** UI smoke tests for every page against a
paper-trading backend; large-list rendering test (10k+ rows) with
virtual scrolling; confirmation-flow tests for dangerous operations;
UI responsiveness test during a long backend operation.

**Acceptance criteria.** Every page works against live backend data;
no page freezes during backend work; dangerous actions cannot be
triggered without the confirmation step; the §10 palette and layout
rules are followed throughout (no forbidden colours/gradients/emoji).

## Phase 8 — Windows portable build, runtime checks, final acceptance

**Goal.** The distributable portable package is produced by CI and
passes a full real-machine acceptance run.

**Scope.**

- Release pipeline: reproducible portable folder build (exe + nothing
  else in the archive; `data/` created on first run), GUI-subsystem
  executable (no console window — re-verified on the Tauri build),
  PE subsystem check in CI.
- Runtime checks on a clean Windows 11 machine: WebView2 Runtime
  present/missing behaviour (clear error when missing), first-run
  directory creation, full write audit (nothing outside `data/`),
  upgrade by replacing the exe (data preserved), backup/restore.
- Final acceptance run of the §7.6 safety suite on the packaged build
  (Paper Trading mode), plus the Phase 6 testnet suite.
- Documentation freeze: ARCHITECTURE.md and this plan updated to match
  the shipped implementation exactly.

**Prerequisites.** Phase 7 accepted.

**Test requirements.** Clean-machine test matrix (fresh Windows 11,
non-writable install location, missing WebView2 Runtime); archive
extraction → double-click → trade-on-paper → restart → reconcile
end-to-end run.

**Acceptance criteria.** The packaged build passes every check above
on a real machine; the file-write audit is clean; the product owner
signs off. Creating a GitHub Release or Tag happens only after this
sign-off and only on the owner's instruction. Real-money automated
trading remains a separate, later decision.
