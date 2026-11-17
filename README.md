# DeACT

DeACT is a portable Windows 11 x64 desktop auto copy-trading application.
This repository is at the **foundation stage**: it builds a working
application shell (portable data directory, configuration, logging,
SQLite, background runtime, native window) and contains **no trading
functionality yet** — no signal sources, no exchange adapters, no orders,
no real-money trading.

See `AGENTS.md` for the development rules and `docs/ARCHITECTURE.md` for
the architecture actually implemented in this stage.

## Portable layout

The first launch creates `DeACT_Data/` beside `DeACT.exe`. All runtime
data (configuration, SQLite database, logs, cache, backups) lives there.
Nothing is written to AppData, Documents, the user home directory or the
Registry.

## Build (Windows x64, MSVC)

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

Requires Rust 1.100.0 (see `rust-toolchain.toml`).

## Development checks

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

CI (`.github/workflows/ci.yml`) runs the same checks on Windows x64 and
uploads `DeACT.exe` as a GitHub Actions artifact. No releases or tags are
created from this stage.
