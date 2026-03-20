# rustdirstat

Cross-platform disk usage analyzer. 4-crate Cargo workspace.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Workspace manifest; all pinned dependency versions in `[workspace.dependencies]` | Adding/updating dependencies, changing workspace members |
| `Cargo.lock` | Committed lock file (binary crate) | Debugging reproducibility issues |
| `justfile` | Developer task runner (build, test, lint, fmt, run, check, clean) | Running common dev commands |
| `.gitignore` | Ignores `/target`; Cargo.lock is NOT ignored (binary crate) | Checking what is excluded from git |
| `src/main.rs` | Binary entry point: CLI parsing (clap), tracing init, eframe window launch | Modifying CLI args, startup behaviour, window options |
| `README.md` | Architecture, design decisions, invariants | Understanding tree representation, scanner-GUI event flow, dependency strategy |

## Subdirectories

| Directory | What | When to read |
| --------- | ---- | ------------ |
| `crates/rds-core/` | Shared data types; zero deps beyond `serde` | Modifying core types, understanding arena tree layout |
| `crates/rds-scanner/` | Filesystem traversal via `walkdir`; streams `ScanEvent` over bounded channel to receiver | Implementing scan logic, modifying scanner-GUI communication |
| `crates/rds-gui/` | egui/eframe immediate-mode GUI and treemap shell | Implementing UI panels, modifying the eframe app struct |
| `.github/workflows/` | GitHub Actions CI (build/test/clippy/fmt on 3 platforms) | Modifying CI, debugging pipeline failures |
| `docs/` | Project-level documentation (milestones roadmap) | Understanding planned feature scope |
| `plans/` | Milestone implementation plans with decision logs | Reviewing design decisions, understanding why code is structured as it is |

## Build

```
just build    # cargo build --workspace
just run      # cargo run
```

## Test

```
just test     # cargo test --workspace
just lint     # cargo clippy --workspace -- -D warnings
just fmt-check
```

## Development

Requires Rust 1.85+ (edition 2024 minimum).
