// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dense, index-oriented pager implementations.

use crate::{HasLastKey, KeyPosition, KnownLength, Pager};

/// A simple pager over a shared slice.
///
/// `SlicePager` is the reference implementation for this crate's core traits:
/// keys are dense `usize` indices, navigation is constant-time, and resolving an
/// item borrows directly from the underlying slice.
#[derive(Clone, Copy, Debug)]
pub struct SlicePager<'a, T> {
    items: &'a [T],
}

impl<'a, T> SlicePager<'a, T> {
    /// Creates a pager over `items`.
    #[must_use]
    pub const fn new(items: &'a [T]) -> Self {
        Self { items }
    }

    /// Returns the underlying slice.
    #[must_use]
    pub const fn items(&self) -> &'a [T] {
        self.items
    }
}

impl<'a, T> Pager for SlicePager<'a, T> {
    type Key = usize;
    type Item = &'a T;

    fn first_key(&self) -> Option<Self::Key> {
        (!self.items.is_empty()).then_some(0)
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        *key < self.items.len()
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        if !self.contains_key(key) {
            return None;
        }
        let next = *key + 1;
        (next < self.items.len()).then_some(next)
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        if !self.contains_key(key) {
            return None;
        }
        key.checked_sub(1)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.items.get(*key)
    }
}

impl<T> HasLastKey for SlicePager<'_, T> {
    fn last_key(&self) -> Option<Self::Key> {
        self.items.len().checked_sub(1)
    }
}

impl<T> KnownLength for SlicePager<'_, T> {
    fn len(&self) -> usize {
        self.items.len()
    }
}

impl<T> KeyPosition for SlicePager<'_, T> {
    fn key_at_index(&self, index: usize) -> Option<Self::Key> {
        self.contains_key(&index).then_some(index)
    }

    fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
        self.contains_key(key).then_some(*key)
    }
}

#[cfg(test)]
mod tests {
    use crate::{HasLastKey, KeyPosition, KnownLength, Pager};

    use super::SlicePager;

    #[test]
    fn slice_navigation_and_lookup() {
        let pager = SlicePager::new(&["a", "b", "c"]);

        assert_eq!(pager.first_key(), Some(0));
        assert_eq!(pager.last_key(), Some(2));
        assert_eq!(pager.next_key(&0), Some(1));
        assert_eq!(pager.prev_key(&2), Some(1));
        assert_eq!(pager.item(&1), Some(&"b"));
        assert!(pager.contains_key(&2));
        assert!(!pager.contains_key(&9));
    }

    #[test]
    fn slice_exposes_index_capabilities() {
        let pager = SlicePager::new(&[10, 20, 30]);

        assert_eq!(pager.len(), 3);
        assert!(!pager.is_empty());
        assert_eq!(pager.key_at_index(1), Some(1));
        assert_eq!(pager.index_of_key(&2), Some(2));
        assert_eq!(pager.index_of_key(&9), None);
    }
}
