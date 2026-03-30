// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_pager --heading-base-level=0

//! Understory Pager: keyed discrete navigation primitives.
//!
//! This crate provides a small, renderer-agnostic core for navigating
//! revisitable sequences whose items have stable identities and neighbor
//! relationships.
//!
//! The crate is deliberately narrower than a “pagination widget”:
//!
//! - [`Pager`] models navigation over stable keys.
//! - [`PagerCursor`] keeps current-page state outside the pager definition.
//! - [`SlicePager`] provides a simple dense baseline implementation.
//! - [`PagerWindow`] derives the immediate previous/current/next neighborhood
//!   around a cursor.
//! - Cheap adapters such as [`MapPager`], [`TryMapPager`], [`ChainPager`], and
//!   [`ReversePager`] preserve the keyed navigation model without imposing UI
//!   policy.
//!
//! This crate deliberately does **not** know about:
//!
//! - widgets, tabs, breadcrumbs, or button policies,
//! - selection semantics or anchor/range bookkeeping,
//! - async loading or hydration policy,
//! - hidden caches with surprising invalidation rules.
//!
//! The intended layering is:
//!
//! - this crate defines keyed navigation and a small algebra of composition,
//! - higher layers own selection, presentation, and command semantics,
//! - domain crates decide whether keys are indices, IDs, handles, or graph nodes.
//!
//! ## Overview
//!
//! Goal:
//! provide a calm primitive for “discrete navigation over stable items”.
//!
//! Non-goals:
//! own UI widgets, selection/range policy, async fetching, or invisible caches.
//!
//! ## Glossary
//!
//! - **Key**: the stable identity used to revisit an item.
//! - **Cursor**: external state that tracks which key is currently active.
//! - **Dense pager**: a pager whose keys correspond to contiguous positions.
//! - **Derived pager**: a pager whose navigable keys are computed from other state.
//! - **Window**: a small neighborhood around the current key.
//!
//! ## Fence
//!
//! This crate owns discrete navigation over stable keys; it explicitly does not
//! own selection semantics, widget logic, async loading, or history policy.
//!
//! ## Invariants
//!
//! Callers and implementations should rely on the following:
//!
//! - Keys are meaningful outside the pager and may be stored in external state.
//! - [`Pager::next_key`] and [`Pager::prev_key`] describe adjacency, not
//!   indexing.
//! - [`Pager::contains_key`] and [`Pager::item`] may legitimately return `false`
//!   and `None` for stale keys when the underlying source changes.
//! - Length and positional lookup are opt-in capabilities, not baseline
//!   guarantees.
//! - Any adapter whose behavior may scan or otherwise introduce non-obvious cost
//!   should document that cost explicitly.
//!
//! ## Why not just hand-roll this?
//!
//! If you only need `current_page: usize` over a local `Vec`, you probably
//! should hand-roll it.
//!
//! This crate is for the point where navigation semantics start repeating across
//! domains:
//!
//! - search result pages,
//! - wizard steps,
//! - “next invalid item” inspectors,
//! - walkthroughs over scene or graph nodes,
//! - grouped or filtered sequences.
//!
//! The advantage is not that the logic is individually hard. The advantage is
//! that one calm contract handles the repeated edges:
//!
//! - where current state lives,
//! - how stale keys are revalidated,
//! - whether page-number buttons are even meaningful,
//! - how reverse traversal and concatenation behave,
//! - how tests describe navigation without coupling to a specific UI.
//!
//! ## When this crate is overkill
//!
//! Reach for a hand-rolled local model when all of the following are true:
//!
//! - the sequence is dense and short-lived,
//! - raw `usize` positions are the real identity,
//! - current-page state is used in one place only,
//! - there is no expectation of reuse across other flows.
//!
//! ## Minimal example
//!
//! ```rust
//! use understory_pager::{Pager, PagerCursor, PagerExt, SlicePager, move_next, resolve_current};
//!
//! let pager = SlicePager::new(&["intro", "details", "summary"]);
//! let mut cursor = PagerCursor::new();
//!
//! assert!(move_next(&pager, &mut cursor));
//! assert_eq!(cursor.current(), Some(&0));
//! assert_eq!(resolve_current(&pager, &cursor), Some(&"intro"));
//!
//! let mapped = pager.map(|name| name.len());
//! assert_eq!(mapped.item(&1), Some(7));
//! ```
//!
//! ## Dense page-number pager
//!
//! Search results with numbered buttons usually want more than plain
//! [`Pager`]: the buttons need a stable position mapping, so the pager should
//! also implement [`KnownLength`] and [`KeyPosition`].
//!
//! ```rust
//! use understory_pager::{
//!     HasLastKey, KeyPosition, KnownLength, Pager, PagerCursor, move_first, move_next,
//!     resolve_current,
//! };
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct PageIndex(usize);
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct SearchPage<'a> {
//!     label: &'a str,
//! }
//!
//! struct SearchResultsPager<'a> {
//!     pages: &'a [SearchPage<'a>],
//! }
//!
//! impl<'a> Pager for SearchResultsPager<'a> {
//!     type Key = PageIndex;
//!     type Item = &'a SearchPage<'a>;
//!
//!     fn first_key(&self) -> Option<Self::Key> {
//!         (!self.pages.is_empty()).then_some(PageIndex(0))
//!     }
//!
//!     fn contains_key(&self, key: &Self::Key) -> bool {
//!         key.0 < self.pages.len()
//!     }
//!
//!     fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
//!         let next = key.0 + 1;
//!         (next < self.pages.len()).then_some(PageIndex(next))
//!     }
//!
//!     fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
//!         key.0.checked_sub(1).map(PageIndex)
//!     }
//!
//!     fn item(&self, key: &Self::Key) -> Option<Self::Item> {
//!         self.pages.get(key.0)
//!     }
//! }
//!
//! impl HasLastKey for SearchResultsPager<'_> {
//!     fn last_key(&self) -> Option<Self::Key> {
//!         self.pages.len().checked_sub(1).map(PageIndex)
//!     }
//! }
//!
//! impl KnownLength for SearchResultsPager<'_> {
//!     fn len(&self) -> usize {
//!         self.pages.len()
//!     }
//! }
//!
//! impl KeyPosition for SearchResultsPager<'_> {
//!     fn key_at_index(&self, index: usize) -> Option<Self::Key> {
//!         (index < self.pages.len()).then_some(PageIndex(index))
//!     }
//!
//!     fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
//!         self.contains_key(key).then_some(key.0)
//!     }
//! }
//!
//! let pages = [
//!     SearchPage { label: "Page 1" },
//!     SearchPage { label: "Page 2" },
//!     SearchPage { label: "Page 3" },
//! ];
//! let pager = SearchResultsPager { pages: &pages };
//! let mut cursor = PagerCursor::new();
//!
//! assert!(move_first(&pager, &mut cursor));
//! assert_eq!(resolve_current(&pager, &cursor).map(|page| page.label), Some("Page 1"));
//!
//! let buttons: Vec<_> = (0..pager.len())
//!     .filter_map(|index| pager.key_at_index(index))
//!     .collect();
//! assert_eq!(buttons, vec![PageIndex(0), PageIndex(1), PageIndex(2)]);
//!
//! assert!(move_next(&pager, &mut cursor));
//! assert_eq!(resolve_current(&pager, &cursor).map(|page| page.label), Some("Page 2"));
//! ```
//!
//! ## Derived keyed pager
//!
//! Not every useful pager is a flat `0..len` list. Sometimes the navigable
//! sequence is derived from a larger model and the key should be the domain ID,
//! not the filtered position.
//!
//! ```rust
//! use understory_pager::{Pager, PagerCursor, move_next, resolve_current};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct IssueId(u32);
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct Issue {
//!     id: IssueId,
//!     title: &'static str,
//! }
//!
//! struct VisibleIssuePager<'a> {
//!     ordered_ids: &'a [IssueId],
//!     issues: &'a [Issue],
//! }
//!
//! impl<'a> VisibleIssuePager<'a> {
//!     fn position_of(&self, key: &IssueId) -> Option<usize> {
//!         self.ordered_ids.iter().position(|candidate| candidate == key)
//!     }
//! }
//!
//! impl<'a> Pager for VisibleIssuePager<'a> {
//!     type Key = IssueId;
//!     type Item = &'a Issue;
//!
//!     fn first_key(&self) -> Option<Self::Key> {
//!         self.ordered_ids.first().copied()
//!     }
//!
//!     fn contains_key(&self, key: &Self::Key) -> bool {
//!         self.position_of(key).is_some()
//!     }
//!
//!     fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
//!         let index = self.position_of(key)?;
//!         self.ordered_ids.get(index + 1).copied()
//!     }
//!
//!     fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
//!         let index = self.position_of(key)?;
//!         index.checked_sub(1).map(|previous| self.ordered_ids[previous])
//!     }
//!
//!     fn item(&self, key: &Self::Key) -> Option<Self::Item> {
//!         self.issues.iter().find(|issue| issue.id == *key)
//!     }
//! }
//!
//! let issues = [
//!     Issue {
//!         id: IssueId(7),
//!         title: "Hidden helper row",
//!     },
//!     Issue {
//!         id: IssueId(42),
//!         title: "Missing material",
//!     },
//!     Issue {
//!         id: IssueId(99),
//!         title: "Broken constraint",
//!     },
//! ];
//! let pager = VisibleIssuePager {
//!     ordered_ids: &[IssueId(42), IssueId(99)],
//!     issues: &issues,
//! };
//! let mut cursor = PagerCursor::new();
//!
//! assert!(move_next(&pager, &mut cursor));
//! assert_eq!(
//!     resolve_current(&pager, &cursor).map(|issue| issue.title),
//!     Some("Missing material")
//! );
//! assert_eq!(pager.next_key(&IssueId(42)), Some(IssueId(99)));
//! ```
//!
//! ## Design notes
//!
//! The current API is an intentionally calm first slice. In particular:
//!
//! - Filtering and flattening adapters are not in v0 yet because real call sites
//!   should establish the right invalidation and complexity story first.
//! - The window model currently only covers immediate neighbors; larger
//!   prefetch neighborhoods should be added only after host code demonstrates a
//!   stable need.
//! - History remains outside the crate for now; a cursor is enough to let
//!   selection, workflows, or UI state own policy.
//!
//! ## Extension points
//!
//! The most likely next additions are:
//!
//! - a dense page-oriented key newtype if page-number pagers show up often,
//! - filtering and flattening once real call sites establish their cost model,
//! - richer neighborhood helpers if prefetch/warming behavior becomes common.
//!
//! ## Gotchas
//!
//! - Page-number buttons only make sense when the pager exposes position data.
//! - `contains_key` exists because external cursor state may outlive a
//!   particular snapshot of domain data.
//! - Derived pagers often scan unless the host domain provides a faster index;
//!   that cost should stay visible in the implementation and docs.
//!
//! This crate is `no_std` and uses `alloc`.

#![no_std]

extern crate alloc;

mod adapters;
mod indexed;
mod pager;
mod state;
mod window;

pub use adapters::{ChainKey, ChainPager, MapPager, ReversePager, TryMapPager};
pub use indexed::SlicePager;
pub use pager::{HasLastKey, KeyPosition, KnownLength, Pager, PagerExt};
pub use state::{PagerCursor, move_first, move_last, move_next, move_prev, resolve_current};
pub use window::PagerWindow;
