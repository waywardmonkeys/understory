// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Binary bounding hierarchy backend generic over scalar `T: Scalar`.
//!
//! This is a simple, allocation-friendly BVH intended for dynamic workloads.
//!
//! - Inserts use a greedy descent and occasionally split leaves using an SAH-like heuristic.
//! - Updates are handled by *refitting* ancestor bounds instead of remove+reinsert.
//!   This keeps small per-frame changes cheap, but can make the tree "looser" over time.
//! - Queries use an explicit stack (with a small inline fast-path) and aggressively
//!   early-exit/prune to reduce overhead.

use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::backend::Backend;
use crate::types::{Aabb2D, Scalar};

/// A simple BVH backend using SAH-like splits.
pub struct Bvh<T: Scalar> {
    /// Maximum number of items stored in a leaf before we split it.
    max_leaf: usize,
    root: Option<NodeIdx>,
    /// Node storage. Nodes are never removed; empty subtrees are represented via `Node::count == 0`.
    arena: Vec<Node<T>>,
    slots: Vec<Option<Aabb2D<T>>>,
    /// Maps each slot to the leaf node currently holding it, enabling O(1) update/remove.
    ///
    /// This is kept in sync on insert (including leaf splits).
    slot_leaf: Vec<Option<NodeIdx>>,
}

enum Kind<T: Scalar> {
    Leaf(Vec<(usize, Aabb2D<T>)>),
    Internal { left: NodeIdx, right: NodeIdx },
}

struct Node<T: Scalar> {
    /// Parent pointer for refitting after updates/removals.
    parent: Option<NodeIdx>,
    /// Bounding box for the subtree. Meaningful only when `count > 0`.
    bbox: Aabb2D<T>,
    /// Number of live items in this subtree.
    ///
    /// We keep this so empty subtrees can be pruned quickly (and so we can compute parent
    /// bounds without scanning children).
    count: usize,
    kind: Kind<T>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct NodeIdx(usize);

impl NodeIdx {
    const fn new(i: usize) -> Self {
        Self(i)
    }

    const fn get(self) -> usize {
        self.0
    }
}

impl<T: Scalar> Default for Bvh<T> {
    fn default() -> Self {
        Self {
            max_leaf: 8,
            root: None,
            arena: Vec::new(),
            slots: Vec::new(),
            slot_leaf: Vec::new(),
        }
    }
}

// Reduce clippy::type_complexity noise for local helpers.
type BvhItem<TS> = (usize, Aabb2D<TS>);
type BvhItems<TS> = Vec<BvhItem<TS>>;
type BvhBestSplit<TS> = Option<(crate::types::ScalarAcc<TS>, BvhItems<TS>, BvhItems<TS>)>;

const INLINE_STACK_CAP: usize = 64;

impl<T: Scalar> Bvh<T> {
    /// Ensure backing arrays are large enough for `slot` and record the latest AABB for it.
    fn ensure_slot(&mut self, slot: usize, bbox: Aabb2D<T>) {
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
        if self.slot_leaf.len() <= slot {
            self.slot_leaf.resize_with(slot + 1, || None);
        }
        self.slots[slot] = Some(bbox);
    }

    fn bbox_items(items: &[(usize, Aabb2D<T>)]) -> Aabb2D<T> {
        let mut it = items.iter();
        if let Some((_, b)) = it.next() {
            let mut acc = *b;
            for (_, bb) in it {
                acc = acc.union(*bb);
            }
            acc
        } else {
            Aabb2D::new(T::zero(), T::zero(), T::zero(), T::zero())
        }
    }

    fn bbox_children(arena: &[Node<T>], left: NodeIdx, right: NodeIdx) -> Aabb2D<T> {
        let l = &arena[left.get()];
        let r = &arena[right.get()];
        // Treat empty subtrees as "not contributing" to the parent bound.
        match (l.count, r.count) {
            (0, 0) => Aabb2D::new(T::zero(), T::zero(), T::zero(), T::zero()),
            (0, _) => r.bbox,
            (_, 0) => l.bbox,
            (_, _) => l.bbox.union(r.bbox),
        }
    }

    /// SAH-like split: sort along an axis, precompute prefix/suffix AABBs, and
    /// choose `k` that minimizes `area(LB_k) * k + area(RB_k) * (n - k)`.
    fn split_sah(mut items: BvhItems<T>, max_leaf: usize) -> (BvhItems<T>, BvhItems<T>) {
        let n = items.len();
        let min_children = (max_leaf / 2).max(2).min(n.saturating_sub(2));
        let mut best: BvhBestSplit<T> = None;
        for axis in 0..2 {
            items.sort_by(|a, b| {
                let ca = if axis == 0 {
                    Scalar::mid(a.1.min_x, a.1.max_x)
                } else {
                    Scalar::mid(a.1.min_y, a.1.max_y)
                };
                let cb = if axis == 0 {
                    Scalar::mid(b.1.min_x, b.1.max_x)
                } else {
                    Scalar::mid(b.1.min_y, b.1.max_y)
                };
                match ca.partial_cmp(&cb) {
                    Some(ord) => ord,
                    None => core::cmp::Ordering::Equal,
                }
            });

            // Precompute prefix/suffix bboxes for O(1) split evaluation
            let mut prefix: Vec<Aabb2D<T>> = Vec::with_capacity(n);
            for (i, (_, bb)) in items.iter().enumerate() {
                if i == 0 {
                    prefix.push(*bb);
                } else {
                    let prev = *prefix.last().unwrap();
                    prefix.push(prev.union(*bb));
                }
            }
            let mut suffix: Vec<Aabb2D<T>> = Vec::with_capacity(n);
            for (i, (_, bb)) in items.iter().enumerate().rev() {
                if i == n - 1 {
                    suffix.push(*bb);
                } else {
                    let prev = *suffix.last().unwrap();
                    suffix.push(prev.union(*bb));
                }
            }
            suffix.reverse();

            for k in min_children..=(n - min_children) {
                let lb = prefix[k - 1];
                let rb = suffix[k];
                let cost = lb.area() * T::acc_from_usize(k) + rb.area() * T::acc_from_usize(n - k);
                if best.as_ref().map(|(bc, _, _)| cost < *bc).unwrap_or(true) {
                    let left = items[..k].to_vec();
                    let right = items[k..].to_vec();
                    best = Some((cost, left, right));
                }
            }
        }
        let (_, l, r) = best.expect("BVH split requires at least 4 items");
        (l, r)
    }

    fn insert_node(
        arena: &mut Vec<Node<T>>,
        slot_leaf: &mut Vec<Option<NodeIdx>>,
        node_idx: usize,
        slot: usize,
        bbox: Aabb2D<T>,
        max_leaf: usize,
    ) {
        let kind = core::mem::replace(&mut arena[node_idx].kind, Kind::Leaf(Vec::new()));
        match kind {
            Kind::Leaf(mut items) => {
                items.push((slot, bbox));
                slot_leaf[slot] = Some(NodeIdx::new(node_idx));
                let mut node_bbox = if arena[node_idx].count == 0 {
                    bbox
                } else {
                    arena[node_idx].bbox.union(bbox)
                };
                arena[node_idx].count += 1;
                let new_kind = if items.len() > max_leaf {
                    // Split this leaf into two child leaves. We keep the existing node index as
                    // the internal parent so pointers to it remain valid.
                    let (l, r) = Self::split_sah(items, max_leaf);
                    let l_idx = arena.len();
                    arena.push(Node {
                        parent: Some(NodeIdx::new(node_idx)),
                        bbox: Self::bbox_items(&l),
                        count: l.len(),
                        kind: Kind::Leaf(l),
                    });
                    let r_idx = arena.len();
                    arena.push(Node {
                        parent: Some(NodeIdx::new(node_idx)),
                        bbox: Self::bbox_items(&r),
                        count: r.len(),
                        kind: Kind::Leaf(r),
                    });
                    let left = NodeIdx::new(l_idx);
                    let right = NodeIdx::new(r_idx);
                    // Rebuild slot→leaf mappings for the moved items.
                    for &(s, _) in match &arena[l_idx].kind {
                        Kind::Leaf(items) => items,
                        Kind::Internal { .. } => unreachable!("new nodes are leaves"),
                    } {
                        slot_leaf[s] = Some(left);
                    }
                    for &(s, _) in match &arena[r_idx].kind {
                        Kind::Leaf(items) => items,
                        Kind::Internal { .. } => unreachable!("new nodes are leaves"),
                    } {
                        slot_leaf[s] = Some(right);
                    }
                    arena[node_idx].count = arena[l_idx].count + arena[r_idx].count;
                    node_bbox = Self::bbox_children(arena, left, right);
                    Kind::Internal { left, right }
                } else {
                    Kind::Leaf(items)
                };
                arena[node_idx].kind = new_kind;
                arena[node_idx].bbox = node_bbox;
            }
            Kind::Internal { left, right } => {
                let lb = arena[left.get()].bbox;
                let rb = arena[right.get()].bbox;
                // Descend into whichever child would expand the least (greedy).
                let cost_l = lb.union(bbox).area() - lb.area();
                let cost_r = rb.union(bbox).area() - rb.area();
                if cost_l <= cost_r {
                    Self::insert_node(arena, slot_leaf, left.get(), slot, bbox, max_leaf);
                } else {
                    Self::insert_node(arena, slot_leaf, right.get(), slot, bbox, max_leaf);
                }
                arena[node_idx].count = arena[left.get()].count + arena[right.get()].count;
                let node_bbox = Self::bbox_children(arena, left, right);
                arena[node_idx].kind = Kind::Internal { left, right };
                arena[node_idx].bbox = node_bbox;
            }
        }
    }

    fn refit_upwards(&mut self, mut node_idx: NodeIdx) {
        // Walk up to the root recomputing parent bounds from their children. Stop early once the
        // parent is unchanged to avoid doing work for stable subtrees.
        loop {
            let parent = self.arena[node_idx.get()].parent;
            let Some(parent_idx) = parent else {
                break;
            };
            let parent_i = parent_idx.get();

            let Kind::Internal { left, right } = self.arena[parent_i].kind else {
                break;
            };
            let new_count = self.arena[left.get()].count + self.arena[right.get()].count;
            let new_bbox = Self::bbox_children(&self.arena, left, right);

            let old_count = self.arena[parent_i].count;
            let old_bbox = self.arena[parent_i].bbox;
            self.arena[parent_i].count = new_count;
            self.arena[parent_i].bbox = new_bbox;

            if old_count == new_count && old_bbox == new_bbox {
                break;
            }
            node_idx = parent_idx;
        }
    }
}

impl<T: Scalar> Backend<T> for Bvh<T> {
    fn insert(&mut self, slot: usize, aabb: Aabb2D<T>) {
        self.ensure_slot(slot, aabb);
        match self.root {
            None => {
                // First insert creates a single-leaf tree.
                let idx = self.arena.len();
                self.arena.push(Node {
                    parent: None,
                    bbox: aabb,
                    count: 1,
                    kind: Kind::Leaf(vec![(slot, aabb)]),
                });
                self.root = Some(NodeIdx::new(idx));
                self.slot_leaf[slot] = Some(NodeIdx::new(idx));
            }
            Some(root_idx) => {
                Self::insert_node(
                    &mut self.arena,
                    &mut self.slot_leaf,
                    root_idx.get(),
                    slot,
                    aabb,
                    self.max_leaf,
                );
            }
        }
    }

    fn update(&mut self, slot: usize, aabb: Aabb2D<T>) {
        let Some(leaf) = self.slot_leaf.get(slot).and_then(|v| *v) else {
            self.insert(slot, aabb);
            return;
        };
        self.ensure_slot(slot, aabb);

        let leaf_i = leaf.get();
        let (new_bbox, new_count) = {
            let Kind::Leaf(items) = &mut self.arena[leaf_i].kind else {
                self.insert(slot, aabb);
                return;
            };
            let mut found = false;
            for (s, bb) in items.iter_mut() {
                if *s == slot {
                    *bb = aabb;
                    found = true;
                    break;
                }
            }
            if !found {
                self.insert(slot, aabb);
                return;
            }
            (Self::bbox_items(items), items.len())
        };

        self.arena[leaf_i].bbox = new_bbox;
        self.arena[leaf_i].count = new_count;
        self.refit_upwards(leaf);
    }

    fn remove(&mut self, slot: usize) {
        let Some(leaf) = self.slot_leaf.get_mut(slot).and_then(|v| v.take()) else {
            return;
        };
        if let Some(s) = self.slots.get_mut(slot) {
            *s = None;
        }

        let leaf_i = leaf.get();
        let (new_bbox, new_count) = {
            let Kind::Leaf(items) = &mut self.arena[leaf_i].kind else {
                return;
            };
            items.retain(|(s, _)| *s != slot);
            (Self::bbox_items(items), items.len())
        };
        self.arena[leaf_i].bbox = new_bbox;
        self.arena[leaf_i].count = new_count;
        self.refit_upwards(leaf);

        if let Some(root) = self.root
            && self.arena[root.get()].count == 0
        {
            // The tree is empty; reset to keep memory use bounded and keep invariants simple.
            self.root = None;
            self.arena.clear();
            self.slots.clear();
            self.slot_leaf.clear();
        }
    }

    fn clear(&mut self) {
        self.root = None;
        self.arena.clear();
        self.slots.clear();
        self.slot_leaf.clear();
    }

    fn visit_point<F: FnMut(usize)>(&self, x: T, y: T, mut f: F) {
        let Some(root_idx) = self.root else {
            return;
        };
        if self.arena[root_idx.get()].count == 0 {
            return;
        }
        // Early-exit: if the point is outside the root bound, nothing can match.
        if !self.arena[root_idx.get()].bbox.contains_point(x, y) {
            return;
        }
        let mut inline = [root_idx; INLINE_STACK_CAP];
        let mut inline_len = 1_usize;
        let mut heap_stack = Vec::new();

        while let Some(i) = heap_stack.pop().or_else(|| {
            (inline_len > 0).then(|| {
                inline_len -= 1;
                inline[inline_len]
            })
        }) {
            let n = &self.arena[i.get()];
            if n.count == 0 {
                continue;
            }
            match &n.kind {
                Kind::Leaf(items) => {
                    for (s, b) in items {
                        if b.contains_point(x, y) {
                            f(*s);
                        }
                    }
                }
                Kind::Internal { left, right } => {
                    // Prefilter children before pushing them: avoids work in high-fanout trees.
                    let lb = self.arena[left.get()].bbox;
                    let rb = self.arena[right.get()].bbox;
                    if rb.contains_point(x, y) {
                        if !heap_stack.is_empty() || inline_len == inline.len() {
                            heap_stack.push(*right);
                        } else {
                            inline[inline_len] = *right;
                            inline_len += 1;
                        }
                    }
                    if lb.contains_point(x, y) {
                        if !heap_stack.is_empty() || inline_len == inline.len() {
                            heap_stack.push(*left);
                        } else {
                            inline[inline_len] = *left;
                            inline_len += 1;
                        }
                    }
                }
            }
        }
    }

    fn visit_rect<F: FnMut(usize)>(&self, rect: Aabb2D<T>, mut f: F) {
        let Some(root_idx) = self.root else {
            return;
        };
        if self.arena[root_idx.get()].count == 0 {
            return;
        }
        // Early-exit: if the query rect doesn't overlap the root bound, nothing can match.
        if !self.arena[root_idx.get()].bbox.overlaps(&rect) {
            return;
        }
        let mut inline = [root_idx; INLINE_STACK_CAP];
        let mut inline_len = 1_usize;
        let mut heap_stack = Vec::new();

        while let Some(i) = heap_stack.pop().or_else(|| {
            (inline_len > 0).then(|| {
                inline_len -= 1;
                inline[inline_len]
            })
        }) {
            let n = &self.arena[i.get()];
            if n.count == 0 {
                continue;
            }
            match &n.kind {
                Kind::Leaf(items) => {
                    for (s, b) in items {
                        if b.overlaps(&rect) {
                            f(*s);
                        }
                    }
                }
                Kind::Internal { left, right } => {
                    // Prefilter children before pushing them: avoids work in high-fanout trees.
                    let lb = self.arena[left.get()].bbox;
                    let rb = self.arena[right.get()].bbox;
                    if rb.overlaps(&rect) {
                        if !heap_stack.is_empty() || inline_len == inline.len() {
                            heap_stack.push(*right);
                        } else {
                            inline[inline_len] = *right;
                            inline_len += 1;
                        }
                    }
                    if lb.overlaps(&rect) {
                        if !heap_stack.is_empty() || inline_len == inline.len() {
                            heap_stack.push(*left);
                        } else {
                            inline[inline_len] = *left;
                            inline_len += 1;
                        }
                    }
                }
            }
        }
    }
}

impl<T: Scalar> Debug for Bvh<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let total = self.slots.len();
        let alive = self.slots.iter().filter(|e| e.is_some()).count();
        let has_root = self.root.is_some();
        f.debug_struct("Bvh")
            .field("max_leaf", &self.max_leaf)
            .field("arena_nodes", &self.arena.len())
            .field("total_slots", &total)
            .field("alive", &alive)
            .field("has_root", &has_root)
            .finish_non_exhaustive()
    }
}

/// Convenience type aliases for common scalar choices.
/// BVH with f32 coordinates and f64 metrics.
pub type BvhF32 = Bvh<f32>;

/// BVH with f64 coordinates and f64 metrics.
pub type BvhF64 = Bvh<f64>;

/// BVH with i64 coordinates and i128 metrics.
pub type BvhI64 = Bvh<i64>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;

    #[test]
    fn bvh_f64_basic() {
        let mut idx = Index::<f64, u32>::with_bvh();
        let _k1 = idx.insert(Aabb2D::new(0.0, 0.0, 10.0, 10.0), 1);
        let _k2 = idx.insert(Aabb2D::new(5.0, 5.0, 15.0, 15.0), 2);
        let _ = idx.commit();
        let hits: Vec<_> = idx.query_point(6.0, 6.0).collect();
        assert!(hits.len() >= 2);
        let q: Vec<_> = idx
            .query_rect(Aabb2D::new(12.0, 12.0, 20.0, 20.0))
            .collect();
        assert!(!q.is_empty());
    }

    #[test]
    fn bvh_f64_update_move_correctness() {
        // Use backend directly to inspect structure behavior on updates.
        let mut b: Bvh<f64> = Bvh::default();
        b.insert(0, Aabb2D::new(0.0, 0.0, 10.0, 10.0));
        b.insert(1, Aabb2D::new(12.0, 0.0, 22.0, 10.0));

        let arena_before = b.arena.len();
        let root_leaf_before = b
            .root
            .map(|ri| matches!(b.arena[ri.get()].kind, Kind::Leaf(_)))
            .unwrap_or(false);

        // Move slot 0 far away; our remove+insert path should keep a valid tree
        // without gratuitous node growth for this tiny case.
        b.update(0, Aabb2D::new(100.0, 100.0, 110.0, 110.0));

        // Arena size should not grow for this small case; root leaf-ness unchanged or becomes internal
        // is acceptable, but for two items it should remain a leaf.
        assert_eq!(b.arena.len(), arena_before);
        let root_leaf_after = b
            .root
            .map(|ri| matches!(b.arena[ri.get()].kind, Kind::Leaf(_)))
            .unwrap_or(false);
        assert_eq!(root_leaf_after, root_leaf_before);

        // Query correctness
        let v_old: Vec<_> = b.query_point(5.0, 5.0).collect();
        assert!(v_old.is_empty());
        let v_new: Vec<_> = b.query_point(105.0, 105.0).collect();
        assert_eq!(v_new, vec![0]);
        let v_neighbor: Vec<_> = b.query_point(15.0, 5.0).collect();
        assert_eq!(v_neighbor, vec![1]);
    }

    #[test]
    fn bvh_i64_update_churn_small() {
        let mut b: Bvh<i64> = Bvh::default();
        b.insert(0, Aabb2D::new(0, 0, 10, 10));
        b.insert(1, Aabb2D::new(12, 0, 22, 10));
        let baseline_nodes = b.arena.len();

        // Move slot 0 back and forth a few times.
        for _ in 0..10 {
            b.update(0, Aabb2D::new(100, 100, 110, 110));
            b.update(0, Aabb2D::new(0, 0, 10, 10));
        }

        // Query correctness stays intact.
        let here: Vec<_> = b.query_point(5, 5).collect();
        assert_eq!(here, vec![0]);
        let there: Vec<_> = b.query_point(105, 105).collect();
        assert!(there.is_empty());

        // Arena size should not explode under small churn.
        assert!(b.arena.len() <= baseline_nodes + 2);
    }

    #[test]
    fn bvh_f64_split_then_updates_on_internal() {
        // Force a split by exceeding max_leaf (8), then update several items and
        // verify the internal-node tree remains correct.
        let mut b: Bvh<f64> = Bvh::default();

        // Build 12 non-overlapping AABBs along the x-axis
        let n = 12_usize;
        let mut current: Vec<Aabb2D<f64>> = Vec::with_capacity(n);
        for i in 0..n {
            let x0 = (i as f64) * 20.0;
            let a = Aabb2D::new(x0, 0.0, x0 + 10.0, 10.0);
            current.push(a);
            b.insert(i, a);
        }

        // Ensure we created an internal root with two children after split
        let root = b.root.expect("root exists").get();
        match b.arena[root].kind {
            Kind::Internal { left, right } => {
                assert!(matches!(b.arena[left.get()].kind, Kind::Leaf(_)));
                assert!(matches!(b.arena[right.get()].kind, Kind::Leaf(_)));
            }
            _ => panic!("expected internal root after split"),
        }

        let baseline_nodes = b.arena.len();

        // Move three items far away (to another cluster)
        for &i in &[0_usize, 5, 9] {
            let new_bb = Aabb2D::new(
                1000.0 + i as f64 * 5.0,
                1000.0,
                1010.0 + i as f64 * 5.0,
                1010.0,
            );
            b.update(i, new_bb);
            current[i] = new_bb;
        }

        // Validate: each item's midpoint hits exactly that slot
        for (i, bb) in current.iter().enumerate() {
            let mx = (bb.min_x + bb.max_x) * 0.5;
            let my = (bb.min_y + bb.max_y) * 0.5;
            let hits: Vec<_> = b.query_point(mx, my).collect();
            assert_eq!(hits, vec![i], "midpoint lookup must return the slot itself");
        }

        // Structure sanity: arena should not grow unboundedly due to updates
        assert!(b.arena.len() <= baseline_nodes + 4);
    }
}
