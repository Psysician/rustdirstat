use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rds_core::{DirTree, FileNode, compute_extension_stats};

fn make_file_node(name: &str, size: u64, ext: Option<&str>) -> FileNode {
    FileNode {
        name: name.to_string(),
        size,
        is_dir: false,
        children: Vec::new(),
        parent: None,
        extension: ext.map(|s| s.to_string()),
        modified: None,
        deleted: false,
    }
}

fn make_dir_node(name: &str) -> FileNode {
    FileNode {
        name: name.to_string(),
        size: 0,
        is_dir: true,
        children: Vec::new(),
        parent: None,
        extension: None,
        modified: None,
        deleted: false,
    }
}

/// Benchmark: insert N nodes into a flat tree (all children of root).
fn bench_insert_nodes(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_operations");

    for &n in &[1_000usize, 10_000, 100_000, 1_000_000] {
        if n >= 1_000_000 {
            group.sample_size(10);
        }
        group.bench_with_input(BenchmarkId::new("insert_nodes", n), &n, |b, &n| {
            b.iter(|| {
                let mut tree = DirTree::new("/root");
                for i in 0..n {
                    let node = make_file_node(&format!("file_{i}.txt"), 1024, Some("txt"));
                    tree.insert(0, node);
                }
                tree
            });
        });
    }

    group.finish();
}

/// Build a nested tree: 100 directories with `n / 100` files each.
fn build_nested_tree(n: usize) -> DirTree {
    let mut tree = DirTree::new("/root");
    let dirs_count = 100;
    let files_per_dir = n / dirs_count;

    for d in 0..dirs_count {
        let dir_idx = tree.insert(0, make_dir_node(&format!("dir_{d}")));
        for f in 0..files_per_dir {
            let ext = match f % 5 {
                0 => Some("rs"),
                1 => Some("txt"),
                2 => Some("json"),
                3 => Some("toml"),
                _ => Some("md"),
            };
            let node = make_file_node(
                &format!("file_{f}.{}", ext.unwrap_or("bin")),
                (f as u64 + 1) * 100,
                ext,
            );
            tree.insert(dir_idx, node);
        }
    }

    tree
}

/// Benchmark: compute subtree_size(0) on a nested tree with N nodes.
fn bench_subtree_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_operations");

    for &n in &[10_000usize, 100_000, 1_000_000] {
        if n >= 1_000_000 {
            group.sample_size(10);
        }

        let tree = build_nested_tree(n);

        group.bench_with_input(BenchmarkId::new("subtree_size", n), &n, |b, &_n| {
            b.iter(|| tree.subtree_size(0));
        });
    }

    group.finish();
}

/// Benchmark: compute_extension_stats on a nested tree with N nodes.
fn bench_compute_extension_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_operations");

    for &n in &[10_000usize, 100_000, 1_000_000] {
        if n >= 1_000_000 {
            group.sample_size(10);
        }

        let tree = build_nested_tree(n);

        group.bench_with_input(
            BenchmarkId::new("compute_extension_stats", n),
            &n,
            |b, &_n| {
                b.iter(|| compute_extension_stats(&tree));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_insert_nodes,
    bench_subtree_size,
    bench_compute_extension_stats,
);
criterion_main!(benches);
