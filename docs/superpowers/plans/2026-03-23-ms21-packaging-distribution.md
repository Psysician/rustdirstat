# MS21: Packaging & Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable users to install rustdirstat on any platform without building from source — by shipping pre-built binaries via GitHub Releases, adding an MIT license, embedding an app icon, enriching Cargo.toml package metadata, and updating the README with installation instructions and usage documentation.

**Architecture:** Five independent areas are modified: (1) a LICENSE file and Cargo.toml package metadata are added for legal and distribution compliance, (2) a GitHub Actions release workflow builds platform-specific binaries on tag push and publishes them to a GitHub Release, (3) an app icon is generated, committed, and embedded in the viewport via `egui::IconData`, (4) the README is expanded with installation instructions, usage examples, and a screenshot section, (5) the justfile gets a `release-build` recipe. No existing runtime behavior changes. The release workflow builds natively on each platform (Linux, macOS, Windows) rather than cross-compiling, because eframe requires platform-specific system libraries.

**Tech Stack:** GitHub Actions (`softprops/action-gh-release@v2` for release creation), `image` crate (PNG decode for icon loading — already a transitive dep of eframe), `lipo` (macOS universal binary from aarch64 + x86_64), `include_bytes!` (compile-time icon embedding). No new runtime dependencies beyond `image`.

**Prerequisite:** MS20 must be complete. Cross-platform polish (theme support, keyboard shortcuts, HiDPI) must be in place before the first public release.

---

## Decision Log

| ID | Decision | Reasoning Chain |
|---|---|---|
| DL-001 | Native GitHub Actions matrix builds, not `cross` | eframe on Linux requires X11/GTK system libraries (`libxcb-render`, `libxkbcommon`, `libgtk-3`, `libatk`). `cross` uses Docker containers that don't have these libraries pre-configured for GUI crates. Building natively on each platform via GitHub Actions runners (ubuntu-latest, macos-latest, windows-latest) guarantees the correct system libraries are available. The existing CI workflow already demonstrates this pattern. |
| DL-002 | Tag-triggered releases (`v*`) with `softprops/action-gh-release@v2` | The standard Rust project pattern: push a semver tag (`v0.1.0`), GitHub Actions builds binaries, creates a Release with auto-generated release notes. `softprops/action-gh-release` is the most widely used release action (35k+ stars), handles artifact upload, and supports `generate_release_notes: true` for automatic changelog from commits. |
| DL-003 | macOS universal binary via `lipo` instead of separate arch binaries | Modern macOS users run both Intel and Apple Silicon. Shipping a single universal binary (`rustdirstat-macos-universal.tar.gz`) eliminates confusion about which binary to download. `lipo -create` combines the aarch64 and x86_64 binaries into one Mach-O universal binary. Cross-compiling x86_64 on an M-series runner works because Xcode includes both architecture toolchains and macOS system frameworks are universal. |
| DL-004 | MIT license as specified in milestone | MIT is the most permissive common open source license. It's the standard choice for Rust CLI tools (used by ripgrep, fd, bat, dust, dua). The milestone explicitly specifies MIT. |
| DL-005 | Add `image` crate as direct dependency for icon loading | `image` is already a transitive dependency of eframe (via `egui-wgpu`), so adding it as a direct dependency incurs zero additional compile time. Using `image::load_from_memory()` to decode the embedded PNG to RGBA is the standard approach for Rust apps. Alternative: raw RGBA const array — but 256*256*4 = 256KB of source code is impractical. |
| DL-006 | Icon embedded via `include_bytes!` at compile time, not loaded from filesystem | `include_bytes!("../assets/icon-256.png")` bakes the icon into the binary. No filesystem lookup at runtime, no missing-icon errors, no path resolution issues across platforms. The 256x256 PNG is ~5-15KB compressed — negligible binary size impact. |
| DL-007 | Icon is a treemap-style colored block pattern | A treemap visualization is the app's defining visual feature. Using colored blocks in a squarified layout as the icon creates instant visual recognition. The icon is generated via a Python script (`scripts/generate-icon.py`) using Pillow — run once, commit the result. The script is not a runtime dependency; it's a one-time asset generation tool. |
| DL-008 | No crates.io publish in this milestone | Publishing to crates.io requires account setup, API token management, CI secret configuration, and careful `[package]` field validation. This is a separate workflow from GitHub Releases. The README instructs users to use `cargo install --git` for source installation. crates.io publishing is deferred to a follow-up task after the first release is validated. |
| DL-009 | Archive format: `.tar.gz` for Unix, `.zip` for Windows | Follows platform conventions. Linux/macOS users expect tarballs; Windows users expect zip files. The archive contains only the binary — no installer, no directory structure. Users extract and place the binary wherever they want. |
| DL-010 | Version extracted from `GITHUB_REF_NAME` tag for artifact naming | Artifact names follow `rustdirstat-{tag}-{target}.{ext}` (e.g., `rustdirstat-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`). The tag is the single source of truth for the version in the release workflow. Cargo.toml `version` should match but is not enforced automatically — a manual check step in the workflow verifies consistency. |
| DL-011 | README install instructions cover cargo-from-git and binary download; brew/winget/AUR noted as future | Creating Homebrew formulas, winget manifests, and AUR PKGBUILDs requires the first release to be published (for download URLs and checksums). These package definitions live in separate repositories (Homebrew tap, AUR, winget-pkgs). The README documents the intent and provides build-from-source instructions as a universal fallback. |
| DL-012 | Linux release tarball includes only the binary, no `.desktop` file | A `.desktop` file requires an icon installed to a system-specific location (`/usr/share/icons/`), which varies by distro and desktop environment. This complexity belongs in distro-specific packaging (deb, rpm, AUR), not in a generic tarball. Users who want desktop integration can create their own `.desktop` file — the README provides the binary path. |

### Rejected Alternatives

| Alternative | Why Rejected |
|---|---|
| `cross` for cross-compilation | eframe requires platform-specific GUI libraries (X11/GTK on Linux, Cocoa on macOS, Win32 on Windows). `cross` Docker containers are optimized for headless Rust builds and lack these libraries. Native CI builds on each platform are simpler and more reliable. |
| `cargo-dist` for release automation | `cargo-dist` is a higher-level tool that generates release workflows. While powerful, it adds an opaque abstraction layer over GitHub Actions. For a project that already has a working CI pipeline, a handwritten release workflow is more maintainable and easier to debug. |
| Separate macOS aarch64 and x86_64 binaries | Users on macOS would need to know their architecture to download the correct binary. A universal binary eliminates this friction and is the standard for macOS software distribution. The build cost is building twice on one runner (~3 min extra). |
| `.msi` installer for Windows | An MSI installer adds complexity (WiX toolset, signing certificates) for minimal benefit. The target audience (developers, sysadmins) is comfortable with extracting a `.zip` and adding the binary to PATH. An installer can be added in a future milestone if user demand warrants it. |
| `.dmg` for macOS | Creating a proper `.dmg` with drag-to-Applications requires macOS-specific tooling (`create-dmg`), code signing, and notarization. A `.tar.gz` with the universal binary is sufficient for the initial release. |
| App icon via build script (`build.rs`) | A `build.rs` that generates/converts the icon adds build-time complexity and requires the `image` crate at build time. `include_bytes!` with a pre-committed PNG is simpler and the decode cost at startup is negligible (~1ms). |
| Embed icon as Windows `.ico` resource | Windows `.ico` embedding requires a `windows_resource` build script and `.rc` file. While this would show the icon in File Explorer, the eframe viewport icon (shown in taskbar and title bar) is the higher-priority visual. `.ico` resource embedding can be added later for File Explorer integration. |
| Publish to crates.io in this milestone | crates.io publishing requires: (1) account creation, (2) API token as CI secret, (3) careful `[package]` metadata including `include`/`exclude` fields, (4) testing with `cargo publish --dry-run`, (5) understanding that publishes are permanent. This is a separate workflow that should be validated after the first GitHub Release proves the packaging is correct. |

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `LICENSE` | MIT license text with copyright holder and year. Required for open-source distribution. |
| `.github/workflows/release.yml` | GitHub Actions workflow triggered on `v*` tag push. Builds release binaries on Linux (x86_64), macOS (universal aarch64+x86_64), and Windows (x86_64). Packages into platform-appropriate archives and uploads to a GitHub Release with auto-generated release notes. |
| `assets/icon-256.png` | 256x256 RGBA PNG icon depicting a treemap-style colored block pattern. Committed binary asset — generated once by `scripts/generate-icon.py`, then checked into the repo. Loaded at compile time via `include_bytes!`. |
| `scripts/generate-icon.py` | One-time Python/Pillow script that generates the treemap-style app icon. Not a runtime dependency — run once to produce `assets/icon-256.png`, then the PNG is committed. Included in the repo for reproducibility (if the icon needs regeneration or modification). |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace root) | Add package metadata fields: `license`, `description`, `repository`, `homepage`, `keywords`, `categories`, `readme`. Add `image` to `[workspace.dependencies]` with version pin. |
| `Cargo.toml` (binary `[dependencies]`) | Add `image = { workspace = true }` to the binary crate's dependencies. |
| `src/main.rs` | Load the embedded icon PNG via `include_bytes!` + `image::load_from_memory()`, convert to `egui::IconData`, and pass to `ViewportBuilder::with_icon()`. |
| `README.md` | Add sections: project description with feature highlights, installation instructions (pre-built binaries, cargo install from git, build from source), usage (CLI flags, basic workflow), screenshot placeholder, license section. |
| `justfile` | Add `release-build` recipe for local optimized builds. |

---

## Task 1: Add MIT License File

**Files:**
- Create: `LICENSE`

- [ ] **Step 1: Create the LICENSE file**

Create `LICENSE` at the repo root with the MIT license text:

```text
MIT License

Copyright (c) 2026 rustdirstat contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 2: Commit**

```bash
git add LICENSE
git commit -m "chore: add MIT license"
```

---

## Task 2: Add Package Metadata to Cargo.toml

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add package metadata fields**

In the `[package]` section of the root `Cargo.toml` (after `edition = "2024"`), add the following fields:

```toml
license = "MIT"
description = "Cross-platform disk usage analyzer with interactive treemap visualization"
repository = "https://github.com/Psysician/rustdirstat"
homepage = "https://github.com/Psysician/rustdirstat"
keywords = ["disk-usage", "treemap", "filesystem", "analyzer", "windirstat"]
categories = ["command-line-utilities", "filesystem"]
readme = "README.md"
```

- [ ] **Step 2: Add `image` to workspace dependencies**

Add `image` to the `[workspace.dependencies]` section:

```toml
image = "0.25"
```

This is already a transitive dependency of eframe, so it adds zero compile time.

- [ ] **Step 3: Add `image` to binary crate dependencies**

In the `[dependencies]` section of the root `Cargo.toml` (the binary crate), add:

```toml
image = { workspace = true }
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build --workspace`
Expected: Clean build with no errors.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add package metadata and image dependency for icon loading"
```

---

## Task 3: Create App Icon and Embed in Viewport

**Files:**
- Create: `assets/icon-256.png`
- Create: `scripts/generate-icon.py`
- Modify: `src/main.rs`

- [ ] **Step 1: Create the `assets/` directory**

```bash
mkdir -p assets
```

- [ ] **Step 2: Create the icon generation script**

Create `scripts/generate-icon.py`:

```python
#!/usr/bin/env python3
"""Generate a treemap-style app icon for rustdirstat.

Requires: pip install Pillow
Usage: python scripts/generate-icon.py
Output: assets/icon-256.png
"""
from PIL import Image, ImageDraw

SIZE = 256
PAD = 16
CORNER = 6

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# Draw rounded background
draw.rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=32, fill=(38, 38, 38))

inner = SIZE - 2 * PAD
# Treemap-style colored blocks (relative coords within inner area)
blocks = [
    (0.00, 0.00, 0.47, 0.47, (76, 175, 80)),   # green — large directory
    (0.50, 0.00, 0.50, 0.32, (33, 150, 243)),   # blue
    (0.50, 0.35, 0.50, 0.12, (255, 152, 0)),    # orange
    (0.00, 0.50, 0.68, 0.50, (156, 39, 176)),   # purple — large directory
    (0.71, 0.50, 0.29, 0.27, (244, 67, 54)),    # red
    (0.71, 0.80, 0.29, 0.20, (0, 188, 212)),    # teal
]

for rx, ry, rw, rh, color in blocks:
    x1 = PAD + int(rx * inner)
    y1 = PAD + int(ry * inner)
    x2 = PAD + int((rx + rw) * inner) - 2
    y2 = PAD + int((ry + rh) * inner) - 2
    draw.rounded_rectangle([x1, y1, x2, y2], radius=CORNER, fill=color)

img.save("assets/icon-256.png")
print("Generated assets/icon-256.png")
```

- [ ] **Step 3: Generate the icon**

Run: `python3 scripts/generate-icon.py`
Expected: `assets/icon-256.png` is created — a 256x256 PNG with colored blocks on a dark rounded background.

If Python/Pillow is not available, create any 256x256 PNG icon manually and save it as `assets/icon-256.png`. The code integration works with any valid PNG.

- [ ] **Step 4: Embed the icon in `main.rs`**

In `main.rs`, add the icon loading between the `load_config()` call and the `NativeOptions` construction. Replace the current `NativeOptions` block (lines 146-151):

Current code:
```rust
let native_options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default()
        .with_inner_size([1024.0, 768.0])
        .with_min_inner_size([640.0, 480.0]),
    ..Default::default()
};
```

Replace with:
```rust
let icon_image = image::load_from_memory(include_bytes!("../assets/icon-256.png"))
    .expect("embedded icon is valid PNG")
    .to_rgba8();
let (icon_w, icon_h) = icon_image.dimensions();
let icon_data = egui::IconData {
    rgba: icon_image.into_raw(),
    width: icon_w,
    height: icon_h,
};

let native_options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default()
        .with_inner_size([1024.0, 768.0])
        .with_min_inner_size([640.0, 480.0])
        .with_icon(Arc::new(icon_data)),
    ..Default::default()
};
```

Note: `Arc` is already imported (`use std::sync::Arc;`). Add `use image` is not needed — the `image` crate is used via its fully qualified path `image::load_from_memory`.

- [ ] **Step 5: Verify the app builds and runs**

Run: `cargo run`
Expected: The app window shows the treemap icon in the taskbar and title bar (behavior varies by platform — most visible on Windows and Linux).

- [ ] **Step 6: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass. The icon loading is in the `main()` function path, not in test paths.

- [ ] **Step 7: Commit**

```bash
git add assets/icon-256.png scripts/generate-icon.py src/main.rs
git commit -m "feat: add treemap app icon embedded in window viewport"
```

---

## Task 4: Create GitHub Actions Release Workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create the release workflow file**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build-release:
    name: Build (${{ matrix.name }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - name: linux-x86_64
            os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - name: macos-universal
            os: macos-latest
            target: universal
          - name: windows-x86_64
            os: windows-latest
            target: x86_64-pc-windows-msvc

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          key: release-${{ matrix.target }}

      # Linux: install X11/GTK system libraries required by eframe.
      - name: Install Linux system dependencies
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
            libxkbcommon-dev libgtk-3-dev libatk1.0-dev

      # macOS: add x86_64 target for universal binary.
      - name: Add x86_64 target (macOS)
        if: runner.os == 'macOS'
        run: rustup target add x86_64-apple-darwin

      # Verify Cargo.toml version matches the git tag.
      - name: Verify version consistency
        shell: bash
        run: |
          TAG="${GITHUB_REF_NAME#v}"
          CARGO_VER=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
          if [ "$TAG" != "$CARGO_VER" ]; then
            echo "ERROR: Tag v$TAG does not match Cargo.toml version $CARGO_VER"
            exit 1
          fi
          echo "Version $TAG verified"

      # Build: Linux and Windows (single target).
      - name: Build release binary
        if: matrix.target != 'universal'
        run: cargo build --release

      # Build: macOS universal binary (both architectures).
      - name: Build macOS universal binary
        if: matrix.target == 'universal'
        run: |
          cargo build --release --target aarch64-apple-darwin
          cargo build --release --target x86_64-apple-darwin
          lipo -create \
            target/aarch64-apple-darwin/release/rustdirstat \
            target/x86_64-apple-darwin/release/rustdirstat \
            -output target/release/rustdirstat

      # Package: Unix (.tar.gz)
      - name: Package (Unix)
        if: runner.os != 'Windows'
        run: |
          cd target/release
          tar czf ../../rustdirstat-${{ github.ref_name }}-${{ matrix.name }}.tar.gz rustdirstat
          cd ../..

      # Package: Windows (.zip)
      - name: Package (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: |
          Compress-Archive `
            -Path target\release\rustdirstat.exe `
            -DestinationPath rustdirstat-${{ github.ref_name }}-${{ matrix.name }}.zip

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: rustdirstat-${{ matrix.name }}
          path: rustdirstat-${{ github.ref_name }}-*

  create-release:
    name: Create GitHub Release
    needs: build-release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - name: Create Release
        uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: |
            rustdirstat-*.tar.gz
            rustdirstat-*.zip
```

- [ ] **Step 2: Verify the workflow YAML is valid**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" 2>/dev/null || echo "Install PyYAML to validate, or review manually"`

If PyYAML is not available, visually review the YAML structure for correct indentation and syntax.

- [ ] **Step 3: Verify CI workflow is unchanged**

Run: `cat .github/workflows/ci.yml | head -5`
Expected: The existing CI workflow is untouched. The release workflow is a separate file.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add GitHub Actions release workflow for cross-platform binaries"
```

---

## Task 5: Update README with Installation & Usage

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite the README with installation and usage sections**

Replace the entire `README.md` content. The new README preserves all existing technical content (Crate Layout, Architecture, Development, CI) and adds new sections at the top. The full replacement:

```markdown
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
```

- [ ] **Step 2: Verify the README renders correctly**

Run: `head -30 README.md`
Expected: The new header, feature list, and installation section are present.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: expand README with installation instructions and usage guide"
```

---

## Task 6: Add Release Build Recipe to justfile

**Files:**
- Modify: `justfile`

- [ ] **Step 1: Add `release-build` recipe**

Add the following recipe after the existing `run` recipe in the `justfile`:

```just
# Build optimized release binary
release-build:
    cargo build --release
```

- [ ] **Step 2: Verify the recipe works**

Run: `just release-build`
Expected: Release build completes. Binary at `target/release/rustdirstat`.

- [ ] **Step 3: Commit**

```bash
git add justfile
git commit -m "chore: add release-build recipe to justfile"
```

---

## Task 7: Run Full Verification Suite

**Files:**
- No file changes — verification only.

- [ ] **Step 1: Run the full test suite**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Check formatting**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: Build release binary**

Run: `cargo build --release`
Expected: Clean release build.

- [ ] **Step 5: Manual smoke test**

Run the release binary and perform these checks:
1. **App icon:** Launch `target/release/rustdirstat`. Verify the treemap icon appears in the taskbar/dock and window title bar. (On Linux, the icon appears in the taskbar. On Windows, it appears in the taskbar and window title bar. On macOS, it appears in the Dock.)
2. **Scan a directory:** Verify the scan completes successfully, all three panels display correctly, and theme/keyboard shortcuts from MS20 still work.
3. **CLI scan-only:** Run `target/release/rustdirstat --scan-only /tmp` and verify stats are printed.
4. **Verify release workflow locally:** Review `.github/workflows/release.yml` to confirm it references the correct binary name and archive formats.

---

## Task 8: Update CLAUDE.md Documentation

**Files:**
- Modify: `CLAUDE.md` (root)
- Modify: `docs/CLAUDE.md`
- Modify: `docs/ai-context/project-structure.md`
- Modify: `docs/ai-context/docs-overview.md`

- [ ] **Step 1: Update root CLAUDE.md**

In the root `CLAUDE.md`, add entries for the new files:

In the **Files** table, add:
```
| `LICENSE` | MIT license | Checking license terms |
```

In the **Subdirectories** table, add or update:
```
| `assets/` | App icon PNG | Modifying the app icon |
```

- [ ] **Step 2: Update docs/CLAUDE.md**

Add an entry for the release plans:
```
| `superpowers/plans/2026-03-23-ms21-packaging-distribution.md` | MS21 implementation plan: license, packaging, release workflow, app icon, README update |
```

- [ ] **Step 3: Update project-structure.md**

Add the new files to the project structure tree:
- `LICENSE` at the root
- `assets/icon-256.png` under a new `assets/` directory
- `.github/workflows/release.yml` alongside `ci.yml`
- `scripts/generate-icon.py` in the scripts directory

Update the Milestone Status table:
```
| 20 | Cross-Platform Polish | Done |
| 21 | Packaging & Distribution | Done |
```

- [ ] **Step 4: Update docs-overview.md**

Add entries for the new documentation:
- `LICENSE` in Tier 1
- `.github/workflows/release.yml` in the Cross-Reference Map under CI pipeline
- The MS21 plan in Tier 3

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md docs/CLAUDE.md docs/ai-context/project-structure.md \
       docs/ai-context/docs-overview.md
git commit -m "docs: update CLAUDE.md and ai-context files for MS21 packaging"
```
