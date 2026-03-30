// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cheap pager adapters.
//!
//! The adapters in this module preserve the keyed navigation model and avoid
//! hidden storage or policy. They are intentionally biased toward operations
//! whose cost model is easy to explain.
//!
//! TODO: Revisit `FilterPager` and `FlattenPager` after first real call sites
//! establish the right invalidation and scan-cost story.

use core::fmt;

use crate::{HasLastKey, KeyPosition, KnownLength, Pager};

/// An adapter that transforms resolved items while preserving keys and navigation.
pub struct MapPager<P, F> {
    inner: P,
    map: F,
}

impl<P, F> MapPager<P, F> {
    /// Creates a new mapping adapter.
    #[must_use]
    pub const fn new(inner: P, map: F) -> Self {
        Self { inner, map }
    }

    /// Returns a shared reference to the wrapped pager.
    #[must_use]
    pub const fn inner(&self) -> &P {
        &self.inner
    }

    /// Consumes the adapter and returns the wrapped pager and mapping function.
    #[must_use]
    pub fn into_parts(self) -> (P, F) {
        (self.inner, self.map)
    }
}

impl<P, F> fmt::Debug for MapPager<P, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapPager").finish_non_exhaustive()
    }
}

impl<P, F, T> Pager for MapPager<P, F>
where
    P: Pager,
    F: Fn(P::Item) -> T,
{
    type Key = P::Key;
    type Item = T;

    fn first_key(&self) -> Option<Self::Key> {
        self.inner.first_key()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        self.inner.contains_key(key)
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.inner.next_key(key)
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.inner.prev_key(key)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.inner.item(key).map(&self.map)
    }
}

impl<P, F, T> HasLastKey for MapPager<P, F>
where
    P: HasLastKey,
    F: Fn(P::Item) -> T,
{
    fn last_key(&self) -> Option<Self::Key> {
        self.inner.last_key()
    }
}

impl<P, F, T> KnownLength for MapPager<P, F>
where
    P: KnownLength,
    F: Fn(P::Item) -> T,
{
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<P, F, T> KeyPosition for MapPager<P, F>
where
    P: KeyPosition,
    F: Fn(P::Item) -> T,
{
    fn key_at_index(&self, index: usize) -> Option<Self::Key> {
        self.inner.key_at_index(index)
    }

    fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
        self.inner.index_of_key(key)
    }
}

/// An adapter that maps resolved items with a fallible transform.
///
/// Navigation still delegates directly to the wrapped pager; only item
/// resolution may fail.
pub struct TryMapPager<P, F> {
    inner: P,
    map: F,
}

impl<P, F> TryMapPager<P, F> {
    /// Creates a new fallible mapping adapter.
    #[must_use]
    pub const fn new(inner: P, map: F) -> Self {
        Self { inner, map }
    }
}

impl<P, F> fmt::Debug for TryMapPager<P, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryMapPager").finish_non_exhaustive()
    }
}

impl<P, F, T> Pager for TryMapPager<P, F>
where
    P: Pager,
    F: Fn(P::Item) -> Option<T>,
{
    type Key = P::Key;
    type Item = T;

    fn first_key(&self) -> Option<Self::Key> {
        self.inner.first_key()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        self.inner.contains_key(key)
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.inner.next_key(key)
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.inner.prev_key(key)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.inner.item(key).and_then(&self.map)
    }
}

impl<P, F, T> HasLastKey for TryMapPager<P, F>
where
    P: HasLastKey,
    F: Fn(P::Item) -> Option<T>,
{
    fn last_key(&self) -> Option<Self::Key> {
        self.inner.last_key()
    }
}

impl<P, F, T> KnownLength for TryMapPager<P, F>
where
    P: KnownLength,
    F: Fn(P::Item) -> Option<T>,
{
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<P, F, T> KeyPosition for TryMapPager<P, F>
where
    P: KeyPosition,
    F: Fn(P::Item) -> Option<T>,
{
    fn key_at_index(&self, index: usize) -> Option<Self::Key> {
        self.inner.key_at_index(index)
    }

    fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
        self.inner.index_of_key(key)
    }
}

/// Provenance-preserving key for [`ChainPager`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainKey<KA, KB> {
    /// A key from the first pager.
    First(KA),
    /// A key from the second pager.
    Second(KB),
}

/// An adapter that concatenates two pagers.
///
/// Reverse traversal across the boundary requires the first pager to implement
/// [`HasLastKey`], so `ChainPager` carries that requirement in its [`Pager`]
/// implementation rather than scanning implicitly.
pub struct ChainPager<A, B> {
    first: A,
    second: B,
}

impl<A, B> ChainPager<A, B> {
    /// Creates a new chained pager.
    #[must_use]
    pub const fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

impl<A, B> fmt::Debug for ChainPager<A, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainPager").finish_non_exhaustive()
    }
}

impl<A, B> Pager for ChainPager<A, B>
where
    A: HasLastKey,
    B: Pager<Item = A::Item>,
{
    type Key = ChainKey<A::Key, B::Key>;
    type Item = A::Item;

    fn first_key(&self) -> Option<Self::Key> {
        if let Some(key) = self.first.first_key() {
            Some(ChainKey::First(key))
        } else {
            self.second.first_key().map(ChainKey::Second)
        }
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        match key {
            ChainKey::First(key) => self.first.contains_key(key),
            ChainKey::Second(key) => self.second.contains_key(key),
        }
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match key {
            ChainKey::First(key) => self
                .first
                .next_key(key)
                .map(ChainKey::First)
                .or_else(|| self.second.first_key().map(ChainKey::Second)),
            ChainKey::Second(key) => self.second.next_key(key).map(ChainKey::Second),
        }
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match key {
            ChainKey::First(key) => self.first.prev_key(key).map(ChainKey::First),
            ChainKey::Second(key) => self
                .second
                .prev_key(key)
                .map(ChainKey::Second)
                .or_else(|| self.first.last_key().map(ChainKey::First)),
        }
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        match key {
            ChainKey::First(key) => self.first.item(key),
            ChainKey::Second(key) => self.second.item(key),
        }
    }
}

impl<A, B> HasLastKey for ChainPager<A, B>
where
    A: HasLastKey,
    B: Pager<Item = A::Item> + HasLastKey,
{
    fn last_key(&self) -> Option<Self::Key> {
        if let Some(key) = self.second.last_key() {
            Some(ChainKey::Second(key))
        } else {
            self.first.last_key().map(ChainKey::First)
        }
    }
}

impl<A, B> KnownLength for ChainPager<A, B>
where
    A: KnownLength + HasLastKey,
    B: KnownLength + Pager<Item = A::Item>,
{
    fn len(&self) -> usize {
        self.first.len() + self.second.len()
    }
}

/// An adapter that reverses traversal direction.
pub struct ReversePager<P> {
    inner: P,
}

impl<P> ReversePager<P> {
    /// Creates a new reversing adapter.
    #[must_use]
    pub const fn new(inner: P) -> Self {
        Self { inner }
    }
}

impl<P> fmt::Debug for ReversePager<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReversePager").finish_non_exhaustive()
    }
}

impl<P> Pager for ReversePager<P>
where
    P: HasLastKey,
{
    type Key = P::Key;
    type Item = P::Item;

    fn first_key(&self) -> Option<Self::Key> {
        self.inner.last_key()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        self.inner.contains_key(key)
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.inner.prev_key(key)
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.inner.next_key(key)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.inner.item(key)
    }
}

impl<P> HasLastKey for ReversePager<P>
where
    P: HasLastKey,
{
    fn last_key(&self) -> Option<Self::Key> {
        self.inner.first_key()
    }
}

impl<P> KnownLength for ReversePager<P>
where
    P: KnownLength + HasLastKey,
{
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<P> KeyPosition for ReversePager<P>
where
    P: KeyPosition + KnownLength + HasLastKey,
{
    fn key_at_index(&self, index: usize) -> Option<Self::Key> {
        let len = self.inner.len();
        let reversed = len.checked_sub(index + 1)?;
        self.inner.key_at_index(reversed)
    }

    fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
        let len = self.inner.len();
        let index = self.inner.index_of_key(key)?;
        len.checked_sub(index + 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::{HasLastKey, Pager, PagerExt, SlicePager};

    use super::ChainKey;

    #[test]
    fn map_preserves_keys() {
        let pager = SlicePager::new(&["alpha", "beta"]).map(|value: &&str| value.len());

        assert_eq!(pager.first_key(), Some(0));
        assert_eq!(pager.item(&0), Some(5));
        assert_eq!(pager.item(&1), Some(4));
    }

    #[test]
    fn try_map_can_drop_resolution() {
        let pager = SlicePager::new(&["alpha", "beta"])
            .try_map(|value: &&str| value.starts_with('a').then_some(value.len()));

        assert_eq!(pager.item(&0), Some(5));
        assert_eq!(pager.item(&1), None);
    }

    #[test]
    fn chain_crosses_the_boundary_in_both_directions() {
        let pager = SlicePager::new(&["a", "b"]).chain(SlicePager::new(&["c"]));

        assert_eq!(pager.first_key(), Some(ChainKey::First(0)));
        assert_eq!(
            pager.next_key(&ChainKey::First(1)),
            Some(ChainKey::Second(0))
        );
        assert_eq!(
            pager.prev_key(&ChainKey::Second(0)),
            Some(ChainKey::First(1))
        );
        assert_eq!(pager.last_key(), Some(ChainKey::Second(0)));
        assert_eq!(pager.item(&ChainKey::Second(0)), Some(&"c"));
    }

    #[test]
    fn reverse_flips_direction() {
        let pager = SlicePager::new(&["a", "b", "c"]).reverse();

        assert_eq!(pager.first_key(), Some(2));
        assert_eq!(pager.last_key(), Some(0));
        assert_eq!(pager.next_key(&2), Some(1));
        assert_eq!(pager.prev_key(&1), Some(2));
        assert_eq!(pager.item(&0), Some(&"a"));
    }
}
