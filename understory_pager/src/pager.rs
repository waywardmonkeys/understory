// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core pager traits.

use crate::adapters::{ChainPager, MapPager, ReversePager, TryMapPager};

/// A keyed, discrete, revisitable sequence.
///
/// `Pager` is the calm center of this crate. It models navigation over stable
/// keys without assuming that the sequence:
///
/// - is densely indexed,
/// - has a known total length,
/// - can be cheaply materialized in full.
///
/// The trait deliberately separates navigation from cursor state. Implementors
/// define how keys relate to one another and how a key resolves to an item;
/// higher layers decide which key is “current”.
pub trait Pager {
    /// Stable key type used to revisit items.
    type Key: Clone + Eq;

    /// Item yielded when a key is resolved.
    type Item;

    /// Returns the first key in the sequence, if any.
    fn first_key(&self) -> Option<Self::Key>;

    /// Returns `true` if `key` is currently part of this pager.
    ///
    /// This method exists so cursor-like state can cheaply revalidate a key
    /// without forcing item hydration.
    fn contains_key(&self, key: &Self::Key) -> bool;

    /// Returns the next key after `key`, if any.
    fn next_key(&self, key: &Self::Key) -> Option<Self::Key>;

    /// Returns the previous key before `key`, if any.
    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key>;

    /// Resolves the item associated with `key`.
    ///
    /// Returning `None` for stale or absent keys is expected behavior.
    fn item(&self, key: &Self::Key) -> Option<Self::Item>;
}

impl<P> Pager for &P
where
    P: Pager + ?Sized,
{
    type Key = P::Key;
    type Item = P::Item;

    fn first_key(&self) -> Option<Self::Key> {
        (**self).first_key()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        (**self).contains_key(key)
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).next_key(key)
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).prev_key(key)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        (**self).item(key)
    }
}

impl<P> Pager for &mut P
where
    P: Pager + ?Sized,
{
    type Key = P::Key;
    type Item = P::Item;

    fn first_key(&self) -> Option<Self::Key> {
        (**self).first_key()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        (**self).contains_key(key)
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).next_key(key)
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).prev_key(key)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        (**self).item(key)
    }
}

/// Capability trait for pagers that can cheaply identify their last key.
pub trait HasLastKey: Pager {
    /// Returns the last key in the sequence, if any.
    fn last_key(&self) -> Option<Self::Key>;
}

impl<P> HasLastKey for &P
where
    P: HasLastKey + ?Sized,
{
    fn last_key(&self) -> Option<Self::Key> {
        (**self).last_key()
    }
}

impl<P> HasLastKey for &mut P
where
    P: HasLastKey + ?Sized,
{
    fn last_key(&self) -> Option<Self::Key> {
        (**self).last_key()
    }
}

/// Capability trait for pagers with a known total number of keys.
pub trait KnownLength: Pager {
    /// Returns the total number of keys in this pager.
    fn len(&self) -> usize;

    /// Returns `true` if the pager has no keys.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<P> KnownLength for &P
where
    P: KnownLength + ?Sized,
{
    fn len(&self) -> usize {
        (**self).len()
    }
}

impl<P> KnownLength for &mut P
where
    P: KnownLength + ?Sized,
{
    fn len(&self) -> usize {
        (**self).len()
    }
}

/// Capability trait for pagers that support positional lookup.
///
/// This trait is intentionally separate from [`Pager`] because many useful
/// pagers can navigate by key without offering cheap indexing.
pub trait KeyPosition: Pager {
    /// Returns the key at `index`, if any.
    fn key_at_index(&self, index: usize) -> Option<Self::Key>;

    /// Returns the index associated with `key`, if known.
    fn index_of_key(&self, key: &Self::Key) -> Option<usize>;
}

impl<P> KeyPosition for &P
where
    P: KeyPosition + ?Sized,
{
    fn key_at_index(&self, index: usize) -> Option<Self::Key> {
        (**self).key_at_index(index)
    }

    fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
        (**self).index_of_key(key)
    }
}

impl<P> KeyPosition for &mut P
where
    P: KeyPosition + ?Sized,
{
    fn key_at_index(&self, index: usize) -> Option<Self::Key> {
        (**self).key_at_index(index)
    }

    fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
        (**self).index_of_key(key)
    }
}

/// Extension methods for composing pagers.
pub trait PagerExt: Pager + Sized {
    /// Maps resolved items while preserving keys and navigation.
    fn map<F, T>(self, map: F) -> MapPager<Self, F>
    where
        F: Fn(Self::Item) -> T,
    {
        MapPager::new(self, map)
    }

    /// Maps resolved items with a fallible transform.
    fn try_map<F, T>(self, map: F) -> TryMapPager<Self, F>
    where
        F: Fn(Self::Item) -> Option<T>,
    {
        TryMapPager::new(self, map)
    }

    /// Concatenates two pagers.
    ///
    /// The left pager must expose its last key so reverse traversal can cross
    /// the boundary without hidden scans.
    fn chain<P2>(self, other: P2) -> ChainPager<Self, P2>
    where
        Self: HasLastKey,
        P2: Pager<Item = Self::Item>,
    {
        ChainPager::new(self, other)
    }

    /// Reverses the traversal direction of a pager.
    fn reverse(self) -> ReversePager<Self>
    where
        Self: HasLastKey,
    {
        ReversePager::new(self)
    }

    // TODO: Add filtering once first-use call sites clarify the right stale-key
    // and scan-cost story.
    //
    // TODO: Add flattening only after grouped flows demonstrate a stable
    // key-pair representation and boundary behavior.
}

impl<P: Pager> PagerExt for P {}
