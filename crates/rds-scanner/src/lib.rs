//! Single-threaded filesystem scanner.
//!
//! Walks a directory tree using `walkdir` and sends [`ScanEvent`] values through
//! a bounded `crossbeam-channel` so the receiver can build a [`rds_core::tree::DirTree`]
//! without blocking on IO.
//!
//! The scanner does NOT own a `DirTree`. It sends `NodeDiscovered` events carrying
//! `FileNode` values; the receiver (GUI or test harness) constructs the arena tree.
//! Arena indices in `NodeDiscovered::parent_index` are receiver-side indices predicted
//! by the scanner via sequential counter (DL-001).
//!
//! Module structure: `scanner.rs` owns all scan logic; this crate root re-exports
//! `Scanner` as the public API surface (DL-003).

mod scanner;

pub use scanner::Scanner;
