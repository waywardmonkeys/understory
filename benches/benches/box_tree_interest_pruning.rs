// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use kurbo::{Point, Rect};
use understory_box_tree::{InterestMask, LocalNode, NodeFlags, Tree};

fn build_scene(num_cold_subtrees: usize, num_hot_subtrees: usize, depth: usize) -> Tree {
    let mut tree = Tree::new();
    let root = tree.insert(
        None,
        LocalNode {
            local_bounds: Rect::new(0.0, 0.0, 10_000.0, 10_000.0),
            ..Default::default()
        },
    );

    fn build_chain(tree: &mut Tree, parent: understory_box_tree::NodeId, depth: usize) -> understory_box_tree::NodeId {
        let mut current = parent;
        for i in 0..depth {
            let node = tree.insert(
                Some(current),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 10_000.0 - i as f64, 10_000.0 - i as f64),
                    z_index: i as i32,
                    flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                    ..Default::default()
                },
            );
            current = node;
        }
        current
    }

    // Cold subtrees: no POINTER_MOVE interest.
    for _ in 0..num_cold_subtrees {
        let leaf = build_chain(&mut tree, root, depth);
        tree.set_interest(leaf, InterestMask::WHEEL);
    }

    // Hot subtrees: POINTER_MOVE interest at the leaf.
    for _ in 0..num_hot_subtrees {
        let leaf = build_chain(&mut tree, root, depth);
        tree.set_interest(leaf, InterestMask::POINTER_MOVE);
    }

    let _ = tree.commit();
    tree
}

fn bench_walk_interest_pruning(c: &mut Criterion) {
    let mut group = c.benchmark_group("box_tree_interest_walk");
    // Larger scene: many cold subtrees, fewer hot ones, with some depth.
    let num_cold = 512usize;
    let num_hot = 32usize;
    let depth = 12usize;
    let tree = build_scene(num_cold, num_hot, depth);
    let pt = Point::new(5_000.0, 5_000.0);

    group.throughput(Throughput::Elements((num_cold + num_hot) as u64));

    group.bench_function("walk_point_topdown_no_interest", |b| {
        b.iter(|| {
            let filter = understory_box_tree::QueryFilter {
                visible_only: true,
                pickable_only: true,
                ..understory_box_tree::QueryFilter::default()
            };
            for id in tree.walk_point_topdown(pt, filter) {
                black_box(id);
            }
        });
    });

    group.bench_function("walk_point_topdown_pointer_move_interest", |b| {
        b.iter(|| {
            let filter = understory_box_tree::QueryFilter {
                visible_only: true,
                pickable_only: true,
                interest_required: InterestMask::POINTER_MOVE,
            };
            for id in tree.walk_point_topdown(pt, filter) {
                black_box(id);
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_walk_interest_pruning);
criterion_main!(benches);
