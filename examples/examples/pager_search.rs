// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Search pager basics.
//!
//! Drive previous/next controls plus numbered page buttons over a dense
//! search-results pager whose keys are explicit page indices.
//!
//! Run:
//! - `cargo run -p understory_examples --example pager_search`

use understory_pager::{
    HasLastKey, KeyPosition, KnownLength, Pager, PagerCursor, move_first, move_next,
    resolve_current,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PageIndex(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SearchPage<'a> {
    label: &'a str,
    hits: &'a [&'a str],
}

struct SearchResultsPager<'a> {
    pages: &'a [SearchPage<'a>],
}

impl<'a> Pager for SearchResultsPager<'a> {
    type Key = PageIndex;
    type Item = &'a SearchPage<'a>;

    fn first_key(&self) -> Option<Self::Key> {
        (!self.pages.is_empty()).then_some(PageIndex(0))
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        key.0 < self.pages.len()
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        let next = key.0 + 1;
        (next < self.pages.len()).then_some(PageIndex(next))
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        key.0.checked_sub(1).map(PageIndex)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.pages.get(key.0)
    }
}

impl HasLastKey for SearchResultsPager<'_> {
    fn last_key(&self) -> Option<Self::Key> {
        self.pages.len().checked_sub(1).map(PageIndex)
    }
}

impl KnownLength for SearchResultsPager<'_> {
    fn len(&self) -> usize {
        self.pages.len()
    }
}

impl KeyPosition for SearchResultsPager<'_> {
    fn key_at_index(&self, index: usize) -> Option<Self::Key> {
        (index < self.pages.len()).then_some(PageIndex(index))
    }

    fn index_of_key(&self, key: &Self::Key) -> Option<usize> {
        self.contains_key(key).then_some(key.0)
    }
}

fn main() {
    let pages = [
        SearchPage {
            label: "Page 1",
            hits: &["alpha.rs", "beta.rs"],
        },
        SearchPage {
            label: "Page 2",
            hits: &["gamma.rs", "delta.rs"],
        },
        SearchPage {
            label: "Page 3",
            hits: &["epsilon.rs"],
        },
    ];
    let pager = SearchResultsPager { pages: &pages };
    let mut cursor = PagerCursor::new();

    let buttons: Vec<_> = (0..pager.len())
        .filter_map(|index| pager.key_at_index(index))
        .collect();
    println!("Buttons: {buttons:?}");
    println!("Last page key: {:?}", pager.last_key());

    let _ = move_first(&pager, &mut cursor);
    print_current(&pager, &cursor);

    let _ = move_next(&pager, &mut cursor);
    print_current(&pager, &cursor);

    cursor.set_current(Some(PageIndex(2)));
    print_current(&pager, &cursor);
}

fn print_current(pager: &SearchResultsPager<'_>, cursor: &PagerCursor<PageIndex>) {
    let current = resolve_current(pager, cursor).expect("cursor should point at a valid page");
    let index = pager
        .index_of_key(cursor.current().expect("cursor should have a page"))
        .expect("current page should have a dense position");
    println!(
        "Current: {} (button #{index}) -> {:?}",
        current.label, current.hits
    );
}
