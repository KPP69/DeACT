# DeACT — Agent Engineering Rules

These rules apply to every AI agent and contributor.

## Product target
- Windows 11 x64 first.
- End-user distribution: one DeACT.exe. First launch creates DeACT_Data beside the executable.
- Development builds are GitHub Actions artifacts only. Do not create GitHub Releases or version tags until all release gates pass and the owner explicitly approves the first release.
- All mutable app data (database, config, Telegram sessions, logs, cache, backups, recovery state) must remain under DeACT_Data.
- Resolve the data root from the executable path, never the current working directory.
- Never silently fall back to AppData, LocalAppData, Documents, Registry, cloud storage, or another directory. If the executable directory is not writable, explain the problem and exit safely.
- The source tree is not included in the end-user package.

## Architecture constraints
- One modular desktop process. No microservices, Docker, Kubernetes, Redis, Kafka, NATS, or local web server without a documented and approved requirement.
- UI: eframe/egui, pure Rust native desktop; no browser/WebView runtime.
- Core: Rust. Tokio for asynchronous network work.
- Persistence: SQLite. Use one controlled write path; WAL does not make SQLite multi-writer.
- Do not block the UI thread with network or database work.
- Network: REST for commands/snapshots and WebSocket for streaming where supported. Reconnect with bounded backoff and reconcile after reconnect.
- Exchange integrations implement a small shared interface but preserve exchange-specific semantics.
- Prefer maintained, pinned crates and official API documentation. Avoid Git dependencies unless necessary; pin an immutable revision and explain why.
- Keep dependencies minimal. Justify every new dependency and review maintenance and license.

## Trading safety invariants
- Early stages use offline/mock or clearly labeled paper mode. No live-money orders.
- Never put API keys, Telegram credentials, sessions, wallet secrets, or user data in Git, Issues, PRs, Actions logs, artifacts, or test fixtures.
- Never use real credentials in CI.
- Persist order intent before submission.
- Timeout means unknown outcome, not failure. Query exchange state before retrying.
- Deduplicate source messages and use idempotency keys.
- Track order lifecycle and fills separately; partial fills are first-class.
- Reconcile local orders/positions after startup and reconnect. If state is uncertain, pause new exposure.
- Emergency stop prevents new orders and increases. Closing positions is a separate explicit action.
- Risk checks run in the backend immediately before submission.
- Never claim profitability or guaranteed execution.

## Portable storage
- Centralize paths in one portable-paths module and inject paths into services.
- Database and SQLite WAL/SHM files stay under DeACT_Data/database.
- Disable or redirect framework-managed persistence explicitly.
- No telemetry, crash upload, cloud sync, or external data storage by default.
- Backups stay under DeACT_Data/backups and must be consistent with SQLite state.
- Updates replace the executable only; never overwrite DeACT_Data.
- Database migrations are versioned; back up before migration.

## Code quality
- Small cohesive modules; no speculative abstraction or generic plugin system.
- Typed errors at module boundaries; do not swallow errors.
- Avoid unwrap/expect in production paths unless an invariant is documented.
- Do not block Tokio worker threads with synchronous DB/file I/O. Use a dedicated DB worker or spawn_blocking.
- Use bounded channels with explicit overload behavior. Never silently drop signals, fills, or order transitions.
- Structured logs with secret redaction. Do not log credentials or raw message contents by default.
- Test parsing, sizing, state transitions, idempotency and risk rules. Use deterministic mock exchanges.
- Before PR: cargo fmt --all -- --check; cargo clippy --all-targets -- -D warnings; cargo test.
- Commit Cargo.lock for this application. Review dependency updates deliberately.

## GitHub workflow
- Work in focused branches and PRs. Do not push directly to main unless explicitly requested.
- PRs state scope, design decisions, tests, limitations and data/schema impact.
- CI artifacts are temporary test builds. No Release/tag for incomplete stages.
- Least-privilege workflow permissions. Never put secrets in workflow YAML.
- Release only after complete acceptance and owner approval.

## Current stage
Stage 0/1: repository foundation, architecture decisions, portable-runtime contract and Windows CI. Do not implement live trading in this stage.
