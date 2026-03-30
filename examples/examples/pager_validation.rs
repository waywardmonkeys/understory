// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Derived pager basics.
//!
//! Navigate a derived sequence of visible validation issues using stable domain
//! IDs rather than filtered positions.
//!
//! Run:
//! - `cargo run -p understory_examples --example pager_validation`

use understory_pager::{Pager, PagerCursor, PagerWindow, move_next, resolve_current};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IssueId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Issue {
    id: IssueId,
    label: &'static str,
    severity: Severity,
    visible: bool,
}

struct VisibleErrorsPager<'a> {
    issues: &'a [Issue],
}

impl VisibleErrorsPager<'_> {
    fn visible_error_ids(&self) -> impl Iterator<Item = IssueId> + '_ {
        self.issues
            .iter()
            .filter(|issue| issue.visible && issue.severity == Severity::Error)
            .map(|issue| issue.id)
    }

    fn position_of(&self, key: &IssueId) -> Option<usize> {
        self.visible_error_ids()
            .position(|candidate| candidate == *key)
    }
}

impl<'a> Pager for VisibleErrorsPager<'a> {
    type Key = IssueId;
    type Item = &'a Issue;

    fn first_key(&self) -> Option<Self::Key> {
        self.visible_error_ids().next()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        self.position_of(key).is_some()
    }

    fn next_key(&self, key: &Self::Key) -> Option<Self::Key> {
        let current = self.position_of(key)?;
        self.visible_error_ids().nth(current + 1)
    }

    fn prev_key(&self, key: &Self::Key) -> Option<Self::Key> {
        let current = self.position_of(key)?;
        current
            .checked_sub(1)
            .and_then(|previous| self.visible_error_ids().nth(previous))
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.issues.iter().find(|issue| issue.id == *key)
    }
}

fn main() {
    let issues = [
        Issue {
            id: IssueId(7),
            label: "Hidden helper row",
            severity: Severity::Warning,
            visible: false,
        },
        Issue {
            id: IssueId(42),
            label: "Missing material",
            severity: Severity::Error,
            visible: true,
        },
        Issue {
            id: IssueId(88),
            label: "Collapsed subtree",
            severity: Severity::Warning,
            visible: true,
        },
        Issue {
            id: IssueId(99),
            label: "Broken constraint",
            severity: Severity::Error,
            visible: true,
        },
    ];
    let pager = VisibleErrorsPager { issues: &issues };
    let mut cursor = PagerCursor::new();

    let _ = move_next(&pager, &mut cursor);
    print_current(&pager, &cursor);
    println!("Window: {:?}", PagerWindow::from_cursor(&pager, &cursor));

    let _ = move_next(&pager, &mut cursor);
    print_current(&pager, &cursor);
    println!("Window: {:?}", PagerWindow::from_cursor(&pager, &cursor));
}

fn print_current(pager: &VisibleErrorsPager<'_>, cursor: &PagerCursor<IssueId>) {
    let issue = resolve_current(pager, cursor).expect("cursor should point at a visible error");
    println!("Current issue: {:?} -> {}", issue.id, issue.label);
}
