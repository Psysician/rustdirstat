# rustdirstat

Cross-platform disk usage analyzer with interactive treemap visualization. A Rust reimplementation of [WinDirStat](https://windirstat.net/).

**Features:**
- Parallel filesystem scanning via jwalk (utilizes all CPU cores)
- Interactive squarified treemap with cushion shading
- Three synchronized views: directory tree, treemap, extension statistics
- Duplicate file detection via SHA-256
- Cleanup actions: delete to recycle bin, open in file manager, custom commands
- CSV/JSON export of scan results
- Dark/light/system theme support

## Installation

### Pre-built Binaries

Download the latest release from [GitHub Releases](https://github.com/Psysician/rustdirstat/releases):

| Platform | Archive |
|----------|---------|
| Linux (x86_64) | `rustdirstat-vX.Y.Z-linux-x86_64.tar.gz` |
| macOS (Universal: Intel + Apple Silicon) | `rustdirstat-vX.Y.Z-macos-universal.tar.gz` |
| Windows (x86_64) | `rustdirstat-vX.Y.Z-windows-x86_64.zip` |

Extract the archive and place the `rustdirstat` binary somewhere in your `PATH`.

### From Source (cargo)

```bash
cargo install --git https://github.com/Psysician/rustdirstat
```

Requires Rust 1.85+ and platform-specific dependencies (see [Development](#development)).

### Build from Source

```bash
git clone https://github.com/Psysician/rustdirstat.git
cd rustdirstat
cargo build --release
# Binary: target/release/rustdirstat
```

## Usage

```bash
# Launch the GUI
rustdirstat

# Launch and immediately scan a directory
rustdirstat /path/to/scan

# Headless scan (print stats to stdout, no GUI)
rustdirstat --scan-only /path/to/scan
```

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` / `Cmd+O` | Open directory picker |
| `F5` | Rescan current directory |
| `Escape` | Close dialog / cancel scan / deselect |
| `Backspace` | Navigate up in treemap |

## Screenshots

<!-- TODO: Add screenshots after first release -->

## Crate Layout

| Crate | Purpose | Key constraint |
|-------|---------|----------------|
| `rds-core` | Shared data types | Zero deps beyond `serde` |
| `rds-scanner` | Parallel filesystem scan; streams ScanEvent over bounded channel | Uses `jwalk` + `rayon` |
| `rds-gui` | egui/eframe immediate-mode GUI + treemap | Uses `eframe`, `egui`, `streemap` |
| `rustdirstat` (root) | CLI entry point + eframe bootstrap | Uses `clap`, `eframe` |

## Architecture

### Tree representation

The file tree is an arena-allocated `Vec<FileNode>` where nodes reference
parent/children by `usize` index rather than `Rc`/`Box` pointers. This gives
cache-local traversal and zero reference-counting cost for potentially millions
of nodes.

### Scanner-to-GUI event flow

The scanner thread pushes `ScanEvent` values over a bounded
`crossbeam-channel`. The GUI drains the channel with `try_recv` on each frame
so it never blocks waiting for IO. Backpressure from the bounded channel
prevents the scanner from outrunning the GUI.

### Dependency management

All dependency versions are declared once in `[workspace.dependencies]` in the
root `Cargo.toml`. Individual crates opt in with `{ workspace = true }`.
This prevents version drift across the 4-crate workspace.

`Cargo.lock` is committed because `rustdirstat` is a binary, not a library.
Committing the lock file ensures reproducible builds on CI and developer
machines.

## Development

Requires Rust 1.85+ (edition 2024 minimum). Edition 2024 is used because
this is a greenfield project with no downstream consumers — no migration
cost is incurred, and the latest language features are available.

```
just build    # cargo build --workspace
just test     # cargo test --workspace
just lint     # cargo clippy --workspace -- -D warnings
just run      # cargo run
just fmt      # cargo fmt --all
```

Linux build dependencies:
```bash
sudo apt-get install -y \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libgtk-3-dev libatk1.0-dev
```

## CI

GitHub Actions runs on ubuntu, macos, and windows. The Linux job installs
X11/GTK system libraries required by eframe before building.

The CI pipeline enforces the `rds-core` zero-dependency invariant via
`cargo tree`: any dependency beyond `serde` causes the job to fail.

## Notes on `streemap`

`streemap` is declared in `rds-gui`'s dependencies alongside all other
workspace dependencies. Upfront declaration prevents Cargo.toml churn and
version drift. If a compatibility issue arises, vendoring or forking is
the fallback.

## License

[MIT](LICENSE)
