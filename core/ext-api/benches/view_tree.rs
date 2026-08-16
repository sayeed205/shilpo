use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use shilpo_ext_api::{ContainerDirection, ContainerNode, ViewLimits, ViewNode, ViewTree};

fn build_divider_tree(total_nodes: usize) -> ViewTree {
    assert!(total_nodes >= 1);
    if total_nodes == 1 {
        ViewTree::new(ViewNode::Divider)
    } else {
        let child_count = total_nodes - 1;
        let children = (0..child_count).map(|_| ViewNode::Divider).collect();
        ViewTree::new(ViewNode::Container(ContainerNode {
            direction: ContainerDirection::Column,
            children,
            style: None,
            gap: None,
            align_items: None,
            justify_content: None,
            wrap: false,
            event_id: None,
        }))
    }
}

fn bench_view_tree_validation(c: &mut Criterion) {
    let limits = ViewLimits::default();

    // 1. Valid trees of 1, 64, 256, 1024 nodes
    let sizes = [1, 64, 256, 1_024];
    let trees: Vec<(usize, ViewTree)> = sizes
        .iter()
        .map(|&size| {
            let tree = build_divider_tree(size);
            assert!(
                tree.validate(limits).is_ok(),
                "tree of size {size} must pass validation"
            );
            (size, tree)
        })
        .collect();

    let mut valid_group = c.benchmark_group("view_tree/validate_valid");
    for (size, tree) in &trees {
        valid_group.throughput(Throughput::Elements(*size as u64));
        valid_group.bench_with_input(BenchmarkId::new("nodes", size), tree, |b, t| {
            b.iter(|| black_box(t).validate(black_box(limits)));
        });
    }
    valid_group.finish();

    // 2. Exact 1,025-node rejection tree
    let rejection_size = 1_025;
    let rejection_tree = build_divider_tree(rejection_size);
    assert!(
        rejection_tree.validate(limits).is_err(),
        "tree of size 1025 must fail validation with default limits"
    );

    let mut rejection_group = c.benchmark_group("view_tree/validate_rejection");
    rejection_group.throughput(Throughput::Elements(rejection_size as u64));
    rejection_group.bench_with_input(
        BenchmarkId::new("nodes", rejection_size),
        &rejection_tree,
        |b, t| {
            b.iter(|| black_box(t).validate(black_box(limits)));
        },
    );
    rejection_group.finish();
}

criterion_group!(benches, bench_view_tree_validation);
criterion_main!(benches);
