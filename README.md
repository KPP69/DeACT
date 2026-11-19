# DeACT

DeACT is a portable Windows 11 x64 desktop auto copy-trading application.
This repository is at **Phase 1**: the Tauri 2 foundation (Rust backend,
Svelte + TypeScript frontend, portable data directory, configuration,
logging, SQLite, background task supervision, six-page UI skeleton,
Windows GUI startup error feedback). It contains **no trading
functionality yet** — no signal sources, no exchange adapters, no orders,
no real-money trading.

See `AGENTS.md` for the development rules, `docs/ARCHITECTURE.md` for the
target architecture and `docs/DEVELOPMENT_PLAN.md` for the phase plan.

## Portable layout

The first launch creates `data/` beside `DeACT.exe`:

```
DeACT.exe
data/
  deact.db            (SQLite, WAL mode; with deact.db-wal / deact.db-shm)
  config/deact.toml   (application configuration, non-sensitive only)
  logs/deact.log
  sessions/telegram/
  keys/
  cache/
  backups/
  webview/            (WebView2 user data)
```

Nothing is written to AppData, Documents, the user home directory or the
Registry, and there is no fallback: if the folder containing `DeACT.exe`
is not writable, DeACT shows a native error dialog and exits.

## Build (Windows x64, MSVC)

Requires Rust 1.100.0 (see `rust-toolchain.toml`) and Node.js 24 with npm.

```powershell
npm ci
npm run tauri -- icon src-tauri/icons/icon.svg
npm run tauri build -- --no-bundle
```

The icon set is generated from `src-tauri/icons/icon.svg` (only the SVG
source is kept in the repository). The build produces the portable
`src-tauri/target/release/deact.exe`; no installer is created.

## Development checks

```powershell
npm run build          # frontend production build
npm run check          # svelte-check + TypeScript
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

CI (`.github/workflows/ci.yml`) runs the same checks on Windows x64 and
uploads the bare executable as a GitHub Actions artifact, plus a separate
diagnostic artifact built with the `fault-injection` Cargo feature (used
only to verify the startup error dialogs on a real machine; it is never a
release build). No GitHub releases or tags are created from this phase.
