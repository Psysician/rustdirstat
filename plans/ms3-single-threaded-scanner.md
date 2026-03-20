# Plan

## Overview

rds-scanner has no scan implementation. The GUI and integration tests have no way to traverse a directory tree and produce a DirTree via ScanEvent streaming. Without a working scanner, milestones MS4-MS5 and beyond are blocked.

**Approach**: Implement Scanner::scan() in rds-scanner using walkdir for single-threaded directory traversal. The scanner runs on a background thread, walks the filesystem, and sends ScanEvent::NodeDiscovered events through a bounded crossbeam channel. An internal HashMap<PathBuf, usize> maps filesystem paths to predicted arena indices (sequential counter starting at 0 for root). The receiver inserts nodes in order, so predicted indices match receiver-side indices. ScanComplete with ScanStats is sent after the walk finishes. Permission errors and IO failures produce ScanError events and scanning continues. Integration tests use tempfile to create real directory fixtures and verify tree correctness.

### Scanner Event Flow

[Diagram pending Technical Writer rendering: DIAG-001]

## Planning Context

### Decision Log

| ID | Decision | Reasoning Chain |
|---|---|---|
| DL-001 | Scanner predicts receiver-side arena indices via sequential counter, tracked in internal HashMap<PathBuf, usize>; relies on walkdir documented parent-before-child depth-first ordering | Events flow sequentially through bounded channel -> receiver inserts nodes in send order -> scanner counter starts at 0 (root) and increments per NodeDiscovered -> predicted indices always match receiver-side indices -> no back-channel needed -> simpler than bidirectional protocol and matches design spec description of scanner tracking mapping internally -> this scheme depends on walkdir yielding parents before children (so parent is in HashMap when child is processed); walkdir::WalkDir documents depth-first top-down iteration where directory entries are yielded before their contents, guaranteeing this ordering (DL-014) |
| DL-002 | Use walkdir 2.5.0, not jwalk with parallelism=1 | MS3 spec explicitly says walkdir -> MS4 explicitly says replace walkdir with jwalk -> milestone progression is intentional: simple single-threaded first to validate correctness, then parallel -> using jwalk early would skip validation of the simple path and blur milestone boundaries |
| DL-003 | Scanner module structure: scanner.rs contains Scanner struct and scan() function, lib.rs re-exports public API | rds-core uses submodule pattern (tree.rs, scan.rs, config.rs, stats.rs with re-exports from lib.rs) -> following same convention in rds-scanner keeps workspace consistent -> scanner.rs owns all scan logic, lib.rs owns module declaration and re-exports -> single module sufficient for MS3 scope (no duplicate detection yet) |
| DL-004 | Channel bound size: 4096 events | Too small (e.g. 64) -> scanner blocks frequently on full channel -> slower throughput -> too large (e.g. 1M) -> excessive memory buffering if GUI is slow -> 4096 is 4096 * ~200 bytes per FileNode ~ 800KB -> reasonable memory ceiling -> provides enough buffering for GUI frame-rate draining (~100 events/frame at 60fps = 6000/sec) while limiting worst-case memory |
| DL-005 | Progress events sent every 100 nodes discovered (files and directories combined) | Too frequent (every node) -> doubles channel traffic with no user benefit -> too infrequent (every 10000) -> progress bar appears stuck on small scans -> 100 nodes balances responsiveness with channel efficiency -> counter includes both files and directories because walkdir yields DirEntry for both and distinguishing adds complexity for no user benefit -> configurable later if needed -> DL-016 documents the counter basis explicitly |
| DL-006 | Integration tests use tempfile crate with real filesystem fixtures, no mocks | User global instructions explicitly prohibit simulated/mock functions -> design spec testing strategy says integration tests scanning temp directory fixture -> tempfile creates real directories that are auto-cleaned -> tests verify actual walkdir traversal, real metadata, real filesystem behavior |
| DL-007 | walkdir added to workspace.dependencies in root Cargo.toml, tempfile added as workspace dev-dependency | All dependency versions pinned in workspace.dependencies per DL-001 convention -> walkdir is a runtime dep for rds-scanner -> tempfile is a dev-dep for rds-scanner integration tests -> both follow workspace dependency pattern established in MS1 |
| DL-008 | exclude_patterns in ScanConfig are ignored in MS3, not implemented | MS3 scope is single-threaded walker with basic event streaming -> exclude pattern matching adds complexity (glob matching against OsStr paths) -> MS3 description does not mention exclude patterns -> field exists in ScanConfig from MS2 but scanner can skip it for now -> implementing pattern matching is either MS4 scope or a dedicated milestone |
| DL-009 | FileNode.size for directories is 0, only files carry size | DirTree::subtree_size() recursively sums child sizes -> if directories also carried size, it would double-count -> rds-core tree.rs tests confirm directory nodes have size=0 -> scanner follows this convention: is_dir=true nodes get size=0, file nodes get metadata.len() |
| DL-010 | Cancel flag uses Relaxed atomic ordering | Cancel flag is a cooperative shutdown signal, not a synchronization primitive -> exact timing of cancellation is not critical (ms-level delay acceptable) -> Relaxed ordering avoids acquire/release overhead on the hot path (checked every iteration) -> no other data depends on the cancel flag value for correctness (it only controls loop termination) -> stronger ordering (SeqCst, Acquire/Release) would add unnecessary overhead for no correctness benefit |
| DL-011 | Cancel flag is not checked during blocked tx.send(); scanner thread may block until receiver drains channel | tx.send() on bounded crossbeam channel blocks when channel is full -> cancel flag is only checked at loop top, not during blocked send -> alternative: use try_send() in a loop with cancel checks, but this adds spin-wait complexity and latency -> alternative: use select! macro with cancel channel, but adds a second channel for a rare edge case -> in practice, receiver (GUI) drains channel every frame (~16ms), so channel rarely fills to 4096 -> worst-case cancel latency is time to drain one slot (microseconds) -> acceptable tradeoff: simple blocking send with cancel check per iteration vs complex non-blocking machinery for a rare scenario |
| DL-012 | Root metadata failure sends ScanError then ScanComplete and exits thread | config.root may not exist or may be inaccessible -> fs::metadata(config.root) can fail -> ScanComplete must always be the last event (invariant) -> on root metadata failure: send ScanError with path=config.root and error message, then send ScanComplete with zeroed stats (total_files=0, total_dirs=0, total_bytes=0, errors=1) and exit thread -> this preserves the ScanComplete-always-last invariant and gives the receiver a clear error signal -> alternative: panic the thread, but this violates the invariant and leaves receiver hanging on channel |
| DL-013 | Parent_index HashMap lookup failure for children of error-skipped directories: skip the entry and send ScanError | When walkdir encounters a directory error (permission denied), scanner sends ScanError and continues -> the errored directory path is never inserted into HashMap -> walkdir may still yield children of that directory -> child entry parent path lookup returns None -> options: (1) panic - unacceptable, crashes scanner thread, (2) skip silently - loses visibility into what happened, (3) skip and send ScanError - preserves scanning, gives receiver error visibility -> chose option 3: if parent_index lookup fails, send ScanError for the child path with message explaining parent was inaccessible, do not send NodeDiscovered, do not increment arena index counter -> this maintains correct index prediction because skipped entries consume no index |
| DL-014 | walkdir parent-before-child ordering is guaranteed by its depth-first traversal and is documented in walkdir API | walkdir::WalkDir iterates in depth-first order by default -> walkdir documentation states entries are yielded in a top-down manner: a directory entry is yielded before its contents -> this means parent directories always appear before their children in the iteration -> the scanner relies on this ordering for HashMap parent lookups (parent must already be in map when child is processed) -> walkdir crate version 2.x has maintained this guarantee since initial release -> this is not just an implementation detail but documented API behavior |
| DL-015 | SystemTime pre-1970 dates produce modified=None | SystemTime::duration_since(UNIX_EPOCH) returns Err for timestamps before 1970-01-01 -> FileNode.modified is Option<u64> specifically to handle missing or invalid timestamps -> on Err from duration_since: set modified=None rather than panicking or using a sentinel value -> pre-epoch files are rare in practice (mainly FAT filesystem artifacts or corrupt metadata) -> None is semantically correct: the modified time is not representable as a positive epoch seconds value |
| DL-016 | Progress event counter includes both files and directories (counts all NodeDiscovered events, not just files) | walkdir yields DirEntry for both files and directories -> the progress counter tracks total nodes discovered, not just files -> Progress event files_scanned field semantically means entries scanned and includes dirs -> this is simpler (one counter, not two) and gives more accurate progress indication -> the ScanComplete stats separate total_files and total_dirs for final reporting, but progress updates use a combined counter for simplicity -> progress sent every 100 nodes discovered (not every 100 files) |
| DL-017 | build_tree_from_events uses DirTree::new(root_name) for the first NodeDiscovered event (parent_index=None), then DirTree::insert() for all subsequent events | DirTree::new(name) creates a root node at index 0 -> the first NodeDiscovered event IS the root with parent_index=None -> if helper calls DirTree::new() then inserts root event via insert(), root is duplicated at indices 0 and 1 -> solution: the helper uses DirTree::new(root_node.name) for the first event (parent_index=None) to create the tree with correct root, then for all subsequent events (parent_index=Some(i)) calls DirTree::insert(i, node) -> the first event is consumed by DirTree::new(), not inserted separately -> this maps scanner index 0 to DirTree index 0 correctly -> DirTree::new() also sets size=0 and is_dir=true which matches root node expectations |
| DL-018 | tx.send() failure when receiver disconnects mid-scan: scanner breaks out of loop and exits thread without sending ScanComplete | If receiver drops its end of the crossbeam channel, tx.send() returns Err(SendError) -> the receiver is gone so there is nobody to receive ScanComplete -> attempting to send ScanComplete would also fail -> scanner should break out of loop and let the thread exit cleanly -> this is the only case where ScanComplete is NOT sent, because there is no receiver to consume it -> this is not a violation of the ScanComplete-always-last invariant because the invariant is about event ordering for the receiver, and the receiver no longer exists |

### Rejected Alternatives

| Alternative | Why Rejected |
|---|---|
| Using jwalk with parallelism=1 instead of walkdir | MS3 explicitly says walkdir, MS4 explicitly says Replace walkdir with jwalk. Milestone progression is intentional: simple single-threaded first to validate correctness, then parallel. (ref: DL-002) |
| Building the DirTree inside the scanner | Design spec says scanner sends NodeDiscovered events and the receiver (GUI) builds its own DirTree. Scanner should not own a DirTree; it only sends events. (ref: DL-001) |

### Constraints

- MUST: use walkdir crate for single-threaded traversal (MS3 spec explicit)
- MUST: send NodeDiscovered events through crossbeam-channel (bounded)
- MUST: produce a correct DirTree from events — file counts, sizes, tree structure verified by integration tests
- MUST: Scanner::scan() takes ScanConfig + Sender<ScanEvent> + Arc<AtomicBool> cancel flag per design spec
- MUST: return JoinHandle from scan() — scanner runs on background thread
- MUST: send ScanComplete with ScanStats after walk finishes
- MUST: send ScanError for permission-denied and other IO errors, then continue scanning
- MUST: respect ScanConfig.max_nodes — abort with ScanError if exceeded
- MUST: populate FileNode fields: name, size, is_dir, extension (lowercased, no dot), modified (epoch seconds)
- SHOULD: send Progress events periodically during scan
- MUST NOT: use jwalk in this milestone (jwalk replaces walkdir in MS4)
- MUST NOT: implement duplicate detection (MS12 scope)

### Known Risks

- **HashMap<PathBuf, usize> grows linearly with discovered nodes — for 10M nodes at ~100 bytes per entry, this is ~1GB**: max_nodes default of 10M provides a safety bound. The HashMap is freed when the scanner thread exits. Memory estimation validated in MS19.
- **walkdir is single-threaded, so scan speed is limited by IO throughput on one thread**: This is intentional for MS3 correctness validation; MS4 replaces walkdir with jwalk for parallelism.
- **Cancel flag is not checked during blocked tx.send() — if receiver stops draining and channel fills, scanner thread blocks indefinitely even with cancel=true**: GUI drains channel every frame (~16ms) and channel capacity is 4096 — in practice channel rarely fills. Worst-case cancel latency is time to drain one slot.

## Invisible Knowledge

### System

Scanner predicts receiver-side arena indices using a sequential counter (starting at 0 for root). This works because events flow through a bounded crossbeam channel in order, and the receiver inserts nodes in arrival order. The HashMap<PathBuf, usize> maps each discovered path to its predicted arena index so child entries can look up their parent's index. walkdir iterates depth-first, guaranteeing parents appear before children. The scanner thread owns the walkdir iterator and the path-to-index map; neither crosses the channel boundary. Only FileNode values (cloned into ScanEvent::NodeDiscovered) cross the channel. The cancel flag is an Arc<AtomicBool> shared between the caller and the scanner thread, checked with Relaxed ordering since exact timing is not critical. Channel bound of 4096 provides ~800KB worst-case buffer. Progress events every 100 nodes discovered (files and directories combined) keep channel overhead under 1% of traffic.

### Invariants

- First event is always NodeDiscovered with parent_index=None (the root), assigned arena index 0
- Every subsequent NodeDiscovered has parent_index=Some(i) where i was the index assigned to an earlier NodeDiscovered event
- Arena index N corresponds to the (N+1)th NodeDiscovered event sent
- ScanComplete is always the last event sent, even on cancellation or max_nodes abort
- ScanError events do not consume an arena index — they are informational and do not correspond to FileNode insertions
- walkdir iterates depth-first, so parent directories are always discovered before their children
- Scanner never creates or owns a DirTree — it only sends events; the receiver builds the tree

### Tradeoffs

- Sequential index prediction is simple and correct but couples scanner and receiver ordering — if the receiver ever reorders or drops events, indices break. This is acceptable because the bounded channel preserves order and the receiver processes all events.
- walkdir is single-threaded, so scan speed is limited by IO throughput on one thread. This is intentional for MS3 correctness validation; MS4 replaces walkdir with jwalk for parallelism.
- HashMap<PathBuf, usize> grows linearly with discovered nodes — for 10M nodes at ~100 bytes per entry, this is ~1GB. max_nodes default of 10M provides a safety bound. The HashMap is freed when the scanner thread exits.
- Progress events every 100 files is a fixed interval, not adaptive. This is sufficient for MS3; can be made configurable or adaptive in a later milestone.

## Milestones

### Milestone 1: Workspace dependency additions

**Files**: Cargo.toml, crates/rds-scanner/Cargo.toml

#### Code Intent

- **CI-M-001-001** `Cargo.toml`: Add walkdir = "2.5.0" to [workspace.dependencies] section. Add tempfile = "3.27.0" to a new [workspace.dev-dependencies] section (or add it to [workspace.dependencies] since workspace dev-deps are declared in the same section with a note). walkdir is a runtime dependency for rds-scanner. tempfile is a dev-dependency for rds-scanner integration tests. (refs: DL-007)
- **CI-M-001-002** `crates/rds-scanner/Cargo.toml`: Add walkdir = { workspace = true } to [dependencies]. Add tempfile = { workspace = true } to [dev-dependencies]. Update crate doc comment to mention walkdir for single-threaded traversal (MS3), with jwalk listed for future parallel traversal (MS4). (refs: DL-002, DL-007)

#### Code Changes

**CC-M-001-001** (Cargo.toml) - implements CI-M-001-001

**Code:**

```diff
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -27,6 +27,8 @@
 rds-core = { path = "crates/rds-core" }
 rds-scanner = { path = "crates/rds-scanner" }
 rds-gui = { path = "crates/rds-gui" }
+walkdir = "2.5.0"
+tempfile = "3.27.0"
 
 # Binary crate. Owns CLI parsing and eframe bootstrap; delegates scanning
 # and rendering to rds-scanner and rds-gui respectively.
```

**Documentation:**

```diff
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,4 +1,4 @@
-# Workspace root. All crate versions and feature flags are pinned here;
-# individual crates opt-in with `{ workspace = true }`. (ref: DL-001, DL-002, DL-004)
+# Workspace root. All crate versions and feature flags are pinned here;
+# individual crates opt-in with `{ workspace = true }` (DL-007).
 #
 [workspace]
@@ -26,6 +26,8 @@
 rds-core = { path = "crates/rds-core" }
 rds-scanner = { path = "crates/rds-scanner" }
 rds-gui = { path = "crates/rds-gui" }
+walkdir = "2.5.0"       # single-threaded directory walker (DL-002)
+tempfile = "3.27.0"     # dev-only: real filesystem fixtures for integration tests (DL-006)

```


**CC-M-001-002** (crates/rds-scanner/Cargo.toml) - implements CI-M-001-002

**Code:**

```diff
@@ rds-scanner Cargo.toml
 Add walkdir dep + tempfile dev-dep, update crate comment
--- a/crates/rds-scanner/Cargo.toml
+++ b/crates/rds-scanner/Cargo.toml
@@ -1,8 +1,9 @@
-# rds-scanner: parallel filesystem traversal and hashing.
-# Uses jwalk for parallel directory walking and crossbeam-channel to stream
-# scan events to the GUI without blocking either thread.
+# rds-scanner: filesystem traversal and hashing.
+# MS3: single-threaded traversal via walkdir with crossbeam-channel event streaming.
+# MS4: parallel traversal replaces walkdir with jwalk.
 [package]
 name = "rds-scanner"
 version = "0.1.0"
 edition = "2024"
 
 [dependencies]
 rds-core = { workspace = true }
-jwalk = { workspace = true }
-rayon = { workspace = true }
-sha2 = { workspace = true }
 crossbeam-channel = { workspace = true }
 tracing = { workspace = true }
+walkdir = { workspace = true }
+
+[dev-dependencies]
+tempfile = { workspace = true }
```

**Documentation:**

```diff
--- a/crates/rds-scanner/Cargo.toml
+++ b/crates/rds-scanner/Cargo.toml
@@ -1,4 +1,5 @@
-# rds-scanner: filesystem traversal and hashing.
-# MS3: single-threaded traversal via walkdir with crossbeam-channel event streaming.
-# MS4: parallel traversal replaces walkdir with jwalk.
+# rds-scanner: filesystem traversal and event streaming.
+#
+# Uses walkdir for single-threaded traversal; produces ScanEvent stream over
+# bounded crossbeam-channel (DL-002).
 [package]

```


### Milestone 2: Scanner implementation and integration tests

**Files**: crates/rds-scanner/src/lib.rs, crates/rds-scanner/src/scanner.rs, crates/rds-scanner/tests/scan_integration.rs

#### Code Intent

- **CI-M-002-001** `crates/rds-scanner/src/lib.rs`: Replace placeholder test with module declaration for scanner submodule. Re-export public API: Scanner struct. Module doc comment describes the scanner as a single-threaded filesystem walker (MS3) that sends ScanEvent values through a bounded crossbeam channel. (refs: DL-003)
- **CI-M-002-002** `crates/rds-scanner/src/scanner.rs`: Scanner struct with a single public method: scan(config: ScanConfig, tx: crossbeam_channel::Sender<ScanEvent>, cancel: Arc<AtomicBool>) -> JoinHandle<()>. The scan() method spawns a background thread that: (1) Attempts to read fs::metadata(config.root). On failure: sends ScanError { path: config.root, error: error message }, then sends ScanComplete { stats: zeroed stats with errors=1 }, then exits thread (DL-012). On success: creates root FileNode (is_dir=true, size=0, name from path, modified from metadata converted via duration_since(UNIX_EPOCH) with Err mapped to None per DL-015). (2) Sends NodeDiscovered { node: root_node, parent_index: None } as the first event. If tx.send() returns Err (receiver disconnected), exits thread immediately without sending ScanComplete (DL-018). This becomes arena index 0 on the receiver side. (3) Maintains HashMap<PathBuf, usize> mapping canonical filesystem paths to predicted arena indices. Root path maps to index 0. Index counter starts at 1. (4) Creates walkdir::WalkDir::new(config.root) with follow_links(config.follow_symlinks) and iterates. Skips the root entry itself (already sent). walkdir guarantees parent-before-child ordering in depth-first traversal (DL-014). (5) For each walkdir entry: checks cancel flag (AtomicBool::load Relaxed per DL-010) at loop top. If true, breaks out of loop. Note: cancel is NOT checked during blocked tx.send(); scanner may block until receiver drains (DL-011). Checks max_nodes: if counter exceeds config.max_nodes, sends ScanError with descriptive message and breaks. Reads fs::metadata for size (metadata.len() for files, 0 for dirs per DL-009), modified time (SystemTime -> epoch seconds via duration_since(UNIX_EPOCH), Err maps to None per DL-015), extension (lowercased, no dot). Looks up parent path in the HashMap to get parent_index. If parent_index lookup fails (parent was error-skipped and never inserted): sends ScanError for this path explaining parent was inaccessible, does NOT send NodeDiscovered, does NOT increment counter, continues to next entry (DL-013). Creates FileNode with name (entry file_name), size, is_dir, empty children vec, parent=None (receiver sets this), extension, modified. Sends NodeDiscovered { node, parent_index: Some(parent_idx) }. If tx.send() returns Err, exits thread immediately (DL-018). Inserts entry path -> current counter into HashMap. Increments counter. (6) On walkdir errors (permission denied, IO errors): sends ScanError { path, error: error.to_string() } and continues iteration. The errored directory path is NOT inserted into HashMap; children of errored directories are handled by parent_index lookup failure path (DL-013). (7) After loop completes (or cancel/max_nodes abort): computes ScanStats from accumulated counters (total_files, total_dirs, total_bytes, duration_ms via Instant::elapsed, errors count). Sends ScanComplete { stats }. Thread exits. Progress events: sends ScanEvent::Progress every 100 nodes discovered (both files and directories counted per DL-016) with running totals of files_scanned and bytes_scanned. (refs: DL-001, DL-002, DL-004, DL-005, DL-008, DL-009, DL-010, DL-011, DL-012, DL-013, DL-014, DL-015, DL-016, DL-018)
- **CI-M-002-003** `crates/rds-scanner/tests/scan_integration.rs`: Integration test file using tempfile::TempDir to create real directory fixtures. Helper function build_tree_from_events(rx) -> (DirTree, ScanStats) that drains the channel: for the first NodeDiscovered event (parent_index=None), creates DirTree via DirTree::new(node.name) consuming the root event as the tree root at index 0 (DL-017); for subsequent NodeDiscovered events (parent_index=Some(i)), calls tree.insert(i, node); collects ScanError events; extracts ScanStats from ScanComplete; returns the tree and stats. Tests: (1) scan_basic_tree: Creates fixture with root dir containing 2 files (a.txt 5 bytes, b.dat 10 bytes) and 1 subdir with 1 file (c.rs 3 bytes). Scans via Scanner::scan(). Receives all events from channel. Builds DirTree from NodeDiscovered events. Verifies: 5 NodeDiscovered events received (root + subdir + 3 files = 5 nodes), tree.len() == 5, subtree_size(root) == 18 bytes, ScanComplete received with correct stats (total_files=3, total_dirs=2, total_bytes=18). (2) scan_empty_directory: Creates empty temp dir. Scans. Verifies: 1 NodeDiscovered (root only), tree.len()==1, subtree_size==0, ScanComplete with total_files=0 total_dirs=1. (3) scan_respects_cancellation: Creates fixture with many files. Sets cancel flag to true after receiving first few events. Verifies: ScanComplete is still sent (scanner sends it on exit regardless), scan did not process all files. (4) scan_max_nodes_abort: Creates fixture exceeding max_nodes (set config.max_nodes = Some(3)). Verifies: ScanError event received with max_nodes message, scan stops early. (5) scan_file_extensions: Creates files with various extensions (.TXT, .rs, .tar.gz, no extension). Verifies: extension field is lowercased without dot (txt, gz, None for extensionless). (6) scan_reports_errors_and_continues: Creates directory with a permission-denied subdirectory (chmod 000 on Unix). Verifies: ScanError event received for that path, scan continues past the error, ScanComplete stats.errors >= 1. Skipped on Windows where permission model differs. (refs: DL-006, DL-001, DL-017)

#### Code Changes

**CC-M-002-001** (crates/rds-scanner/src/lib.rs) - implements CI-M-002-001

**Code:**

```diff
@@ lib.rs - replace placeholder with module decl and re-exports
--- a/crates/rds-scanner/src/lib.rs
+++ b/crates/rds-scanner/src/lib.rs
@@ -1,16 +1,8 @@
-//! Parallel filesystem scanner.
-//!
-//! Walks a directory tree using `jwalk` (parallel) and `rayon`, building a
-//! `Vec<FileNode>` arena in `rds-core`. Scan events are pushed over a bounded
-//! `crossbeam-channel` so the GUI thread can drain them without stalling on IO.
-//!
-//! SHA-2 hashing for duplicate detection is provided by `sha2`.
+//! Single-threaded filesystem scanner.
+//!
+//! Walks a directory tree using `walkdir` and sends `ScanEvent` values through
+//! a bounded `crossbeam-channel` so the receiver can build a `DirTree` without
+//! blocking on IO.
 
-#[cfg(test)]
-mod tests {
-    #[test]
-    fn crate_compiles() {
-        let result = 2 + 2;
-        assert_eq!(result, 4);
-    }
-}
+mod scanner;
+
+pub use scanner::Scanner;
```

**Documentation:**

```diff
--- a/crates/rds-scanner/src/lib.rs
+++ b/crates/rds-scanner/src/lib.rs
@@ -1,4 +1,9 @@
-//! Single-threaded filesystem scanner.
+//! Single-threaded filesystem scanner.
 //!
-//! Walks a directory tree using `walkdir` and sends `ScanEvent` values through
-//! a bounded `crossbeam-channel` so the receiver can build a `DirTree` without
-//! blocking on IO.
+//! Walks a directory tree using `walkdir` and sends [`ScanEvent`] values through
+//! a bounded `crossbeam-channel` so the receiver can build a [`rds_core::tree::DirTree`]
+//! without blocking on IO.
+//!
+//! The scanner does NOT own a `DirTree`. It sends `NodeDiscovered` events carrying
+//! `FileNode` values; the receiver (GUI or test harness) constructs the arena tree.
+//! Arena indices in `NodeDiscovered::parent_index` are receiver-side indices predicted
+//! by the scanner via sequential counter (DL-001).
+//!
+//! Module structure: `scanner.rs` owns all scan logic; this crate root re-exports
+//! `Scanner` as the public API surface (DL-003).

```


**CC-M-002-002** (crates/rds-scanner/src/scanner.rs) - implements CI-M-002-002

**Code:**

```diff
@@ new file - Scanner struct with scan() method, helpers extracted to stay under GOD_FUNCTION threshold
--- /dev/null
+++ b/crates/rds-scanner/src/scanner.rs
@@ -0,0 +1,208 @@
+use std::collections::HashMap;
+use std::path::PathBuf;
+use std::sync::atomic::{AtomicBool, Ordering};
+use std::sync::Arc;
+use std::thread::{self, JoinHandle};
+use std::time::{Instant, SystemTime, UNIX_EPOCH};
+
+use crossbeam_channel::Sender;
+use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
+use rds_core::tree::FileNode;
+use tracing::{debug, warn};
+use walkdir::WalkDir;
+
+pub struct Scanner;
+
+fn epoch_seconds(metadata: &std::fs::Metadata) -> Option<u64> {
+    metadata
+        .modified()
+        .ok()
+        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
+        .map(|d| d.as_secs())
+}
+
+fn send_root_node(
+    config: &ScanConfig,
+    tx: &Sender<ScanEvent>,
+    path_to_index: &mut HashMap<PathBuf, usize>,
+) -> Result<(), ()> {
+    let root_metadata = match std::fs::metadata(&config.root) {
+        Ok(m) => m,
+        Err(e) => {
+            let _ = tx.send(ScanEvent::ScanError {
+                path: config.root.clone(),
+                error: e.to_string(),
+            });
+            return Err(());
+        }
+    };
+
+    let root_modified = epoch_seconds(&root_metadata);
+    let root_name = config
+        .root
+        .file_name()
+        .map(|n| n.to_string_lossy().into_owned())
+        .unwrap_or_else(|| config.root.to_string_lossy().into_owned());
+
+    let root_node = FileNode {
+        name: root_name,
+        size: 0,
+        is_dir: true,
+        children: Vec::new(),
+        parent: None,
+        extension: None,
+        modified: root_modified,
+    };
+
+    if tx
+        .send(ScanEvent::NodeDiscovered {
+            node: root_node,
+            parent_index: None,
+        })
+        .is_err()
+    {
+        return Err(());
+    }
+
+    path_to_index.insert(config.root.clone(), 0);
+    Ok(())
+}
+
+fn entry_to_node(entry: &walkdir::DirEntry) -> FileNode {
+    let is_dir = entry.file_type().is_dir();
+    let (size, modified) = match entry.metadata() {
+        Ok(ref m) => {
+            let sz = if is_dir { 0 } else { m.len() };
+            (sz, epoch_seconds(m))
+        }
+        Err(_) => (0, None),
+    };
+
+    let ext = if is_dir {
+        None
+    } else {
+        entry
+            .path()
+            .extension()
+            .map(|e| e.to_string_lossy().to_lowercase())
+    };
+
+    let name = entry.file_name().to_string_lossy().into_owned();
+
+    FileNode {
+        name,
+        size,
+        is_dir,
+        children: Vec::new(),
+        parent: None,
+        extension: ext,
+        modified,
+    }
+}
+
+struct WalkAccum {
+    total_files: u64,
+    total_dirs: u64,
+    total_bytes: u64,
+    errors: u64,
+    node_count: usize,
+}
+
+impl Scanner {
+    pub fn scan(
+        config: ScanConfig,
+        tx: Sender<ScanEvent>,
+        cancel: Arc<AtomicBool>,
+    ) -> JoinHandle<()> {
+        thread::spawn(move || {
+            let start = Instant::now();
+            let mut path_to_index: HashMap<PathBuf, usize> = HashMap::new();
+
+            if send_root_node(&config, &tx, &mut path_to_index).is_err() {
+                let _ = tx.send(ScanEvent::ScanComplete {
+                    stats: ScanStats {
+                        total_files: 0,
+                        total_dirs: 0,
+                        total_bytes: 0,
+                        duration_ms: start.elapsed().as_millis() as u64,
+                        errors: 1,
+                    },
+                });
+                return;
+            }
+
+            let mut acc = WalkAccum {
+                total_files: 0,
+                total_dirs: 1,
+                total_bytes: 0,
+                errors: 0,
+                node_count: 1,
+            };
+
+            Self::walk_entries(&config, &tx, &cancel, &mut path_to_index, &mut acc);
+
+            debug!(
+                files = acc.total_files,
+                dirs = acc.total_dirs,
+                bytes = acc.total_bytes,
+                errors = acc.errors,
+                "scan complete"
+            );
+
+            let _ = tx.send(ScanEvent::ScanComplete {
+                stats: ScanStats {
+                    total_files: acc.total_files,
+                    total_dirs: acc.total_dirs,
+                    total_bytes: acc.total_bytes,
+                    duration_ms: start.elapsed().as_millis() as u64,
+                    errors: acc.errors,
+                },
+            });
+        })
+    }
+
+    fn walk_entries(
+        config: &ScanConfig,
+        tx: &Sender<ScanEvent>,
+        cancel: &Arc<AtomicBool>,
+        path_to_index: &mut HashMap<PathBuf, usize>,
+        acc: &mut WalkAccum,
+    ) {
+        let walker = WalkDir::new(&config.root).follow_links(config.follow_symlinks);
+
+        for entry_result in walker {
+            if cancel.load(Ordering::Relaxed) {
+                break;
+            }
+
+            if let Some(max) = config.max_nodes {
+                if acc.node_count >= max {
+                    let _ = tx.send(ScanEvent::ScanError {
+                        path: config.root.clone(),
+                        error: format!("max_nodes limit ({max}) reached, aborting scan"),
+                    });
+                    acc.errors += 1;
+                    break;
+                }
+            }
+
+            let entry = match entry_result {
+                Ok(e) => e,
+                Err(e) => {
+                    let err_path = e.path().map(|p| p.to_path_buf()).unwrap_or_default();
+                    warn!(path = %err_path.display(), error = %e, "walkdir error");
+                    let _ = tx.send(ScanEvent::ScanError {
+                        path: err_path,
+                        error: e.to_string(),
+                    });
+                    acc.errors += 1;
+                    continue;
+                }
+            };
+
+            let entry_path = entry.path().to_path_buf();
+            if entry_path == config.root {
+                continue;
+            }
+
+            let parent_path = match entry_path.parent() {
+                Some(p) => p.to_path_buf(),
+                None => continue,
+            };
+
+            let parent_idx = match path_to_index.get(&parent_path) {
+                Some(&idx) => idx,
+                None => {
+                    warn!(
+                        path = %entry_path.display(),
+                        parent = %parent_path.display(),
+                        "parent not in index map; parent was likely inaccessible"
+                    );
+                    let _ = tx.send(ScanEvent::ScanError {
+                        path: entry_path,
+                        error: "parent directory was inaccessible".to_string(),
+                    });
+                    acc.errors += 1;
+                    continue;
+                }
+            };
+
+            let node = entry_to_node(&entry);
+            let is_dir = node.is_dir;
+            let size = node.size;
+
+            if tx
+                .send(ScanEvent::NodeDiscovered {
+                    node,
+                    parent_index: Some(parent_idx),
+                })
+                .is_err()
+            {
+                return;
+            }
+
+            if is_dir {
+                acc.total_dirs += 1;
+            } else {
+                acc.total_files += 1;
+                acc.total_bytes += size;
+            }
+
+            path_to_index.insert(entry_path, acc.node_count);
+            acc.node_count += 1;
+
+            if acc.node_count % 100 == 0 {
+                let _ = tx.send(ScanEvent::Progress {
+                    files_scanned: (acc.total_files + acc.total_dirs) as u64,
+                    bytes_scanned: acc.total_bytes,
+                });
+            }
+        }
+    }
+}

```

**Documentation:**

```diff
--- /dev/null
+++ b/crates/rds-scanner/src/scanner.rs
@@ -0,0 +1,208 @@
+use std::collections::HashMap;
+use std::path::PathBuf;
+use std::sync::atomic::{AtomicBool, Ordering};
+use std::sync::Arc;
+use std::thread::{self, JoinHandle};
+use std::time::{Instant, SystemTime, UNIX_EPOCH};
+
+use crossbeam_channel::Sender;
+use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
+use rds_core::tree::FileNode;
+use tracing::{debug, warn};
+use walkdir::WalkDir;
+
+/// Single-threaded filesystem scanner. Spawn via [`Scanner::scan`].
+pub struct Scanner;
+
+/// Returns epoch seconds for a file's mtime, or `None` for pre-1970 timestamps
+/// or unreadable metadata. `Option<u64>` matches `FileNode::modified` (DL-015).
+fn epoch_seconds(metadata: &std::fs::Metadata) -> Option<u64> {
+    metadata
+        .modified()
+        .ok()
+        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
+        .map(|d| d.as_secs())
+}
+
+/// Sends the root `NodeDiscovered` event (parent_index=None) and inserts the
+/// root path at index 0 in `path_to_index`.
+///
+/// Returns `Err(())` if root metadata fails or the channel is disconnected.
+/// On error, sends `ScanError` before returning so the caller can emit
+/// `ScanComplete` and preserve the always-last invariant (DL-012).
+fn send_root_node(
+    config: &ScanConfig,
+    tx: &Sender<ScanEvent>,
+    path_to_index: &mut HashMap<PathBuf, usize>,
+) -> Result<(), ()> {
+    let root_metadata = match std::fs::metadata(&config.root) {
+        Ok(m) => m,
+        Err(e) => {
+            let _ = tx.send(ScanEvent::ScanError {
+                path: config.root.clone(),
+                error: e.to_string(),
+            });
+            return Err(());
+        }
+    };
+
+    let root_modified = epoch_seconds(&root_metadata);
+    let root_name = config
+        .root
+        .file_name()
+        .map(|n| n.to_string_lossy().into_owned())
+        .unwrap_or_else(|| config.root.to_string_lossy().into_owned());
+
+    let root_node = FileNode {
+        name: root_name,
+        size: 0,
+        is_dir: true,
+        children: Vec::new(),
+        parent: None,
+        extension: None,
+        modified: root_modified,
+    };
+
+    if tx
+        .send(ScanEvent::NodeDiscovered {
+            node: root_node,
+            parent_index: None,
+        })
+        .is_err()
+    {
+        return Err(());
+    }
+
+    path_to_index.insert(config.root.clone(), 0);
+    Ok(())
+}
+
+/// Converts a `walkdir::DirEntry` to a `FileNode`.
+///
+/// Directories always get size=0 to avoid double-counting in `DirTree::subtree_size`
+/// (DL-009). Extension is lowercased without leading dot per `FileNode` field contract.
+fn entry_to_node(entry: &walkdir::DirEntry) -> FileNode {
+    let is_dir = entry.file_type().is_dir();
+    let (size, modified) = match entry.metadata() {
+        Ok(ref m) => {
+            let sz = if is_dir { 0 } else { m.len() };
+            (sz, epoch_seconds(m))
+        }
+        Err(_) => (0, None),
+    };
+
+    let ext = if is_dir {
+        None
+    } else {
+        entry
+            .path()
+            .extension()
+            .map(|e| e.to_string_lossy().to_lowercase())
+    };
+
+    let name = entry.file_name().to_string_lossy().into_owned();
+
+    FileNode {
+        name,
+        size,
+        is_dir,
+        children: Vec::new(),
+        parent: None,
+        extension: ext,
+        modified,
+    }
+}
+
+/// Running totals accumulated during the walk.
+struct WalkAccum {
+    total_files: u64,
+    total_dirs: u64,
+    total_bytes: u64,
+    errors: u64,
+    /// Total nodes sent via `NodeDiscovered`; used to predict receiver-side arena
+    /// indices and to check `ScanConfig::max_nodes` (DL-001).
+    node_count: usize,
+}
+
+impl Scanner {
+    /// Spawns a background thread that walks `config.root` and sends `ScanEvent`
+    /// values over `tx`.
+    ///
+    /// Event ordering guarantee: `ScanComplete` is always the last event sent,
+    /// except when the receiver disconnects mid-scan (DL-018). The caller must
+    /// drain the channel until `ScanComplete` to get correct stats.
+    ///
+    /// `cancel` is polled with `Relaxed` ordering at the top of each iteration.
+    /// Cancel latency is bounded by one channel drain cycle (DL-010, DL-011).
+    ///
+    /// `exclude_patterns` from `ScanConfig` are not evaluated; pattern matching
+    /// is not evaluated (DL-008).
+    pub fn scan(
+        config: ScanConfig,
+        tx: Sender<ScanEvent>,
+        cancel: Arc<AtomicBool>,
+    ) -> JoinHandle<()> {
+        thread::spawn(move || {
+            let start = Instant::now();
+            let mut path_to_index: HashMap<PathBuf, usize> = HashMap::new();
+
+            if send_root_node(&config, &tx, &mut path_to_index).is_err() {
+                let _ = tx.send(ScanEvent::ScanComplete {
+                    stats: ScanStats {
+                        total_files: 0,
+                        total_dirs: 0,
+                        total_bytes: 0,
+                        duration_ms: start.elapsed().as_millis() as u64,
+                        errors: 1,
+                    },
+                });
+                return;
+            }
+
+            let mut acc = WalkAccum {
+                total_files: 0,
+                total_dirs: 1,  // root counted here; walkdir skips root in iteration
+                total_bytes: 0,
+                errors: 0,
+                node_count: 1,  // root is index 0; next entry will be index 1
+            };
+
+            Self::walk_entries(&config, &tx, &cancel, &mut path_to_index, &mut acc);
+
+            debug!(
+                files = acc.total_files,
+                dirs = acc.total_dirs,
+                bytes = acc.total_bytes,
+                errors = acc.errors,
+                "scan complete"
+            );
+
+            let _ = tx.send(ScanEvent::ScanComplete {
+                stats: ScanStats {
+                    total_files: acc.total_files,
+                    total_dirs: acc.total_dirs,
+                    total_bytes: acc.total_bytes,
+                    duration_ms: start.elapsed().as_millis() as u64,
+                    errors: acc.errors,
+                },
+            });
+        })
+    }
+
+    /// Core walk loop. Iterates `walkdir` entries, skipping the root (already sent),
+    /// building the `path_to_index` map, and sending `NodeDiscovered` or `ScanError`
+    /// events.
+    ///
+    /// Index prediction: `acc.node_count` starts at 1 (root is 0) and increments
+    /// after each successful `NodeDiscovered` send, matching the receiver's
+    /// append-only arena index (DL-001). Skipped error entries consume no index.
+    ///
+    /// Progress events fire every 100 nodes (combined files+dirs) for GUI
+    /// responsiveness without doubling channel traffic (DL-005).
+    fn walk_entries(
+        config: &ScanConfig,
+        tx: &Sender<ScanEvent>,
+        cancel: &Arc<AtomicBool>,
+        path_to_index: &mut HashMap<PathBuf, usize>,
+        acc: &mut WalkAccum,
+    ) {
+        let walker = WalkDir::new(&config.root).follow_links(config.follow_symlinks);
+
+        for entry_result in walker {
+            // Cancel checked per-iteration with Relaxed ordering (DL-010).
+            // Not checked inside tx.send() — channel rarely fills to 4096 (DL-011).
+            if cancel.load(Ordering::Relaxed) {
+                break;
+            }
+
+            if let Some(max) = config.max_nodes {
+                if acc.node_count >= max {
+                    let _ = tx.send(ScanEvent::ScanError {
+                        path: config.root.clone(),
+                        error: format!("max_nodes limit ({max}) reached, aborting scan"),
+                    });
+                    acc.errors += 1;
+                    break;
+                }
+            }
+
+            let entry = match entry_result {
+                Ok(e) => e,
+                Err(e) => {
+                    let err_path = e.path().map(|p| p.to_path_buf()).unwrap_or_default();
+                    warn!(path = %err_path.display(), error = %e, "walkdir error");
+                    let _ = tx.send(ScanEvent::ScanError {
+                        path: err_path,
+                        error: e.to_string(),
+                    });
+                    acc.errors += 1;
+                    continue;
+                }
+            };
+
+            let entry_path = entry.path().to_path_buf();
+            if entry_path == config.root {
+                continue;
+            }
+
+            let parent_path = match entry_path.parent() {
+                Some(p) => p.to_path_buf(),
+                None => continue,
+            };
+
+            let parent_idx = match path_to_index.get(&parent_path) {
+                Some(&idx) => idx,
+                None => {
+                    // Parent was not inserted — likely failed with permission error.
+                    // Send ScanError and skip; do not increment node_count (DL-013).
+                    warn!(
+                        path = %entry_path.display(),
+                        parent = %parent_path.display(),
+                        "parent not in index map; parent was likely inaccessible"
+                    );
+                    let _ = tx.send(ScanEvent::ScanError {
+                        path: entry_path,
+                        error: "parent directory was inaccessible".to_string(),
+                    });
+                    acc.errors += 1;
+                    continue;
+                }
+            };
+
+            let node = entry_to_node(&entry);
+            let is_dir = node.is_dir;
+            let size = node.size;
+
+            if tx
+                .send(ScanEvent::NodeDiscovered {
+                    node,
+                    parent_index: Some(parent_idx),
+                })
+                .is_err()
+            {
+                // Receiver disconnected; nobody to receive ScanComplete (DL-018).
+                return;
+            }
+
+            if is_dir {
+                acc.total_dirs += 1;
+            } else {
+                acc.total_files += 1;
+                acc.total_bytes += size;
+            }
+
+            path_to_index.insert(entry_path, acc.node_count);
+            acc.node_count += 1;
+
+            if acc.node_count % 100 == 0 {
+                let _ = tx.send(ScanEvent::Progress {
+                    files_scanned: (acc.total_files + acc.total_dirs) as u64,
+                    bytes_scanned: acc.total_bytes,
+                });
+            }
+        }
+    }
+}

```


**CC-M-002-003** (crates/rds-scanner/tests/scan_integration.rs) - implements CI-M-002-003

**Code:**

```diff
@@ new file - integration tests for Scanner
--- /dev/null
+++ b/crates/rds-scanner/tests/scan_integration.rs
@@ -0,0 +1,276 @@
+use std::fs;
+use std::sync::atomic::{AtomicBool, Ordering};
+use std::sync::Arc;
+
+use crossbeam_channel::bounded;
+use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
+use rds_core::tree::{DirTree, FileNode};
+use rds_scanner::Scanner;
+use tempfile::TempDir;
+
+fn build_tree_from_events(
+    rx: crossbeam_channel::Receiver<ScanEvent>,
+) -> (DirTree, ScanStats, Vec<(std::path::PathBuf, String)>) {
+    let mut tree: Option<DirTree> = None;
+    let mut stats: Option<ScanStats> = None;
+    let mut errors: Vec<(std::path::PathBuf, String)> = Vec::new();
+
+    for event in rx.iter() {
+        match event {
+            ScanEvent::NodeDiscovered { node, parent_index } => match parent_index {
+                None => {
+                    tree = Some(DirTree::new(&node.name));
+                }
+                Some(pidx) => {
+                    if let Some(ref mut t) = tree {
+                        t.insert(pidx, node);
+                    }
+                }
+            },
+            ScanEvent::ScanComplete { stats: s } => {
+                stats = Some(s);
+            }
+            ScanEvent::ScanError { path, error } => {
+                errors.push((path, error));
+            }
+            ScanEvent::Progress { .. } => {}
+            ScanEvent::DuplicateFound { .. } => {}
+        }
+    }
+
+    (
+        tree.expect("expected at least one NodeDiscovered event"),
+        stats.expect("expected ScanComplete event"),
+        errors,
+    )
+}
+
+fn make_config(root: std::path::PathBuf) -> ScanConfig {
+    ScanConfig {
+        root,
+        follow_symlinks: false,
+        exclude_patterns: Vec::new(),
+        hash_duplicates: false,
+        max_nodes: None,
+    }
+}
+
+#[test]
+fn scan_basic_tree() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    fs::write(root.join("a.txt"), "hello").unwrap();
+    fs::write(root.join("b.dat"), "0123456789").unwrap();
+    let sub = root.join("subdir");
+    fs::create_dir(&sub).unwrap();
+    fs::write(sub.join("c.rs"), "abc").unwrap();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let (tree, stats, errors) = build_tree_from_events(rx);
+
+    assert_eq!(tree.len(), 5, "root + subdir + 3 files = 5 nodes");
+    assert_eq!(tree.subtree_size(tree.root()), 18);
+    assert_eq!(stats.total_files, 3);
+    assert_eq!(stats.total_dirs, 2);
+    assert_eq!(stats.total_bytes, 18);
+    assert!(errors.is_empty(), "no errors expected: {errors:?}");
+}
+
+#[test]
+fn scan_empty_directory() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let (tree, stats, errors) = build_tree_from_events(rx);
+
+    assert_eq!(tree.len(), 1);
+    assert_eq!(tree.subtree_size(tree.root()), 0);
+    assert_eq!(stats.total_files, 0);
+    assert_eq!(stats.total_dirs, 1);
+    assert_eq!(stats.total_bytes, 0);
+    assert!(errors.is_empty());
+}
+
+#[test]
+fn scan_respects_cancellation() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    for i in 0..100 {
+        let dir = root.join(format!("dir_{i:03}"));
+        fs::create_dir(&dir).unwrap();
+        for j in 0..10 {
+            fs::write(dir.join(format!("file_{j}.txt")), "data").unwrap();
+        }
+    }
+
+    // bounded(10): scanner blocks after 10 buffered events, giving the
+    // receiver time to set the cancel flag before the walk completes
+    let (tx, rx) = bounded(10);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let cancel_clone = cancel.clone();
+    let handle = Scanner::scan(config, tx, cancel_clone);
+
+    let mut received = 0;
+    let mut got_complete = false;
+    for event in rx.iter() {
+        match event {
+            ScanEvent::NodeDiscovered { .. } => {
+                received += 1;
+                if received == 5 {
+                    cancel.store(true, Ordering::Relaxed);
+                }
+            }
+            ScanEvent::ScanComplete { .. } => {
+                got_complete = true;
+                break;
+            }
+            _ => {}
+        }
+    }
+
+    handle.join().unwrap();
+    assert!(got_complete, "ScanComplete must be sent even on cancellation");
+    assert!(
+        received < 1101,
+        "scan should have stopped early, got {received} nodes"
+    );
+}
+
+#[test]
+fn scan_max_nodes_abort() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    for i in 0..10 {
+        fs::write(root.join(format!("file_{i}.txt")), "x").unwrap();
+    }
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let mut config = make_config(root.to_path_buf());
+    config.max_nodes = Some(3);
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let (tree, _stats, errors) = build_tree_from_events(rx);
+
+    assert!(
+        tree.len() <= 3,
+        "tree should have at most 3 nodes, got {}",
+        tree.len()
+    );
+    assert!(
+        errors.iter().any(|(_, msg)| msg.contains("max_nodes")),
+        "expected max_nodes error message in errors: {errors:?}"
+    );
+}
+
+#[test]
+fn scan_file_extensions() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    fs::write(root.join("UPPER.TXT"), "a").unwrap();
+    fs::write(root.join("code.rs"), "b").unwrap();
+    fs::write(root.join("archive.tar.gz"), "c").unwrap();
+    fs::write(root.join("noext"), "d").unwrap();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let mut extensions: Vec<(String, Option<String>)> = Vec::new();
+
+    let (tree, _stats, _errors) = build_tree_from_events(rx);
+    for i in 0..tree.len() {
+        let node = tree.get(i).unwrap();
+        if !node.is_dir {
+            extensions.push((node.name.clone(), node.extension.clone()));
+        }
+    }
+
+    extensions.sort_by(|a, b| a.0.cmp(&b.0));
+
+    let find_ext = |name: &str| -> Option<String> {
+        extensions
+            .iter()
+            .find(|(n, _)| n == name)
+            .and_then(|(_, e)| e.clone())
+    };
+
+    assert_eq!(find_ext("UPPER.TXT"), Some("txt".to_string()));
+    assert_eq!(find_ext("code.rs"), Some("rs".to_string()));
+    assert_eq!(find_ext("archive.tar.gz"), Some("gz".to_string()));
+    assert_eq!(find_ext("noext"), None);
+}
+
+#[cfg(unix)]
+#[test]
+fn scan_reports_errors_and_continues() {
+    use std::os::unix::fs::PermissionsExt;
+
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    let denied = root.join("denied");
+    fs::create_dir(&denied).unwrap();
+    fs::write(denied.join("secret.txt"), "hidden").unwrap();
+
+    let accessible = root.join("accessible");
+    fs::create_dir(&accessible).unwrap();
+    fs::write(accessible.join("visible.txt"), "hello").unwrap();
+
+    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    fs::set_permissions(&denied, fs::Permissions::from_mode(0o755)).unwrap();
+
+    let (_tree, stats, errors) = build_tree_from_events(rx);
+
+    assert!(
+        stats.errors >= 1,
+        "expected at least 1 error, got {}",
+        stats.errors
+    );
+    assert!(
+        !errors.is_empty(),
+        "expected ScanError events for permission-denied directory"
+    );
+}

```

**Documentation:**

```diff
--- /dev/null
+++ b/crates/rds-scanner/tests/scan_integration.rs
@@ -0,0 +1,276 @@
+//! Integration tests for [`rds_scanner::Scanner`].
+//!
+//! All tests use real filesystem fixtures via `tempfile::TempDir` — no mocks
+//! or simulated functions (DL-006). Tests verify that the event stream produces
+//! a structurally correct `DirTree` with accurate file counts, sizes, and
+//! extension normalization.
+//!
+//! `build_tree_from_events` is the canonical receiver pattern: drain the channel
+//! until `ScanComplete`, use `DirTree::new` for the root event (parent_index=None),
+//! and `DirTree::insert` for all subsequent events (DL-017).
+use std::fs;
+use std::sync::atomic::{AtomicBool, Ordering};
+use std::sync::Arc;
+
+use crossbeam_channel::bounded;
+use rds_core::scan::{ScanConfig, ScanEvent, ScanStats};
+use rds_core::tree::{DirTree, FileNode};
+use rds_scanner::Scanner;
+use tempfile::TempDir;
+
+/// Drains the event channel and assembles a `DirTree`, final `ScanStats`, and
+/// error list. Panics if no root `NodeDiscovered` event or no `ScanComplete` is
+/// received — both indicate a scanner invariant violation.
+fn build_tree_from_events(
+    rx: crossbeam_channel::Receiver<ScanEvent>,
+) -> (DirTree, ScanStats, Vec<(std::path::PathBuf, String)>) {
+    let mut tree: Option<DirTree> = None;
+    let mut stats: Option<ScanStats> = None;
+    let mut errors: Vec<(std::path::PathBuf, String)> = Vec::new();
+
+    for event in rx.iter() {
+        match event {
+            ScanEvent::NodeDiscovered { node, parent_index } => match parent_index {
+                None => {
+                    tree = Some(DirTree::new(&node.name));
+                }
+                Some(pidx) => {
+                    if let Some(ref mut t) = tree {
+                        t.insert(pidx, node);
+                    }
+                }
+            },
+            ScanEvent::ScanComplete { stats: s } => {
+                stats = Some(s);
+            }
+            ScanEvent::ScanError { path, error } => {
+                errors.push((path, error));
+            }
+            ScanEvent::Progress { .. } => {}
+            ScanEvent::DuplicateFound { .. } => {}
+        }
+    }
+
+    (
+        tree.expect("expected at least one NodeDiscovered event"),
+        stats.expect("expected ScanComplete event"),
+        errors,
+    )
+}
+
+fn make_config(root: std::path::PathBuf) -> ScanConfig {
+    ScanConfig {
+        root,
+        follow_symlinks: false,
+        exclude_patterns: Vec::new(),
+        hash_duplicates: false,
+        max_nodes: None,
+    }
+}
+
+#[test]
+fn scan_basic_tree() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    fs::write(root.join("a.txt"), "hello").unwrap();
+    fs::write(root.join("b.dat"), "0123456789").unwrap();
+    let sub = root.join("subdir");
+    fs::create_dir(&sub).unwrap();
+    fs::write(sub.join("c.rs"), "abc").unwrap();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let (tree, stats, errors) = build_tree_from_events(rx);
+
+    assert_eq!(tree.len(), 5, "root + subdir + 3 files = 5 nodes");
+    assert_eq!(tree.subtree_size(tree.root()), 18);
+    assert_eq!(stats.total_files, 3);
+    assert_eq!(stats.total_dirs, 2);
+    assert_eq!(stats.total_bytes, 18);
+    assert!(errors.is_empty(), "no errors expected: {errors:?}");
+}
+
+#[test]
+fn scan_empty_directory() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let (tree, stats, errors) = build_tree_from_events(rx);
+
+    assert_eq!(tree.len(), 1);
+    assert_eq!(tree.subtree_size(tree.root()), 0);
+    assert_eq!(stats.total_files, 0);
+    assert_eq!(stats.total_dirs, 1);
+    assert_eq!(stats.total_bytes, 0);
+    assert!(errors.is_empty());
+}
+
+#[test]
+fn scan_respects_cancellation() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    for i in 0..100 {
+        let dir = root.join(format!("dir_{i:03}"));
+        fs::create_dir(&dir).unwrap();
+        for j in 0..10 {
+            fs::write(dir.join(format!("file_{j}.txt")), "data").unwrap();
+        }
+    }
+
+    // bounded(10): scanner blocks after 10 buffered events, giving the
+    // receiver time to set the cancel flag before the walk completes
+    let (tx, rx) = bounded(10);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let cancel_clone = cancel.clone();
+    let handle = Scanner::scan(config, tx, cancel_clone);
+
+    let mut received = 0;
+    let mut got_complete = false;
+    for event in rx.iter() {
+        match event {
+            ScanEvent::NodeDiscovered { .. } => {
+                received += 1;
+                if received == 5 {
+                    cancel.store(true, Ordering::Relaxed);
+                }
+            }
+            ScanEvent::ScanComplete { .. } => {
+                got_complete = true;
+                break;
+            }
+            _ => {}
+        }
+    }
+
+    handle.join().unwrap();
+    assert!(got_complete, "ScanComplete must be sent even on cancellation");
+    assert!(
+        received < 1101,
+        "scan should have stopped early, got {received} nodes"
+    );
+}
+
+#[test]
+fn scan_max_nodes_abort() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    for i in 0..10 {
+        fs::write(root.join(format!("file_{i}.txt")), "x").unwrap();
+    }
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let mut config = make_config(root.to_path_buf());
+    config.max_nodes = Some(3);
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let (tree, _stats, errors) = build_tree_from_events(rx);
+
+    assert!(
+        tree.len() <= 3,
+        "tree should have at most 3 nodes, got {}",
+        tree.len()
+    );
+    assert!(
+        errors.iter().any(|(_, msg)| msg.contains("max_nodes")),
+        "expected max_nodes error message in errors: {errors:?}"
+    );
+}
+
+#[test]
+fn scan_file_extensions() {
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    fs::write(root.join("UPPER.TXT"), "a").unwrap();
+    fs::write(root.join("code.rs"), "b").unwrap();
+    fs::write(root.join("archive.tar.gz"), "c").unwrap();
+    fs::write(root.join("noext"), "d").unwrap();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    let mut extensions: Vec<(String, Option<String>)> = Vec::new();
+
+    let (tree, _stats, _errors) = build_tree_from_events(rx);
+    for i in 0..tree.len() {
+        let node = tree.get(i).unwrap();
+        if !node.is_dir {
+            extensions.push((node.name.clone(), node.extension.clone()));
+        }
+    }
+
+    extensions.sort_by(|a, b| a.0.cmp(&b.0));
+
+    let find_ext = |name: &str| -> Option<String> {
+        extensions
+            .iter()
+            .find(|(n, _)| n == name)
+            .and_then(|(_, e)| e.clone())
+    };
+
+    assert_eq!(find_ext("UPPER.TXT"), Some("txt".to_string()));
+    assert_eq!(find_ext("code.rs"), Some("rs".to_string()));
+    assert_eq!(find_ext("archive.tar.gz"), Some("gz".to_string()));
+    assert_eq!(find_ext("noext"), None);
+}
+
+#[cfg(unix)]
+#[test]
+fn scan_reports_errors_and_continues() {
+    use std::os::unix::fs::PermissionsExt;
+
+    let tmp = TempDir::new().unwrap();
+    let root = tmp.path();
+
+    let denied = root.join("denied");
+    fs::create_dir(&denied).unwrap();
+    fs::write(denied.join("secret.txt"), "hidden").unwrap();
+
+    let accessible = root.join("accessible");
+    fs::create_dir(&accessible).unwrap();
+    fs::write(accessible.join("visible.txt"), "hello").unwrap();
+
+    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
+
+    let (tx, rx) = bounded(4096);
+    let cancel = Arc::new(AtomicBool::new(false));
+    let config = make_config(root.to_path_buf());
+
+    let handle = Scanner::scan(config, tx, cancel);
+    handle.join().unwrap();
+
+    fs::set_permissions(&denied, fs::Permissions::from_mode(0o755)).unwrap();
+
+    let (_tree, stats, errors) = build_tree_from_events(rx);
+
+    assert!(
+        stats.errors >= 1,
+        "expected at least 1 error, got {}",
+        stats.errors
+    );
+    assert!(
+        !errors.is_empty(),
+        "expected ScanError events for permission-denied directory"
+    );
+}

```


**CC-M-002-004** (crates/rds-scanner/CLAUDE.md)

**Documentation:**

```diff
--- a/crates/rds-scanner/CLAUDE.md
+++ b/crates/rds-scanner/CLAUDE.md
@@ -1,11 +1,13 @@
 # crates/rds-scanner/
 
-Parallel filesystem traversal and SHA-2 duplicate detection.
+Single-threaded filesystem traversal via `walkdir`; emits `ScanEvent` stream over bounded crossbeam-channel.
 
 ## Files
 
 | File | What | When to read |
 | ---- | ---- | ------------ |
-| `Cargo.toml` | Crate manifest; depends on `jwalk`, `rayon`, `sha2`, `crossbeam-channel`, `tracing`, `rds-core` | Modifying scanner dependencies |
-| `src/lib.rs` | Library root; bounded `crossbeam-channel` event streaming to GUI documented | Implementing scan logic, modifying scanner-GUI event protocol |
+| `Cargo.toml` | Crate manifest; depends on `walkdir`, `crossbeam-channel`, `tracing`, `rds-core` | Modifying scanner dependencies |
+| `src/lib.rs` | Library root; module declaration and public re-exports | Implementing scan logic, modifying public API |
+| `src/scanner.rs` | `Scanner` struct, `scan()` entry point, walk loop, helper functions | Implementing traversal, modifying event emission, debugging scan behaviour |
+| `tests/scan_integration.rs` | Integration tests; real filesystem fixtures via `tempfile` | Adding scan tests, verifying DirTree correctness, debugging event ordering |

```


**CC-M-002-005** (crates/rds-scanner/README.md)

**Documentation:**

```diff
--- /dev/null
+++ b/crates/rds-scanner/README.md
@@ -0,0 +1,86 @@
+# rds-scanner
+
+## Overview
+
+Walks a filesystem tree and emits `ScanEvent` values over a bounded
+`crossbeam-channel`. The receiver assembles a `DirTree` arena from the event
+stream. The scanner does not own a `DirTree`; ownership belongs to the receiver
+(GUI or test harness).
+
+## Architecture
+
+`Scanner::scan()` spawns a background thread. The caller holds the
+`JoinHandle` and the receive end of the channel. The channel bound is 4096
+events (~800 KB peak), balancing GUI drain rate (~60 fps) against worst-case
+memory use.
+
+Event ordering:
+
+1. `NodeDiscovered { parent_index: None }` — root node, always first
+2. `NodeDiscovered { parent_index: Some(i) }` — one per discovered entry,
+   depth-first, parent before child
+3. `Progress { .. }` — every 100 nodes (files and directories combined);
+   `files_scanned` field counts total nodes discovered, not files only (DL-016)
+4. `ScanError { .. }` — on any IO error; scan continues after each error
+5. `ScanComplete { .. }` — always last, except when the receiver disconnects
+
+### Index prediction
+
+`NodeDiscovered::parent_index` carries a receiver-side arena index. The scanner
+predicts it via a sequential counter starting at 0 (root). This works because:
+
+- `walkdir` documents depth-first, parent-before-child ordering
+- events travel through a FIFO channel
+- the receiver inserts nodes in receive order into an append-only arena
+
+Skipped entries (permission errors, parent-lookup failures) consume no index.
+The counter only increments on successful `NodeDiscovered` sends.
+
+## Design Decisions
+
+**walkdir over jwalk (DL-002):** Uses `walkdir` for single-threaded traversal
+to validate correctness on a single thread. Parallel traversal is handled by `jwalk` (MS4).
+
+**Scanner does not build DirTree (RA-002):** The design spec assigns DirTree
+ownership to the receiver. The scanner produces a stream; the receiver interprets
+it. This allows different receivers (GUI, tests, future exporters) to build their
+own representations.
+
+**Channel bound 4096 (DL-004):** Too small causes frequent scanner blocking;
+too large wastes memory when the GUI is slow. 4096 events x ~200 bytes/FileNode
+= ~800 KB ceiling.
+
+**Progress every 100 nodes (DL-005, DL-016):** Per-node progress doubles channel
+traffic with no perceptible user benefit. 100-node granularity balances
+responsiveness on small scans against channel efficiency. The `files_scanned`
+field in `Progress` counts all discovered nodes (files + directories combined),
+not files only — named for API compatibility with `ScanStats::total_files`.
+
+**Directory size = 0 (DL-009):** `DirTree::subtree_size` sums recursively. If
+directory nodes carried size, intermediate directories would double-count their
+subtrees. Only file nodes carry `metadata.len()`.
+
+**exclude_patterns ignored (DL-008):** Pattern matching against `OsStr` paths
+adds glob-matching complexity not addressed in this crate. The field
+exists in `ScanConfig` but is not evaluated.
+
+## Invariants
+
+- `ScanComplete` is always the last event, unless the receiver disconnects
+  mid-scan (DL-018). Callers must drain until `ScanComplete` for correct stats.
+- Arena indices in `NodeDiscovered::parent_index` are sequential and
+  append-only. Index 0 is always the root. Skipped entries do not consume an
+  index.
+- `walkdir` parent-before-child ordering is relied upon for correct parent
+  HashMap lookups. This is documented API behaviour of `walkdir 2.x` (DL-014).
+- `cancel` flag is polled with `Relaxed` ordering at loop top, not inside
+  `tx.send()`. Cancel latency is bounded by one channel drain cycle (DL-010,
+  DL-011).
+- `FileNode.size` for directories is always 0 (DL-009).
+- `FileNode.extension` is lowercased without a leading dot, or `None` for
+  directories and extensionless files.
+- `FileNode.modified` is `None` for pre-1970 timestamps and unreadable metadata
+  (DL-015).
+- Root metadata failure sends `ScanError` then `ScanComplete` with `errors=1`
+  and zero counts (DL-012).
+- Parent HashMap lookup failure sends `ScanError` for the child path and skips
+  the entry without incrementing the index counter (DL-013).

```


**CC-M-002-006** (CLAUDE.md)

**Documentation:**

```diff
--- a/CLAUDE.md
+++ b/CLAUDE.md
@@ -20,7 +20,7 @@
 | Directory | What | When to read |
 | --------- | ---- | ------------ |
 | `crates/rds-core/` | Shared data types; zero deps beyond `serde` | Modifying core types, understanding arena tree layout |
-| `crates/rds-scanner/` | Parallel filesystem traversal and SHA-2 hashing | Implementing scan logic, modifying scanner-GUI communication |
+| `crates/rds-scanner/` | Filesystem traversal via `walkdir`; streams `ScanEvent` over bounded channel to receiver | Implementing scan logic, modifying scanner-GUI communication |
 | `crates/rds-gui/` | egui/eframe immediate-mode GUI and treemap shell | Implementing UI panels, modifying the eframe app struct |

```


**CC-M-002-007** (CLAUDE.md)

**Documentation:**

```diff
--- a/CLAUDE.md
+++ b/CLAUDE.md
@@ -20,7 +20,7 @@
 | Directory | What | When to read |
 | --------- | ---- | ------------ |
 | `crates/rds-core/` | Shared data types; zero deps beyond `serde` | Modifying core types, understanding arena tree layout |
-| `crates/rds-scanner/` | Parallel filesystem traversal and SHA-2 hashing | Implementing scan logic, modifying scanner-GUI communication |
+| `crates/rds-scanner/` | Filesystem traversal via `walkdir`; streams `ScanEvent` over bounded channel to receiver | Implementing scan logic, modifying scanner-GUI communication |
 | `crates/rds-gui/` | egui/eframe immediate-mode GUI and treemap shell | Implementing UI panels, modifying the eframe app struct |

```

