//! Parallel filesystem scanner.
//!
//! Walks a directory tree using `jwalk` and sends [`ScanEvent`] values through
//! a bounded `crossbeam-channel` so the receiver can build a [`rds_core::tree::DirTree`]
//! without blocking on IO.
//!
//! The scanner does NOT own a `DirTree`. It sends `NodeDiscovered` events carrying
//! `FileNode` values; the receiver (GUI or test harness) constructs the arena tree.
//! Arena indices in `NodeDiscovered::parent_index` are receiver-side indices predicted
//! by the scanner via sequential counter. jwalk yields entries in strict depth-first
//! parent-before-child order, preserving the sequential-counter index prediction
//! scheme (ref: DL-001).
//!
//! **Ordering invariant**: sequential index prediction couples scanner and receiver
//! ordering. The bounded `crossbeam-channel` preserves event order; the receiver
//! MUST process `NodeDiscovered` events in arrival order without reordering or
//! dropping events, or arena indices will be wrong.
//!
//! jwalk parallelizes `readdir` syscalls across rayon worker threads while delivering
//! results to the caller in depth-first order. The `process_read_dir` callback runs
//! on worker threads and is used to propagate cancel and max_nodes signals at the
//! earliest possible point (ref: DL-002, DL-003).
//!
//! Module structure: `scanner.rs` owns all scan logic; this crate root re-exports
//! `Scanner` as the public API surface.

mod scanner;

pub use scanner::Scanner;
