# DeACT — Agent Engineering Rules

These rules apply to every AI agent and contributor. This file is the long-term development specification for DeACT. It is not an architecture design document and it is not a development plan: stage tasks are managed by the development plan, and architecture details are managed by architecture documentation.

## 1. Product

- DeACT is a portable desktop auto copy-trading application for Windows 11 x64.
- It connects to multiple external trading signal sources, receives, identifies and parses trading signals, converts signals from different sources into one unified standard model, and executes copy-trading decisions according to user-configured strategies.
- It executes trades through multiple exchange adapters, manages the order lifecycle, and synchronises target positions with actual positions.
- It must provide risk control, state recovery and trading audit.
- DeACT must not be designed to serve only one messaging platform or one exchange.
- Product principles, in priority order: stability and reliability, accurate trading state, data security, smooth operation, simple operation, excellent performance, clear architecture, easy maintenance, easy extension, and no unnecessary complexity.
- No technology or component may be introduced only to pursue technical novelty.

## 2. Roles and responsibilities

- **ChatGPT — technical decision-making and review.** ChatGPT is responsible for technical research, technology selection, overall architecture design, core module and interface specifications, data models and business rules, concrete development instructions, and review of code, architecture and test results. ChatGPT identifies problems and defines the required fixes.
- **Muse — executor and reporter.** Muse is responsible for implementing development according to the confirmed technical solution, writing and modifying code, testing and debugging, fixing problems, running builds and release verification, managing GitHub branches, commits and pull requests, running CI through GitHub Actions, and reporting actual completion and test results honestly.
- **User — product owner and final approver.** The user is responsible for product requirements, important product decisions, final acceptance and approval of a formal release.
- Technical research and overall architecture decisions must not be pushed to Muse. Muse must not decide the technical direction on its own, and must not change a confirmed core architecture without confirmation.
- Muse must not fabricate completion, test results or build results. Completed, partially completed, not completed and test-failed work must be distinguished clearly.
- If a technical solution cannot be implemented or has a major problem, Muse must provide concrete evidence and request guidance from ChatGPT.

## 3. Architecture

- One modular, single-process, monolithic desktop application. No microservices, Docker, Kubernetes, Redis, Kafka, NATS or local web server without a documented and approved requirement.
- Core business logic must not depend on the UI.
- Core: Rust. Tokio for asynchronous network work.
- Async runtime: application background tasks run on the async runtime provided by Tauri (Tokio). Do not create a second, independent Tokio runtime by default.
- UI: Tauri 2 desktop shell with a Svelte + TypeScript frontend and plain CSS. On Windows, Tauri renders through the system WebView2 Runtime; DeACT accepts the WebView2 Runtime present on the system and does not bundle it. The WebView2 user data folder is application data and must be stored under `data/webview/` (see §9).
- Persistence: SQLite. Use one controlled write path; WAL does not make SQLite multi-writer.
- Modules must have clear responsibilities and interfaces. Use small cohesive modules; no speculative abstraction and no generic plugin system.
- The core copy-trading logic must not depend on any specific signal source or on any specific exchange API implementation.
- Network: REST for commands and snapshots, and WebSocket for streaming where supported. Reconnect with bounded backoff and reconcile state after reconnect.
- Do not block the UI thread with network or database work.
- Do not block Tokio worker threads with synchronous database or file I/O. Use a dedicated database worker or `spawn_blocking`.
- Use bounded channels with explicit overload behaviour. Never silently drop signals, fills or order transitions.
- Prefer maintained, pinned crates and official API documentation. Keep dependencies minimal: justify every new dependency and review its maintenance status and license. Avoid Git dependencies unless necessary; pin an immutable revision and explain why.
- Commit `Cargo.lock` for this application. Review dependency updates deliberately.

## 4. Signal sources

- DeACT must use an independent signal-source adapter architecture. Every signal source connects through its own adapter.
- Telegram is only the first signal source. It is not the core architecture of the system.
- Future sources must be extensible without rewriting the copy-trading core, including Discord, Webhook, web pages and other external channels.
- Each adapter is responsible for connection and authentication, signal reception, message identification, source information extraction, a unique message identifier, time information extraction, reconnection, error handling and preservation of the raw content.
- All signal sources must share one unified signal-processing flow.
- A signal-source adapter must never place orders directly and must never operate a copy-trading account directly.
- The copy-trading engine must not depend on Telegram-specific code.
- Duplicate, out-of-order, delayed and invalid signals must be handled.
- A signal whose trading intent cannot be determined must never be executed automatically.

## 5. Unified signal model

- Every signal source must be converted to one unified standard signal model. Source-specific information may be kept in extension fields, but must not break the unified core model.
- The unified model must at least contain: signal ID, source type, source instance ID, trader ID, original message ID, original message content, exchange, trading instrument/symbol, direction, action, quantity, quantity semantics, target position or position delta, leverage, entry price/price conditions, take profit, stop loss, source timestamp, receive timestamp, and parse status.
- `action` covers at least Open, Close, Add, Reduce, LeverageChange and Cancel.
- Quantity semantics must state explicitly what a quantity means (base amount, quote amount, balance fraction or position fraction); it must never be inferred implicitly.
- Target position and position delta are different trading instructions and must never be conflated; a signal must state explicitly which of the two it uses.
- Not every signal needs every field, but every field's semantics are fixed.
- A message whose trading intent cannot be determined must be recorded with a failed or ambiguous parse status, must never be executed automatically, and must remain available for manual review.
- A signal-source adapter must never place orders directly and must never operate a copy-trading account directly (see §4).

## 6. Exchange adapters

- Exchanges must connect through independent adapters. The initial target exchanges are Hyperliquid and Aster. Other exchanges may be added in the future.
- Each exchange adapter is responsible for account queries, trading rules, order submission, order queries, order cancellation, fill queries, position queries and real-time state synchronisation.
- Exchange integrations must implement a small shared interface but preserve exchange-specific semantics.
- The copy-trading core must not depend on a specific exchange API implementation.

## 7. Trading pipeline, state and safety

- The complete trading flow must be: signal reception → signal standardisation → signal processing → copy-trading strategy → risk control → target position calculation → order execution → fill tracking → position synchronisation → reconciliation.
- The following must be implemented: signal deduplication, trading idempotency (including idempotency keys), order lifecycle management, target position management, actual position management, partial fill handling, timeout status queries, disconnection recovery, position reconciliation, abnormal state handling and operation audit.
- Persist order intent before submitting the order to the exchange.
- A network timeout means the outcome is unknown, not that the order failed. Query exchange state before retrying. Never retry blindly before confirming the state of the original order.
- Track order lifecycle and fills separately; partial fills are first-class.
- Reconcile local orders and positions after startup and after reconnect. If state is uncertain, pause new exposure.
- On startup the program must check the database, recover unfinished orders and reconcile with the actual exchange state. Until recovery and reconciliation are complete, no new automatic positions are permitted.
- Early stages use offline/mock or clearly labelled paper mode. No live-money orders.
- Real-money automatic trading must remain off until the user explicitly approves it.
- Risk checks run in the backend immediately before submission.
- Never claim profitability or guaranteed execution.

## 8. Risk controls

- All automatic trading must pass risk checks. The checks must at least cover: maximum amount per order, maximum position per trading pair, maximum total account position, maximum leverage, maximum slippage, minimum margin, maximum daily loss, maximum signal delay, API anomaly circuit breaker, and a global emergency stop.
- The emergency stop by default prohibits new positions and position increases.
- Cancelling orders and closing positions are separate, explicit operations; they are not part of the emergency stop's prohibition on new exposure.

## 9. Portable delivery and storage

- The final product must be a portable Windows 11 x64 single executable: the initial delivery directory contains only `DeACT.exe`.
- On first launch, `data/` must be created automatically beside the executable.
- All runtime data must be stored under `data/`, including the database, configuration, account information, API keys, Telegram Session, other signal-source authentication information, logs, cache, backups and recovery state.
- The database is `data/deact.db`; its SQLite WAL/SHM files stay beside it under `data/`.
- Backups must stay under `data/backups` and must cover the database, the encrypted key files under `data/keys/`, Telegram Sessions and the other runtime data needed to restore the application. A SQLite backup must use a method that is consistent with WAL state; copying the main database file alone is not a valid backup. Database migrations are versioned; back up before migration.
- Resolve the data root from the executable path, never from the current working directory. Centralise paths in one portable-paths module and inject paths into services.
- Never write program data to AppData, LocalAppData, Documents, the Registry or any other system directory. Never silently fall back to any of those directories.
- If the executable directory is not writable, report the problem clearly and exit. Never fall back silently to another directory.
- Disable or redirect framework-managed persistence (including the WebView2 user data folder) explicitly into `data/`.
- Updates replace the executable only; never overwrite `data/`.
- The source tree is not included in the end-user package.
- No telemetry, crash upload, cloud sync or external data storage by default.
- Portability and runtime dependencies must be verified by actual build and run, not assumed.

## 10. Security

- Never put real API keys, Telegram credentials, Telegram Sessions, wallet secrets or user data in Git, Issues, pull requests, Actions logs, artifacts or test fixtures.
- Never use real credentials in CI.
- Never submit a real API key or a Telegram Session.
- Sensitive information must be protected locally by reasonable measures.
- Do not log credentials or raw message contents by default. Use structured logs with secret redaction, and never leak sensitive information in logs.
- Never upload user data to CI.

## 11. Development and testing

- Develop in small, phased steps with continuous testing. Every feature must include the necessary tests.
- Testing must cover normal flows, abnormal flows, boundary conditions and fault recovery, with priority on parsing, sizing, state transitions, idempotency and risk rules.
- Use deterministic mock exchanges. Real funds must never be used for development testing.
- Test failures must be analysed, fixed and re-verified. Never manufacture a passing result by skipping tests or lowering assertion standards.
- Typed errors must be used at module boundaries; do not swallow errors.
- Avoid `unwrap`/`expect` in production paths unless a documented invariant justifies it.
- Before a pull request: `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test`.

## 12. GitHub workflow

- All code development, modification, building and testing go through the GitHub workflow. Use feature branches and pull requests for important changes. Do not push directly to `main` unless explicitly requested.
- Pull requests must state scope, design decisions, tests, limitations and data/schema impact.
- Use GitHub Actions for Windows x64 builds and automated tests.
- Development test builds are provided only through GitHub Actions Artifacts. They are temporary test builds.
- Do not create a formal GitHub Release and do not create a formal release Tag until all functionality is complete, final acceptance has passed and the user has explicitly approved the release.
- Use least-privilege workflow permissions. Never put secrets in workflow YAML.

## 13. Reporting

- Each phased delivery must report: (1) task objective; (2) actual completed work; (3) modified files; (4) key implementation notes; (5) test execution; (6) real test results; (7) build results; (8) GitHub commit records; (9) unresolved issues; (10) next-stage tasks.
- Completed, partially completed, not completed and test-failed work must be distinguished clearly.
