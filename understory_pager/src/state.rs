// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cursor and navigation helpers.

use crate::{HasLastKey, Pager};

/// External state describing the current key in a pager.
///
/// `PagerCursor` is intentionally small and policy-free. It does not store
/// history, selection ranges, or UI state; higher layers can compose those
/// concepts around it as needed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PagerCursor<K> {
    current: Option<K>,
}

impl<K> PagerCursor<K> {
    /// Creates an empty cursor.
    #[must_use]
    pub const fn new() -> Self {
        Self { current: None }
    }

    /// Creates a cursor with an initial current key.
    #[must_use]
    pub const fn with_current(current: Option<K>) -> Self {
        Self { current }
    }

    /// Returns the current key, if any.
    #[must_use]
    pub fn current(&self) -> Option<&K> {
        self.current.as_ref()
    }

    /// Replaces the current key.
    pub fn set_current(&mut self, current: Option<K>) {
        self.current = current;
    }

    /// Clears the current key.
    pub fn clear(&mut self) {
        self.current = None;
    }
}

fn replace_current<K: Eq>(cursor: &mut PagerCursor<K>, next: Option<K>) -> bool {
    if cursor.current == next {
        false
    } else {
        cursor.current = next;
        true
    }
}

/// Moves the cursor to the first key in `pager`.
pub fn move_first<P>(pager: &P, cursor: &mut PagerCursor<P::Key>) -> bool
where
    P: Pager,
{
    replace_current(cursor, pager.first_key())
}

/// Moves the cursor to the last key in `pager`.
pub fn move_last<P>(pager: &P, cursor: &mut PagerCursor<P::Key>) -> bool
where
    P: HasLastKey,
{
    replace_current(cursor, pager.last_key())
}

/// Moves the cursor to the next key in `pager`.
///
/// If the cursor is empty or stale, this attempts to recover by moving to the
/// first key.
pub fn move_next<P>(pager: &P, cursor: &mut PagerCursor<P::Key>) -> bool
where
    P: Pager,
{
    let next = match cursor.current() {
        Some(current) if pager.contains_key(current) => pager.next_key(current),
        _ => pager.first_key(),
    };
    replace_current(cursor, next)
}

/// Moves the cursor to the previous key in `pager`.
///
/// If the cursor is stale or empty, this leaves the cursor unchanged. Call
/// [`move_last`] when you want an explicit “jump to end” operation.
pub fn move_prev<P>(pager: &P, cursor: &mut PagerCursor<P::Key>) -> bool
where
    P: Pager,
{
    let Some(current) = cursor.current() else {
        return false;
    };
    let Some(previous) = pager
        .contains_key(current)
        .then(|| pager.prev_key(current))
        .flatten()
    else {
        return false;
    };
    replace_current(cursor, Some(previous))
}

/// Resolves the item for the cursor's current key, if it is still valid.
pub fn resolve_current<P>(pager: &P, cursor: &PagerCursor<P::Key>) -> Option<P::Item>
where
    P: Pager,
{
    let key = cursor.current()?;
    pager.item(key)
}

#[cfg(test)]
mod tests {
    use crate::{Pager, SlicePager};

    use super::{PagerCursor, move_first, move_last, move_next, move_prev, resolve_current};

    #[test]
    fn cursor_moves_forward_and_resolves() {
        let pager = SlicePager::new(&["a", "b"]);
        let mut cursor = PagerCursor::new();

        assert!(move_next(&pager, &mut cursor));
        assert_eq!(cursor.current(), Some(&0));
        assert_eq!(resolve_current(&pager, &cursor), Some(&"a"));
        assert!(move_next(&pager, &mut cursor));
        assert_eq!(cursor.current(), Some(&1));
    }

    #[test]
    fn cursor_can_jump_to_edges() {
        let pager = SlicePager::new(&["a", "b", "c"]);
        let mut cursor = PagerCursor::new();

        assert!(move_last(&pager, &mut cursor));
        assert_eq!(cursor.current(), Some(&2));
        assert!(move_prev(&pager, &mut cursor));
        assert_eq!(cursor.current(), Some(&1));
        assert!(move_first(&pager, &mut cursor));
        assert_eq!(cursor.current(), Some(&0));
    }

    #[test]
    fn stale_cursor_recovers_on_next() {
        #[derive(Clone)]
        struct SingleKeyPager;

        impl Pager for SingleKeyPager {
            type Key = u8;
            type Item = u8;

            fn first_key(&self) -> Option<Self::Key> {
                Some(7)
            }

            fn contains_key(&self, key: &Self::Key) -> bool {
                *key == 7
            }

            fn next_key(&self, _key: &Self::Key) -> Option<Self::Key> {
                None
            }

            fn prev_key(&self, _key: &Self::Key) -> Option<Self::Key> {
                None
            }

            fn item(&self, key: &Self::Key) -> Option<Self::Item> {
                self.contains_key(key).then_some(*key)
            }
        }

        let pager = SingleKeyPager;
        let mut cursor = PagerCursor::with_current(Some(99));
        assert!(move_next(&pager, &mut cursor));
        assert_eq!(cursor.current(), Some(&7));
    }
}
