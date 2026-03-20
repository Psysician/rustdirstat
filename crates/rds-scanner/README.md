# rds-scanner

Parallel filesystem traversal via `jwalk`; emits `ScanEvent` stream over bounded `crossbeam-channel`.

## Ordering invariant

`jwalk` parallelizes `readdir` syscalls across rayon worker threads but yields results through a single `DirEntryIter` on the calling thread in strict depth-first parent-before-child order. This ordering guarantee is what makes sequential index prediction possible: the scanner starts at 0 for the root and increments a counter per `NodeDiscovered` sent; the receiver inserts in arrival order so its arena indices match the scanner's predictions. If the receiver reorders or drops events, indices break. The bounded `crossbeam-channel` preserves event order; the receiver must not reorder.

`path_to_index` (the `HashMap<PathBuf, usize>`) is only ever accessed from the scanner thread iterating jwalk results. The `process_read_dir` callbacks run on rayon worker threads and never touch it, so no synchronization is needed for that map.

## Two-level abort

Cancel and max_nodes abort operate at two levels:

1. `process_read_dir` callback (rayon worker threads): when `cancel` or `max_nodes_reached` is true, sets `read_children_path = None` on each directory entry to prevent subdirectory scheduling, then clears the children vec to prevent those entries from being yielded. This stops new readdir work at the earliest point.
2. Iteration loop top (scanner thread): checks `cancel` flag and breaks. Catches already-queued entries that were in flight when the callback ran.

Worst-case extra entries after abort is bounded by rayon thread count times directory size.

## max_nodes design

`max_nodes_reached` is an `AtomicBool` set by the main scanner thread when the node count ceiling is hit. The `process_read_dir` callback reads this flag to stop spawning new readdir work. An `AtomicBool` set by the main thread is simpler than an `AtomicUsize` shared counter: the callback sees directory children before they are yielded so a shared counter would be inaccurate anyway. The main thread still enforces the exact limit; the callback only needs to stop spawning, not count precisely.

## skip_hidden parity

`jwalk` defaults to `skip_hidden(true)`, silently skipping dotfiles. `walkdir` does not skip hidden files. `.skip_hidden(false)` is required on the `WalkDir` builder to include dotfiles in scans, matching the expected behavior.

## follow_links behavioral difference

When `follow_symlinks = true`, `jwalk` detects symlink cycles via path-string comparison while `walkdir` uses inode comparison. String comparison is more conservative (may report false positives on hardlinked directories) but never misses real loops. The default is `follow_symlinks = false` so this code path is not exercised in normal operation. No test covers `follow_links = true` because the difference only matters with hardlinked directories, which is rare, and symlink fixtures are platform-specific.

## Error sources

Two error sources for jwalk entries:

- Iterator `Err` values: `jwalk::Error` variants `Io`, `Loop`, `ThreadpoolBusy`. Path extracted via `Error::path()`, falls back to empty `PathBuf` for `ThreadpoolBusy` which carries no path.
- `entry.read_children_error`: directory readdir failure surfaced on an `Ok` entry. Emitted as `ScanError` without consuming an arena index; the entry itself is still processed.
