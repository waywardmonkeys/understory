// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Immediate pager neighborhoods.

use crate::{Pager, PagerCursor};

/// The immediate neighborhood around a current key.
///
/// This type is intentionally tiny: it captures only the previous/current/next
/// keys around a cursor. Larger, policy-heavy neighborhoods are left for a later
/// revision once host code demonstrates a stable need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagerWindow<K> {
    /// The key immediately before `current`, if any.
    pub previous: Option<K>,
    /// The cursor's current key, if it is still valid.
    pub current: Option<K>,
    /// The key immediately after `current`, if any.
    pub next: Option<K>,
}

impl<K> PagerWindow<K> {
    /// Creates a window from its three explicit components.
    #[must_use]
    pub const fn new(previous: Option<K>, current: Option<K>, next: Option<K>) -> Self {
        Self {
            previous,
            current,
            next,
        }
    }

    /// Returns `true` if the window has no current key.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.current.is_none()
    }
}

impl<K> Default for PagerWindow<K> {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}

impl<K: Clone> PagerWindow<K> {
    /// Derives an immediate neighborhood around `cursor`.
    ///
    /// If the cursor is empty or stale, the returned window is empty.
    #[must_use]
    pub fn from_cursor<P>(pager: &P, cursor: &PagerCursor<K>) -> Self
    where
        P: Pager<Key = K>,
    {
        let Some(current) = cursor.current() else {
            return Self::default();
        };
        if !pager.contains_key(current) {
            return Self::default();
        }

        Self {
            previous: pager.prev_key(current),
            current: Some(current.clone()),
            next: pager.next_key(current),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{PagerCursor, SlicePager};

    use super::PagerWindow;

    #[test]
    fn derives_neighbor_keys() {
        let pager = SlicePager::new(&["a", "b", "c"]);
        let cursor = PagerCursor::with_current(Some(1));

        let window = PagerWindow::from_cursor(&pager, &cursor);
        assert_eq!(window.previous, Some(0));
        assert_eq!(window.current, Some(1));
        assert_eq!(window.next, Some(2));
    }

    #[test]
    fn stale_cursor_yields_empty_window() {
        let pager = SlicePager::new(&["a", "b"]);
        let cursor = PagerCursor::with_current(Some(9));

        let window = PagerWindow::from_cursor(&pager, &cursor);
        assert!(window.is_empty());
    }
}
