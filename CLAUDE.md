# rustdirstat

Cross-platform disk usage analyzer. 4-crate Cargo workspace.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `Cargo.toml` | Workspace manifest; all pinned dependency versions in `[workspace.dependencies]` | Adding/updating dependencies, changing workspace members |
| `Cargo.lock` | Committed lock file (binary crate) | Debugging reproducibility issues |
| `justfile` | Developer task runner (build, test, lint, fmt, run, check, clean, bench, bench-report, bench-compare) | Running common dev commands |
| `.gitignore` | Ignores `/target`; Cargo.lock is NOT ignored (binary crate) | Checking what is excluded from git |
| `src/main.rs` | Binary entry point: CLI parsing (clap), tracing init with `RUST_LOG` env filter, config load/save (TOML via `directories`), eframe window launch (1024x768 default, 640x480 minimum) | Modifying CLI args, startup behaviour, window options, config persistence, tracing configuration |
| `README.md` | Architecture, design decisions, invariants | Understanding tree representation, scanner-GUI event flow, dependency strategy |

## Subdirectories

| Directory | What | When to read |
| --------- | ---- | ------------ |
| `crates/rds-core/` | Shared data types; zero deps beyond `serde` | Modifying core types, understanding arena tree layout |
| `crates/rds-scanner/` | Parallel filesystem traversal via `jwalk` with glob-based exclude filtering; streams `ScanEvent` over bounded channel; structured tracing spans | Implementing scan logic, modifying scanner-GUI communication, modifying exclude patterns |
| `crates/rds-gui/` | egui/eframe GUI: tree view, treemap, ext stats, duplicates, actions, export, settings, config persistence, recent paths, toast notifications, scan error log, max-nodes dialog | Implementing UI panels, modifying the eframe app struct, adding actions or settings |
| `.github/workflows/` | GitHub Actions CI (build/test/clippy/fmt on 3 platforms) | Modifying CI, debugging pipeline failures |
| `docs/` | Project-level documentation (milestones roadmap, `benchmarks.md` memory/performance audit) | Understanding planned feature scope, reviewing performance characteristics |
| `plans/` | Milestone implementation plans with decision logs | Reviewing design decisions, understanding why code is structured as it is |
| `scripts/` | Benchmark comparison script (`benchmark-comparison.sh` for hyperfine-based comparison against dust/dua) | Running external tool comparisons |

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
