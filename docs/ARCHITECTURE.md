# DeACT Architecture — Foundation Stage

This document records the architecture **actually implemented** in the
`feature/windows-app-foundation` stage. It is not a plan for future work:
anything not listed here as implemented is not implemented.

## 1. Overview

DeACT is a modular, single-process Rust desktop application for Windows 11
x64. The foundation stage builds only the engineering base: a portable data
directory, configuration, logging, a SQLite worker, a background Tokio
runtime, and a minimal native window. There is no signal source, no
exchange adapter, no copy-trading engine, no order handling and no
real-money functionality.

- Language: Rust 1.100.0 (see `rust-toolchain.toml`), Windows MSVC x64.
- UI: eframe / egui 0.37, native desktop. No WebView/WebView2 runtime.
- Async: Tokio 1.53 (`rt-multi-thread`, `macros`, `sync`, `time`).
- Database: rusqlite 0.43 with `bundled`, SQLite WAL mode.
- Config: Serde + TOML. Logging: `tracing` + `tracing-subscriber` +
  `tracing-appender` (non-blocking file writer).
- `Cargo.lock` is committed; dependency versions are whatever Cargo
  resolved at build time (see the lock file for exact versions).

## 2. Module responsibilities

```
src/
├── main.rs        Startup shell only: ordered initialisation, then the
│                  window, then orderly shutdown. No business logic.
├── lib.rs         Re-exports the modules so integration tests can use them.
├── app/           Native window (eframe/egui). Renders a snapshot of the
│                  startup state. No database, filesystem or network I/O.
├── runtime/       Background multi-thread Tokio runtime with a bounded
│                  command channel and a supervisor task. No network
│                  service is connected in this stage.
├── config/        Minimal TOML configuration (Serde). Load, save, and
│                  load-or-create-default. Corrupt files are errors and
│                  are never silently overwritten.
├── paths/         Portable path resolution from `current_exe()` and
│                  creation of DeACT_Data/ plus logs/, cache/, backups/.
├── storage/       SQLite access through exactly one worker thread fed by
│                  a bounded channel. WAL mode, busy timeout, `meta`
│                  table with schema_version. The worker exposes only
│                  typed operations (`schema_version`, `journal_mode`,
│                  `get_meta`, `set_meta`); no arbitrary SQL is exposed
│                  to any caller, and every statement uses parameter
│                  binding. Schema-version errors distinguish a missing
│                  row, a malformed value, and a failed query.
└── logging/       tracing subscriber: console (ANSI) + DeACT_Data/logs/
                   deact.log (no ANSI, appended) through a non-blocking
                   writer; the `LogGuard` holds the appender's
                   `WorkerGuard`, so dropping it at shutdown flushes
                   buffered records and joins the writer thread.
```

Dependency direction: `main` → `app`, `runtime`, `config`, `paths`,
`storage`, `logging`. `app` depends on nothing else in the crate (it
receives an `AppStatus` snapshot). `storage`, `runtime`, `config`,
`paths` and `logging` do not depend on each other or on `app`.

## 3. Portable data directory

- The data root is resolved from `std::env::current_exe()`, never from
  the current working directory, the user home, Documents, AppData /
  LocalAppData, the Registry, or any system directory.
- Layout beside `DeACT.exe`:
  - `DeACT_Data/config.toml` — configuration.
  - `DeACT_Data/deact.db` (+ `-wal` / `-shm` while running) — SQLite.
  - `DeACT_Data/logs/deact.log` — log file.
  - `DeACT_Data/cache/` — cache (currently unused).
  - `DeACT_Data/backups/` — backups (currently unused).
- At startup the directories are created and writability is proven by
  creating and removing a uniquely named probe file with exclusive
  creation (`OpenOptions::create_new`), so an existing file is never
  overwritten; a probe that cannot be removed is reported as an error
  instead of being silently ignored. If the executable directory is not
  writable, startup fails with a clear error and the process exits.
  There is no fallback to another location.

## 4. UI and background runtime communication

- The Tokio runtime runs on its own worker threads, never on the UI
  thread. It is driven through one bounded channel (capacity 64) carrying
  `RuntimeCommand` values (`Ping`, `Shutdown`).
- The UI never sends commands to the runtime in this stage and background
  tasks never modify UI state. The window only renders the `AppStatus`
  snapshot that `main` built after successful initialisation.
- Shutdown is ordered: the window closes first, then the runtime is shut
  down (5 s timeout), then the database worker is stopped.

## 5. SQLite single-writer worker

- Exactly one `DbWorker` exists, owned by `main`. It spawns one worker
  thread that owns the single `rusqlite::Connection`.
- All requests go through a bounded `std::sync::mpsc::sync_channel`
  (capacity 64) and carry a reply channel. Callers block for the reply;
  the UI thread never calls the worker.
- On open the worker sets `journal_mode = WAL`, `busy_timeout = 5000`,
  `foreign_keys = ON`, creates the `meta` table and inserts
  `schema_version = 1` (`INSERT OR IGNORE`, so reopening is safe).
- `DbWorker::open` blocks until the worker reports initialisation success
  or failure, so a bad database fails startup instead of failing later.
- The worker's public API is typed only: `schema_version()`,
  `journal_mode()`, `get_meta(key)` and `set_meta(key, value)`. There is
  no method that accepts an SQL string, so no other module can run
  arbitrary SQL; the worker's own statements are fixed text with
  parameter binding.
- `schema_version()` distinguishes its failure modes: a missing
  `schema_version` row is `MissingSchemaVersion`, a stored value that is
  not an integer is `InvalidSchemaVersion` (carrying the stored text),
  and a query that fails returns the underlying SQLite error.
- The worker stops on an explicit `Shutdown` request, on channel close,
  or when dropped (`shutdown()` is idempotent and joins the thread).

## 6. Startup and shutdown

Startup (`src/main.rs`, in order):

1. Resolve the executable path; create and verify `DeACT_Data/`.
2. Initialise logging (console + `DeACT_Data/logs/deact.log`).
3. Load `DeACT_Data/config.toml`, or create the default.
4. Open the SQLite worker (`DeACT_Data/deact.db`) and read the schema
   version.
5. Start the background runtime and run a ping health check.
6. Open the native window with the startup status snapshot.
7. On window close: shut down the runtime, then the database worker.

Any failure in steps 1–5 is printed, logged where possible, and exits
the process with code 1. The program never runs with partial startup
state.

## 7. Testing

- `tests/paths_tests.rs` — data directory resolution, directory
  creation, unwritable-location failure, and the writability probe
  never overwriting an existing file and leaving no probe behind.
  Unit tests in `src/paths/mod.rs` also cover probe cleanup failure
  being reported.
- `tests/config_tests.rs` — default generation, loading an existing
  file, corrupt file reported and left untouched.
- `tests/storage_tests.rs` — first initialisation (schema version 1,
  WAL mode), repeat initialisation, typed meta writes with read-back
  through the worker, requests after shutdown fail. Unit tests in
  `src/storage/mod.rs` cover the meta round trip and the three
  `schema_version` error kinds (missing row, malformed value, failed
  query) by corrupting the database through a second connection that
  exists only inside those tests.
- `tests/runtime_tests.rs` — start, ping, shutdown, idempotent shutdown.
- Tests use unique temporary directories under the OS temp directory and
  never touch a real `DeACT_Data/`.

## 8. CI

`.github/workflows/ci.yml` runs on `windows-latest` for pushes to
`feature/**` and pull requests to `foundation/portable-core`:
`cargo fmt --check`, `cargo clippy --all-targets --all-features
-D warnings`, `cargo test --all-targets`, then a Windows x64 MSVC
release build. On success it uploads `deact.exe` as the
`DeACT-windows-x64` Actions artifact. No release or tag is created, no
secrets are used, and no user data is uploaded.

## 9. Not implemented (explicit boundary)

- No signal source (no Telegram, Discord, Webhook or other adapter) and
  no unified signal model implementation.
- No exchange adapter (no Hyperliquid, no Aster) and no trading code.
- No copy-trading engine, order management, risk controls, position
  reconciliation, or account authentication.
- No API key or credential storage, encryption, or management.
- No final UI design; the window is a plain status display.
- No single-EXE runtime-dependency certification. CI proves the project
  builds and its tests pass on Windows x64; it does not prove the final
  portable distribution.
- `DeACT_Data/cache/` and `DeACT_Data/backups/` are created but unused.
