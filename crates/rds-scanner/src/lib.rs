//! Parallel filesystem scanner.
//!
//! Walks a directory tree using `jwalk` (parallel) and `rayon`, building a
//! `Vec<FileNode>` arena in `rds-core`. Scan events are pushed over a bounded
//! `crossbeam-channel` so the GUI thread can drain them without stalling on IO.
//!
//! SHA-2 hashing for duplicate detection is provided by `sha2`.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
