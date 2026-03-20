# Plan

## Overview

The rustdirstat project has no code yet. A Cargo workspace with 4 crates, all dependencies, CI pipeline, and task runner is needed before any feature development can begin.

**Approach**: Create a complete Cargo workspace with workspace-level dependency inheritance. Each crate gets its manifest and placeholder lib.rs. The root binary opens an empty egui window via eframe. GitHub Actions CI validates builds and tests across ubuntu, macos, and windows. A justfile provides ergonomic developer commands.

### Cargo Workspace Crate Dependencies

[Diagram pending Technical Writer rendering: DIAG-001]

## Planning Context

### Decision Log

| ID | Decision | Reasoning Chain |
|---|---|---|
| DL-001 | Declare all workspace dependencies upfront in root Cargo.toml | MS1 spec requires all dependencies declared -> avoids Cargo.toml churn in later milestones -> compile-time cost is minor since deps are used in upcoming milestones |
| DL-002 | Use workspace dependency inheritance via [workspace.dependencies] | Cargo workspace inheritance (stable since Rust 1.64) centralizes version pinning -> crates reference deps with .workspace=true -> single source of truth for versions across 4 crates |
| DL-003 | Use justfile over Makefile for task runner | justfile is cross-platform and Rust-idiomatic -> spec says justfile or Makefile -> justfile supports platform-specific recipes natively and has simpler syntax |
| DL-004 | Use Rust 2024 edition | Latest stable edition as of project start -> provides access to all modern Rust features -> no backward-compatibility constraints on a greenfield project |
| DL-005 | Commit Cargo.lock for reproducible builds | Project produces a binary (not a library) -> Cargo.lock ensures deterministic dependency resolution across CI and developer machines -> remove from .gitignore |
| DL-006 | Use dtolnay/rust-toolchain action for CI | Well-maintained community action -> simpler than actions-rs/toolchain (deprecated) -> supports stable/nightly/MSRV matrix cleanly |
| DL-007 | rds-core depends only on serde with derive feature | Spec invariant: rds-core has zero dependencies beyond std and serde -> keeps core types lightweight, fast to compile, and independently testable -> all other deps belong to rds-scanner or rds-gui |
| DL-008 | Single milestone for entire MS1 scaffold | All scaffold files are interdependent (workspace Cargo.toml references crates, CI tests all crates, justfile wraps cargo commands) -> splitting into separate milestones would create artificial boundaries -> single milestone is the deployable unit |
| DL-009 | Use crossbeam-channel over std::sync::mpsc for scanner-to-GUI event streaming | crossbeam-channel provides bounded channels to apply backpressure when GUI falls behind -> try_recv is non-blocking so the GUI thread never stalls waiting for events -> std::sync::mpsc lacks bounded capacity and its try_recv ergonomics are weaker |
| DL-010 | Arena-allocated tree (Vec<FileNode> with index references) over Rc/Box tree | Vec<FileNode> with usize indices gives cache-local memory layout for tree traversal -> parent traversal is zero-cost (just an index lookup) -> avoids Rc overhead and reference counting on every clone -> aligns with data-oriented design for potentially millions of nodes |
| DL-011 | MSRV is Rust 1.85 (minimum for edition 2024), no explicit pin beyond edition requirement | DL-004 sets edition 2024 which requires Rust 1.85+ -> greenfield project with no downstream consumers -> pinning a specific MSRV floor adds maintenance burden without benefit -> CI uses stable toolchain which is always >= 1.85 |

### Rejected Alternatives

| Alternative | Why Rejected |
|---|---|
| Minimal dependencies per crate (only declare deps used in MS1) | MS1 spec explicitly requires all dependencies declared. Deferring would cause Cargo.toml churn in every subsequent milestone. (ref: DL-001) |
| Makefile instead of justfile | justfile is cross-platform and Rust-idiomatic. Make has platform-specific syntax issues on Windows and requires explicit .PHONY declarations. (ref: DL-003) |
| actions-rs/toolchain for CI | actions-rs/toolchain is deprecated and unmaintained. dtolnay/rust-toolchain is the actively maintained successor. (ref: DL-006) |
| Keep Cargo.lock in gitignore | Project is a binary, not a library. Committing Cargo.lock ensures reproducible builds across CI and developer machines. (ref: DL-005) |

### Constraints

- rds-core has zero dependencies beyond std and serde (spec invariant) [doc-derived]
- Cargo workspace with exactly 4 crates: rds-core, rds-scanner, rds-gui, and root binary [doc-derived]
- CI passes on ubuntu, macos, and windows [doc-derived]
- Binary opens an empty egui window via eframe [doc-derived]

### Known Risks

- **streemap crate (v0.1.0) may have limited API surface or compatibility issues with latest egui version**: Declare the dependency but do not use it in MS1. Actual integration happens in MS8 where incompatibility can be addressed by vendoring or forking.
- **eframe on Linux CI requires system libraries (libxcb, libgtk3) that may not be pre-installed on GitHub Actions ubuntu runners**: Add apt-get install step in CI workflow for Linux-specific system dependencies needed by eframe/egui.

## Invisible Knowledge

### System

rustdirstat is a 4-crate Cargo workspace. rds-core holds data types with zero deps beyond std+serde. rds-scanner handles parallel directory traversal via jwalk and duplicate detection via rayon+sha2, communicating with the GUI via crossbeam-channel. rds-gui renders an egui/eframe UI with treemap, tree view, and extension stats panels. The root binary wires CLI (clap), logging (tracing), and config (toml+directories) together and launches the GUI.

### Invariants

- rds-core depends only on std and serde -- all other deps belong to rds-scanner or rds-gui
- Workspace dependency versions are centralized in root Cargo.toml [workspace.dependencies] section
- Cargo.lock is committed (binary project, not library)
- CI matrix covers ubuntu-latest, macos-latest, windows-latest

### Tradeoffs

- All deps declared upfront (faster future milestones) over minimal deps (faster MS1 CI builds)
- justfile (cross-platform, Rust-idiomatic) over Makefile (more universally known)

## Milestones

### Milestone 1: Workspace Scaffold & CI

**Files**: Cargo.toml, src/main.rs, crates/rds-core/Cargo.toml, crates/rds-core/src/lib.rs, crates/rds-scanner/Cargo.toml, crates/rds-scanner/src/lib.rs, crates/rds-gui/Cargo.toml, crates/rds-gui/src/lib.rs, .github/workflows/ci.yml, justfile, .gitignore

**Requirements**:

- Cargo workspace with 4 members: root binary, rds-core, rds-scanner, rds-gui
- All spec dependencies declared in [workspace.dependencies] with pinned versions
- rds-core depends only on serde (derive feature)
- root binary opens an empty egui window titled rustdirstat
- GitHub Actions CI builds and tests on ubuntu, macos, and windows
- justfile provides build, test, lint, fmt, run, check, clean recipes
- Cargo.lock committed to the repository
- CI runs clippy with -D warnings and cargo fmt --check

**Acceptance Criteria**:

- cargo build --workspace succeeds on all 3 platforms
- cargo test --workspace succeeds on all 3 platforms
- cargo clippy --workspace -- -D warnings produces zero warnings
- cargo fmt --all -- --check reports no formatting issues
- cargo run opens an egui window titled rustdirstat and exits cleanly on close
- GitHub Actions CI workflow passes on push to main
- just build, just test, just lint, just fmt-check all succeed locally

**Tests**:

- rds-core: trivial unit test in lib.rs confirming crate compiles (default-derived)
- rds-scanner: trivial unit test in lib.rs confirming crate and all dependencies compile (default-derived)
- rds-gui: no unit tests per spec (GUI crate testing is manual)
- CI validates cargo build + cargo test + clippy + fmt across 3 platforms

#### Code Intent

- **CI-M-001-001** `Cargo.toml`: Workspace root manifest defining 4 workspace members (the root binary crate, crates/rds-core, crates/rds-scanner, crates/rds-gui). [workspace.dependencies] section declares all project dependencies with pinned versions: eframe, egui, jwalk, rayon, crossbeam-channel, sha2, clap (with derive feature), serde (with derive feature), serde_json, csv, streemap, trash, open, directories, toml, tracing, tracing-subscriber. [package] section for the root binary crate named rustdirstat. [dependencies] section for the binary references workspace deps: clap, eframe, rds-core (path), rds-gui (path), tracing, tracing-subscriber, directories, toml, serde. Edition 2024. (refs: DL-001, DL-002, DL-004)
- **CI-M-001-002** `src/main.rs`: Binary entry point. Parses CLI arguments via clap (a struct with optional path argument). Initializes tracing subscriber for logging. Launches an eframe native window with default options and a placeholder app struct from rds-gui. The window title is rustdirstat. The app struct implements eframe::App with an empty update method that renders nothing (or a minimal label). (refs: DL-001)
- **CI-M-001-003** `crates/rds-core/Cargo.toml`: Crate manifest for rds-core. Only dependency is serde with derive feature, referenced via workspace inheritance. Edition 2024. (refs: DL-002, DL-007)
- **CI-M-001-004** `crates/rds-core/src/lib.rs`: Library root for rds-core. Contains a placeholder public module comment. Exports nothing yet (empty lib with a trivial test that asserts true, confirming the crate compiles). (refs: DL-007)
- **CI-M-001-005** `crates/rds-scanner/Cargo.toml`: Crate manifest for rds-scanner. Dependencies via workspace inheritance: rds-core (path reference), jwalk, rayon, sha2, crossbeam-channel, tracing. Edition 2024. (refs: DL-002)
- **CI-M-001-006** `crates/rds-scanner/src/lib.rs`: Library root for rds-scanner. Contains a placeholder public module comment. Exports nothing yet (empty lib with a trivial test that asserts true, confirming the crate compiles and dependencies resolve). (refs: DL-001)
- **CI-M-001-007** `crates/rds-gui/Cargo.toml`: Crate manifest for rds-gui. Dependencies via workspace inheritance: rds-core (path reference), eframe, egui, streemap, crossbeam-channel, tracing. Edition 2024. (refs: DL-002)
- **CI-M-001-008** `crates/rds-gui/src/lib.rs`: Library root for rds-gui. Defines a public RustDirStatApp struct (empty fields for now). Implements eframe::App for RustDirStatApp with an update method that renders an empty egui CentralPanel. Exports RustDirStatApp as the public API. A constructor function (default or new) creates the struct. No tests needed per spec (GUI crate has no unit tests). (refs: DL-001)
- **CI-M-001-009** `.github/workflows/ci.yml`: GitHub Actions CI workflow triggered on push to main and all pull requests. Uses a matrix strategy with three OS targets: ubuntu-latest, macos-latest, windows-latest. Steps: checkout code, install Rust stable toolchain via dtolnay/rust-toolchain action, install Linux system dependencies on ubuntu runner (apt-get install for libxcb, libgtk3, and other libraries required by eframe/egui -- conditional on runner OS), cache cargo registry and target dir via Swatinem/rust-cache action, run cargo build --workspace, run cargo test --workspace, run cargo clippy --workspace -- -D warnings, verify rds-core zero-dep invariant by running cargo tree on rds-core and asserting only serde appears as a direct dependency (fail CI if any other non-std dependency is found). A separate formatting job runs cargo fmt --all -- --check on ubuntu only (no need for matrix). (refs: DL-006, DL-007)
- **CI-M-001-010** `justfile`: Task runner with recipes: build (cargo build --workspace), test (cargo test --workspace), lint (cargo clippy --workspace -- -D warnings), fmt (cargo fmt --all), fmt-check (cargo fmt --all -- --check), run (cargo run), check (cargo check --workspace), clean (cargo clean). Each recipe has a brief comment describing its purpose. Default recipe lists all available recipes. (refs: DL-003)
- **CI-M-001-011** `.gitignore`: Remove Cargo.lock from the gitignore list. Keep all other existing entries (/target, *.swp, *.swo, *~, .DS_Store, .idea/, .vscode/). (refs: DL-005)

#### Code Changes

**CC-M-001-001** (Cargo.toml) - implements CI-M-001-001

**Code:**

```diff
--- /dev/null
+++ b/Cargo.toml
@@ -0,0 +1,42 @@
+[workspace]
+members = ["crates/rds-core", "crates/rds-scanner", "crates/rds-gui"]
+resolver = "3"
+
+[workspace.dependencies]
+eframe = "0.33.3"
+egui = "0.33.3"
+jwalk = "0.8.1"
+rayon = "1.11.0"
+crossbeam-channel = "0.5.15"
+sha2 = "0.10.9"
+clap = { version = "4.6.0", features = ["derive"] }
+serde = { version = "1.0.228", features = ["derive"] }
+serde_json = "1.0.149"
+csv = "1.4.0"
+streemap = "0.1.0"
+trash = "5.2.5"
+open = "5.3.3"
+directories = "6.0.0"
+toml = "1.0.7"
+tracing = "0.1.44"
+tracing-subscriber = "0.3.23"
+rds-core = { path = "crates/rds-core" }
+rds-scanner = { path = "crates/rds-scanner" }
+rds-gui = { path = "crates/rds-gui" }
+
+[package]
+name = "rustdirstat"
+version = "0.1.0"
+edition = "2024"
+
+[dependencies]
+clap = { workspace = true }
+eframe = { workspace = true }
+egui = { workspace = true }
+rds-core = { workspace = true }
+rds-gui = { workspace = true }
+tracing = { workspace = true }
+tracing-subscriber = { workspace = true }
+directories = { workspace = true }
+toml = { workspace = true }
+serde = { workspace = true }
```

**Documentation:**

```diff
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,3 +1,7 @@
+# Workspace root. All crate versions and feature flags are pinned here;
+# individual crates opt-in with `{ workspace = true }`. (ref: DL-001, DL-002, DL-004)
+#
 [workspace]
 members = ["crates/rds-core", "crates/rds-scanner", "crates/rds-gui"]
 resolver = "3"
@@ -29,6 +33,8 @@
 rds-gui = { path = "crates/rds-gui" }

+# Binary crate. Owns CLI parsing and eframe bootstrap; delegates scanning
+# and rendering to rds-scanner and rds-gui respectively.
 [package]
 name = "rustdirstat"
 version = "0.1.0"

```


**CC-M-001-002** (src/main.rs) - implements CI-M-001-002

**Code:**

```diff
--- /dev/null
+++ b/src/main.rs
@@ -0,0 +1,27 @@
+use clap::Parser;
+use std::path::PathBuf;
+
+#[derive(Parser)]
+#[command(name = "rustdirstat", about = "Cross-platform disk usage analyzer")]
+struct Cli {
+    /// Path to scan
+    path: Option<PathBuf>,
+}
+
+fn main() -> eframe::Result {
+    let _cli = Cli::parse();
+
+    tracing_subscriber::fmt::init();
+
+    let native_options = eframe::NativeOptions {
+        viewport: egui::ViewportBuilder::default()
+            .with_inner_size([1024.0, 768.0]),
+        ..Default::default()
+    };
+
+    eframe::run_native(
+        "rustdirstat",
+        native_options,
+        Box::new(|_cc| Ok(Box::new(rds_gui::RustDirStatApp::default()))),
+    )
+}
```

**Documentation:**

```diff
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,5 @@
+//! Binary entry point. Parses CLI arguments, initialises tracing, and launches
+//! the eframe window. All scanning and rendering logic lives in rds-scanner and rds-gui.
 use clap::Parser;
 use std::path::PathBuf;

@@ -5,6 +7,7 @@
 #[derive(Parser)]
 #[command(name = "rustdirstat", about = "Cross-platform disk usage analyzer")]
+/// Command-line arguments. `path` is the root directory passed to the scanner.
 struct Cli {
     /// Path to scan
     path: Option<PathBuf>,
@@ -11,6 +14,9 @@
 }

+/// Initialises tracing, parses CLI args, and runs the native eframe event loop.
+/// Default window size is 1024x768; eframe enforces no minimum size, so this provides
+/// a usable starting layout for the treemap without requiring the user to resize first.
+/// Returns eframe::Result so OS-level window errors propagate to the process exit code.
 fn main() -> eframe::Result {

```


**CC-M-001-003** (crates/rds-core/Cargo.toml) - implements CI-M-001-003

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-core/Cargo.toml
@@ -0,0 +1,7 @@
+[package]
+name = "rds-core"
+version = "0.1.0"
+edition = "2024"
+
+[dependencies]
+serde = { workspace = true }
```

**Documentation:**

```diff
--- a/crates/rds-core/Cargo.toml
+++ b/crates/rds-core/Cargo.toml
@@ -1,3 +1,6 @@
+# rds-core: shared data types. Zero external dependencies beyond serde.
+# Keeps core types fast to compile and independently testable.
+#
 [package]
 name = "rds-core"
 version = "0.1.0"

```


**CC-M-001-004** (crates/rds-core/src/lib.rs) - implements CI-M-001-004

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-core/src/lib.rs
@@ -0,0 +1,8 @@
+#[cfg(test)]
+mod tests {
+    #[test]
+    fn crate_compiles() {
+        let result = 2 + 2;
+        assert_eq!(result, 4);
+    }
+}
```

**Documentation:**

```diff
--- a/crates/rds-core/src/lib.rs
+++ b/crates/rds-core/src/lib.rs
@@ -1,3 +1,11 @@
+//! Core data types shared across all crates.
+//!
+//! Depends only on `serde` beyond `std` so it compiles fast and tests run
+//! without pulling in IO or GUI dependencies.
+//!
+//! The file tree is represented as an arena-allocated `Vec<FileNode>` with
+//! `usize` index references rather than `Rc`/`Box` pointers, giving cache-local
+//! traversal and zero reference-counting overhead.
 #[cfg(test)]
 mod tests {

```


**CC-M-001-005** (crates/rds-scanner/Cargo.toml) - implements CI-M-001-005

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-scanner/Cargo.toml
@@ -0,0 +1,12 @@
+[package]
+name = "rds-scanner"
+version = "0.1.0"
+edition = "2024"
+
+[dependencies]
+rds-core = { workspace = true }
+jwalk = { workspace = true }
+rayon = { workspace = true }
+sha2 = { workspace = true }
+crossbeam-channel = { workspace = true }
+tracing = { workspace = true }

```

**Documentation:**

```diff
--- a/crates/rds-scanner/Cargo.toml
+++ b/crates/rds-scanner/Cargo.toml
@@ -1,3 +1,6 @@
+# rds-scanner: parallel filesystem traversal and hashing.
+# Uses jwalk for parallel directory walking and crossbeam-channel to stream
+# scan events to the GUI without blocking either thread.
 [package]
 name = "rds-scanner"
 version = "0.1.0"

```


**CC-M-001-006** (crates/rds-scanner/src/lib.rs) - implements CI-M-001-006

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-scanner/src/lib.rs
@@ -0,0 +1,8 @@
+#[cfg(test)]
+mod tests {
+    #[test]
+    fn crate_compiles() {
+        let result = 2 + 2;
+        assert_eq!(result, 4);
+    }
+}
```

**Documentation:**

```diff
--- a/crates/rds-scanner/src/lib.rs
+++ b/crates/rds-scanner/src/lib.rs
@@ -1,3 +1,10 @@
+//! Parallel filesystem scanner.
+//!
+//! Walks a directory tree using `jwalk` (parallel) and `rayon`, building a
+//! `Vec<FileNode>` arena in `rds-core`. Scan events are pushed over a bounded
+//! `crossbeam-channel` so the GUI thread can drain them without stalling on IO.
+//!
+//! SHA-2 hashing for duplicate detection is provided by `sha2`.
 #[cfg(test)]
 mod tests {

```


**CC-M-001-007** (crates/rds-gui/Cargo.toml) - implements CI-M-001-007

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-gui/Cargo.toml
@@ -0,0 +1,12 @@
+[package]
+name = "rds-gui"
+version = "0.1.0"
+edition = "2024"
+
+[dependencies]
+rds-core = { workspace = true }
+eframe = { workspace = true }
+egui = { workspace = true }
+streemap = { workspace = true }
+crossbeam-channel = { workspace = true }
+tracing = { workspace = true }

```

**Documentation:**

```diff
--- a/crates/rds-gui/Cargo.toml
+++ b/crates/rds-gui/Cargo.toml
@@ -1,3 +1,7 @@
+# rds-gui: egui/eframe immediate-mode GUI.
+# Receives scan events from rds-scanner via crossbeam-channel and renders
+# the treemap using the streemap crate. streemap is declared upfront alongside
+# all other dependencies to prevent version drift. (ref: DL-001)
 [package]
 name = "rds-gui"
 version = "0.1.0"

```


**CC-M-001-008** (crates/rds-gui/src/lib.rs) - implements CI-M-001-008

**Code:**

```diff
--- /dev/null
+++ b/crates/rds-gui/src/lib.rs
@@ -0,0 +1,15 @@
+#[derive(Default)]
+pub struct RustDirStatApp;
+
+impl RustDirStatApp {
+    pub fn new() -> Self {
+        Self
+    }
+}
+
+impl eframe::App for RustDirStatApp {
+    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
+        egui::CentralPanel::default().show(ctx, |_ui| {
+        });
+    }
+}

```

**Documentation:**

```diff
--- a/crates/rds-gui/src/lib.rs
+++ b/crates/rds-gui/src/lib.rs
@@ -1,3 +1,8 @@
+//! egui application shell. Implements `eframe::App` for the main window.
+//!
+//! `RustDirStatApp` satisfies the eframe contract with an empty central panel.
+//! Scan state, treemap rendering, and panel layout are separate concerns
+//! handled by other modules.

+/// Top-level application state. Passed to eframe as the `App` implementor.
 #[derive(Default)]
 pub struct RustDirStatApp;

@@ -8,6 +14,9 @@
 }

 impl eframe::App for RustDirStatApp {
+    /// Called by eframe on every frame. Renders the central panel.
+    /// `_frame` satisfies the required trait signature; no frame-level
+    /// operations are needed for the central-panel-only layout.
     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

```


**CC-M-001-009** (.github/workflows/ci.yml) - implements CI-M-001-009

**Code:**

```diff
--- /dev/null
+++ b/.github/workflows/ci.yml
@@ -0,0 +1,69 @@
+name: CI
+
+on:
+  push:
+    branches: [main]
+  pull_request:
+
+env:
+  CARGO_TERM_COLOR: always
+
+jobs:
+  build-and-test:
+    name: Build & Test (${{ matrix.os }})
+    runs-on: ${{ matrix.os }}
+    strategy:
+      fail-fast: false
+      matrix:
+        os: [ubuntu-latest, macos-latest, windows-latest]
+
+    steps:
+      - uses: actions/checkout@v4
+
+      - uses: dtolnay/rust-toolchain@stable
+        with:
+          components: clippy
+
+      - uses: Swatinem/rust-cache@v2
+
+      - name: Install Linux system dependencies
+        if: runner.os == 'Linux'
+        run: |
+          sudo apt-get update
+          sudo apt-get install -y \
+            libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
+            libxkbcommon-dev libgtk-3-dev libatk1.0-dev
+
+      - name: Build workspace
+        run: cargo build --workspace
+
+      - name: Run tests
+        run: cargo test --workspace
+
+      - name: Clippy
+        run: cargo clippy --workspace -- -D warnings
+
+      - name: Verify rds-core zero-dep invariant
+        shell: bash
+        run: |
+          cargo tree -p rds-core --depth 1 --edges normal --prefix none 2>/dev/null | tail -n +2 | awk '{print $1}' | sort -u | while read -r dep; do
+            if [ -n "$dep" ] && [ "$dep" != "serde" ]; then
+              echo "ERROR: rds-core has unexpected dependency: $dep"
+              echo "rds-core must only depend on serde (beyond std)"
+              exit 1
+            fi
+          done
+          echo "rds-core zero-dep invariant verified"
+
+  fmt:
+    name: Formatting
+    runs-on: ubuntu-latest
+    steps:
+      - uses: actions/checkout@v4
+
+      - uses: dtolnay/rust-toolchain@stable
+        with:
+          components: rustfmt
+
+      - name: Check formatting
+        run: cargo fmt --all -- --check
```

**Documentation:**

```diff
--- a/.github/workflows/ci.yml
+++ b/.github/workflows/ci.yml
@@ -1,3 +1,6 @@
+# CI runs build, test, clippy, and formatting checks on ubuntu, macos, and windows.
+# Uses dtolnay/rust-toolchain (actively maintained; actions-rs/toolchain is deprecated).
+#
 name: CI

 on:
@@ -27,6 +30,8 @@
       - uses: Swatinem/rust-cache@v2

+      # eframe on Linux requires X11/GTK system libraries not pre-installed on
+      # GitHub Actions ubuntu runners.
       - name: Install Linux system dependencies
         if: runner.os == 'Linux'
         run: |
@@ -38,6 +43,8 @@
             libxkbcommon-dev libgtk-3-dev libatk1.0-dev

       - name: Build workspace
+        # Builds all 4 crates (root binary + rds-core, rds-scanner, rds-gui).
         run: cargo build --workspace

       - name: Verify rds-core zero-dep invariant
+        # Enforces zero-dependency invariant: rds-core must not depend on anything beyond serde.
         shell: bash

```


**CC-M-001-010** (justfile) - implements CI-M-001-010

**Code:**

```diff
--- /dev/null
+++ b/justfile
@@ -0,0 +1,35 @@
+# List available recipes
+default:
+    @just --list
+
+# Build the entire workspace
+build:
+    cargo build --workspace
+
+# Run all tests
+test:
+    cargo test --workspace
+
+# Run clippy with warnings as errors
+lint:
+    cargo clippy --workspace -- -D warnings
+
+# Format all code
+fmt:
+    cargo fmt --all
+
+# Check formatting without modifying files
+fmt-check:
+    cargo fmt --all -- --check
+
+# Run the application
+run:
+    cargo run
+
+# Type-check the workspace
+check:
+    cargo check --workspace
+
+# Remove build artifacts
+clean:
+    cargo clean
```

**Documentation:**

```diff
--- a/justfile
+++ b/justfile
@@ -1,3 +1,5 @@
+# Task runner. Cross-platform alternative to Makefile; no tab-sensitivity,
+# simpler syntax, and native platform-specific recipe support. (ref: DL-003)
+
 # List available recipes
 default:

```


**CC-M-001-011** (.gitignore) - implements CI-M-001-011

**Code:**

```diff
--- a/.gitignore
+++ b/.gitignore
@@ -1,8 +1,7 @@
 /target
-Cargo.lock
 *.swp
 *.swo
 *~
 .DS_Store
 .idea/
 .vscode/

```

**Documentation:**

```diff
--- a/.gitignore
+++ b/.gitignore
@@ -1,3 +1,5 @@
+# Cargo.lock is committed because this is a binary crate. Committing ensures
+# reproducible builds across CI and developer machines.
+#

```


**CC-M-001-012** (README.md)

**Documentation:**

```diff
--- /dev/null
+++ b/README.md
@@ -0,0 +1,58 @@
+# rustdirstat
+
+Cross-platform disk usage analyzer. Rust reimplementation of WinDirStat.
+
+## Crate Layout
+
+| Crate | Purpose | Key constraint |
+|-------|---------|----------------|
+| `rds-core` | Shared data types | Zero deps beyond `serde` |
+| `rds-scanner` | Parallel filesystem scan + SHA-2 hashing | Uses `jwalk` + `rayon` |
+| `rds-gui` | egui/eframe immediate-mode GUI + treemap | Uses `eframe`, `egui`, `streemap` |
+| `rustdirstat` (root) | CLI entry point + eframe bootstrap | Uses `clap`, `eframe` |
+
+## Architecture
+
+### Tree representation
+
+The file tree is an arena-allocated `Vec<FileNode>` where nodes reference
+parent/children by `usize` index rather than `Rc`/`Box` pointers. This gives
+cache-local traversal and zero reference-counting cost for potentially millions
+of nodes.
+
+### Scanner-to-GUI event flow
+
+The scanner thread pushes `ScanEvent` values over a bounded
+`crossbeam-channel`. The GUI drains the channel with `try_recv` on each frame
+so it never blocks waiting for IO. Backpressure from the bounded channel
+prevents the scanner from outrunning the GUI.
+
+### Dependency management
+
+All dependency versions are declared once in `[workspace.dependencies]` in the
+root `Cargo.toml`. Individual crates opt in with `{ workspace = true }`.
+This prevents version drift across the 4-crate workspace. (ref: DL-001, DL-002)
+
+`Cargo.lock` is committed because `rustdirstat` is a binary, not a library.
+Committing the lock file ensures reproducible builds on CI and developer
+machines.
+
+## Development
+
+Requires Rust 1.85+ (edition 2024 minimum). Edition 2024 is used over 2021
+because greenfield projects incur no migration cost and gain the latest
+language features. (ref: DL-004)
+
+```
+just build    # cargo build --workspace
+just test     # cargo test --workspace
+just lint     # cargo clippy --workspace -- -D warnings
+just run      # cargo run
+just fmt      # cargo fmt --all
+```
+
+## CI
+
+GitHub Actions runs on ubuntu, macos, and windows. The Linux job installs
+X11/GTK system libraries required by eframe before building.
+
+The CI pipeline enforces the `rds-core` zero-dependency invariant via
+`cargo tree`: any dependency beyond `serde` causes the job to fail.
+
+## Notes on `streemap`
+
+`streemap` is declared in `rds-gui`'s dependencies alongside all other
+workspace dependencies. Upfront declaration prevents Cargo.toml churn and
+version drift. If a compatibility issue arises, vendoring or forking is
+the fallback. (ref: DL-001)

```

