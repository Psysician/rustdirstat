use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rds_core::tree::{DirTree, FileNode, NO_PARENT};
use rds_gui::{SubtreeStats, TreemapLayout};

fn make_file(name: &str, size: u64, _ext: Option<&str>) -> FileNode {
    FileNode {
        name: name.into(),
        size,
        children: Vec::new(),
        modified: 0,
        parent: NO_PARENT,
        extension: 0,
        flags: 0,
    }
}

fn make_dir(name: &str) -> FileNode {
    FileNode {
        name: name.into(),
        size: 0,
        children: Vec::new(),
        modified: 0,
        parent: NO_PARENT,
        extension: 0,
        flags: 1,
    }
}

/// Builds a DirTree with approximately `node_count` nodes.
/// ~10% directories, rest files distributed among them.
fn build_tree(node_count: usize) -> DirTree {
    let dir_count = (node_count / 10).max(1);
    let file_count = node_count.saturating_sub(dir_count + 1); // -1 for root

    let mut tree = DirTree::new_with_capacity("/bench_root", node_count);

    let extensions = ["rs", "txt", "json", "toml", "md", "png", "jpg", "csv"];

    // Create directories as children of root.
    let mut dir_indices = Vec::with_capacity(dir_count);
    for i in 0..dir_count {
        let idx = tree.insert(0, make_dir(&format!("dir_{i}")));
        dir_indices.push(idx);
    }

    // Distribute files among directories round-robin.
    for i in 0..file_count {
        let parent = dir_indices[i % dir_count];
        let ext = extensions[i % extensions.len()];
        let size = ((i as u64 % 10_000) + 1) * 1024;
        let ext_idx = tree.intern_extension(Some(ext));
        let mut node = make_file(&format!("file_{i}.{ext}"), size, Some(ext));
        node.extension = ext_idx;
        tree.insert(parent, node);
    }

    tree
}

fn treemap_layout_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("treemap_layout_compute");

    let scales: &[usize] = &[1_000, 10_000, 50_000, 100_000, 500_000];

    for &n in scales {
        let tree = build_tree(n);
        let stats = SubtreeStats::compute(&tree);
        let size = egui::vec2(1920.0, 1080.0);

        if n >= 100_000 {
            group.sample_size(10);
        }

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| TreemapLayout::compute(&tree, &stats, size, 0));
        });
    }

    group.finish();
}

fn subtree_stats_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("subtree_stats_compute");

    let scales: &[usize] = &[1_000, 10_000, 50_000, 100_000, 500_000];

    for &n in scales {
        let tree = build_tree(n);

        if n >= 100_000 {
            group.sample_size(10);
        }

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| SubtreeStats::compute(&tree));
        });
    }

    group.finish();
}

fn treemap_with_aggregation(c: &mut Criterion) {
    let mut group = c.benchmark_group("treemap_with_aggregation");
    group.sample_size(10);

    let scales: &[usize] = &[100_000, 500_000];

    for &n in scales {
        let tree = build_tree(n);
        let stats = SubtreeStats::compute(&tree);
        let size = egui::vec2(1920.0, 1080.0);

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let layout = TreemapLayout::compute(&tree, &stats, size, 0);
                // Verify aggregation is active for large trees
                assert!(layout.rects.len() <= rds_gui::MAX_DISPLAY_RECTS + 1000);
                layout
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    treemap_layout_compute,
    subtree_stats_compute,
    treemap_with_aggregation
);
criterion_main!(benches);
