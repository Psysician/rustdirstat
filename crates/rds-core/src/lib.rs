//! Core data types shared across all crates.
//!
//! Depends only on `serde` beyond `std` so it compiles fast and tests run
//! without pulling in IO or GUI dependencies.
//!
//! The file tree is represented as an arena-allocated `Vec<FileNode>` with
//! `usize` index references rather than `Rc`/`Box` pointers, giving cache-local
//! traversal and zero reference-counting overhead.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
