// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stable identifiers for Overstory runtime objects.

use invalidation::DenseKey;

/// Stable identifier for a semantic element in a [`Ui`](crate::Ui).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementId(u32);

impl ElementId {
    /// Creates an element identifier from a dense raw index.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the dense raw index for this identifier.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl DenseKey for ElementId {
    #[inline]
    fn index(self) -> usize {
        self.index()
    }
}
