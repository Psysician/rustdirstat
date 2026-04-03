//! Core data types shared across all rds crates.
//!
//! Each concern lives in its own submodule rather than a single monolithic
//! file; this keeps each module navigable as the type count grows. (DL-001)
//!
//! Four submodules map to distinct type groups: `tree` (arena + node),
//! `scan` (events, config, stats), `config` (app settings), `stats`
//! (per-extension aggregation). (DL-006)
//!
//! Depends only on `serde` beyond `std` so it compiles fast and tests run
//! without pulling in IO or GUI dependencies.
//!
//! The file tree is represented as an arena-allocated `Vec<FileNode>` with
//! `usize` index references rather than `Rc`/`Box` pointers, giving cache-local
//! traversal and zero reference-counting overhead. (DL-002)

pub mod config;
pub mod scan;
pub mod stats;
pub mod tree;

pub use config::{AppConfig, ColorScheme, CustomCommand, SortOrder};
pub use scan::{ScanConfig, ScanEvent, ScanStats};
pub use stats::{ExtensionStats, HslColor, color_for_extension, compute_extension_stats};
pub use tree::{ChildIter, DirTree, FileNode, NO_PARENT};
