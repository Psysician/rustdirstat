use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::Sender;
use rayon::prelude::*;
use rds_core::scan::ScanEvent;
use sha2::{Digest, Sha256};
use tracing::{info_span, warn};

use crate::scanner::FileEntry;

const PARTIAL_HASH_BYTES: usize = 4096;

pub struct DuplicateDetector;

impl DuplicateDetector {
    pub fn find_duplicates(files: &[FileEntry], tx: &Sender<ScanEvent>, cancel: &Arc<AtomicBool>) {
        let pipeline_span = info_span!("duplicate_pipeline");
        let _pipeline_guard = pipeline_span.enter();

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        // Phase 1: Group by size (unchanged)
        let size_groups = Self::phase1_group_by_size(files);

        // Process each size group through phases 2-3 independently.
        // Each group's intermediates are dropped before the next group starts,
        // reducing peak memory from O(all candidates) to O(largest single size group).
        for (_size, group) in size_groups {
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            // Phase 2: Partial hash within this group
            let partial_groups = Self::phase2_partial_hash_group(&group, cancel);

            // Phase 3: Full hash and emit
            for hash_group in partial_groups {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                Self::phase3_full_hash_group(&hash_group, tx, cancel);
            }
            // group and partial_groups are dropped here, freeing memory
        }
    }

    fn phase1_group_by_size(files: &[FileEntry]) -> Vec<(u64, Vec<&FileEntry>)> {
        let mut by_size: HashMap<u64, Vec<&FileEntry>> = HashMap::new();
        for entry in files {
            if entry.size == 0 {
                continue;
            }
            by_size.entry(entry.size).or_default().push(entry);
        }
        by_size
            .into_iter()
            .filter(|(_size, group)| group.len() >= 2)
            .collect()
    }

    fn phase2_partial_hash_group<'a>(
        group: &[&'a FileEntry],
        cancel: &Arc<AtomicBool>,
    ) -> Vec<Vec<HashedEntry<'a>>> {
        let hashed: Vec<Option<HashedEntry>> = group
            .par_iter()
            .map(|entry| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                match Self::hash_partial(&entry.path) {
                    Ok((hash, is_full)) => Some(HashedEntry {
                        entry,
                        partial_hash: hash,
                        is_full_hash: is_full,
                    }),
                    Err(e) => {
                        warn!(path = %entry.path.display(), error = %e, "I/O error during partial hashing, skipping file");
                        None
                    }
                }
            })
            .collect();

        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }

        let mut by_hash: HashMap<[u8; 32], Vec<HashedEntry>> = HashMap::new();
        for he in hashed.into_iter().flatten() {
            by_hash.entry(he.partial_hash).or_default().push(he);
        }

        by_hash.into_values().filter(|g| g.len() >= 2).collect()
    }

    fn phase3_full_hash_group(
        group: &[HashedEntry<'_>],
        tx: &Sender<ScanEvent>,
        cancel: &Arc<AtomicBool>,
    ) {
        let hashed: Vec<Option<(usize, [u8; 32])>> = group
            .par_iter()
            .map(|he| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                if he.is_full_hash {
                    return Some((he.entry.arena_index, he.partial_hash));
                }
                match Self::hash_full(&he.entry.path) {
                    Ok(hash) => Some((he.entry.arena_index, hash)),
                    Err(e) => {
                        warn!(path = %he.entry.path.display(), error = %e, "I/O error during full hashing, skipping file");
                        None
                    }
                }
            })
            .collect();

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        let mut by_hash: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
        for item in hashed.into_iter().flatten() {
            let (arena_index, hash) = item;
            by_hash.entry(hash).or_default().push(arena_index);
        }

        for (hash, node_indices) in by_hash {
            if node_indices.len() >= 2 {
                let _ = tx.send(ScanEvent::DuplicateFound { hash, node_indices });
            }
        }
    }

    fn hash_partial(path: &std::path::Path) -> std::io::Result<([u8; 32], bool)> {
        let mut file = File::open(path)?;
        let mut buf = [0u8; PARTIAL_HASH_BYTES];
        let mut total_read = 0;
        loop {
            let n = file.read(&mut buf[total_read..])?;
            if n == 0 {
                break;
            }
            total_read += n;
            if total_read == PARTIAL_HASH_BYTES {
                break;
            }
        }
        let mut hasher = Sha256::new();
        hasher.update(&buf[..total_read]);
        let hash: [u8; 32] = hasher.finalize().into();
        let is_full = total_read < PARTIAL_HASH_BYTES;
        Ok((hash, is_full))
    }

    fn hash_full(path: &std::path::Path) -> std::io::Result<[u8; 32]> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().into())
    }
}

struct HashedEntry<'a> {
    entry: &'a FileEntry,
    partial_hash: [u8; 32],
    is_full_hash: bool,
}
