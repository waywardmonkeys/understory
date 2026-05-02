// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Overstory style authoring and inspection helpers.
//!
//! **Fence:** this module owns Overstory's semantic selector vocabulary and
//! inspector-facing style diagnostics; it explicitly does not own selector
//! matching or cascade ordering, which remain in `understory_style`.

use alloc::{boxed::Box, vec::Vec};

use understory_property::PropertyId;
use understory_style::{
    MatchRule, PartTag, PseudoClassId, Selector, SelectorStep, StyleOrigin, TypeTag,
};

use crate::{
    BUTTON_TYPE, CHECKED, CONTENT_SLOT, ElementKind, HOVERED, PRESSED, TOGGLE_THUMB_SLOT,
    TOGGLE_TRACK_SLOT, TOGGLE_TYPE, TemplateSlot,
};

/// Returns a selector step for an element kind.
#[must_use]
pub fn kind(kind: ElementKind) -> SelectorStep {
    SelectorStep::type_tag(kind.type_tag())
}

/// Returns a selector step for an element kind with one pseudoclass.
#[must_use]
pub fn kind_when(element_kind: ElementKind, pseudo: PseudoClassId) -> SelectorStep {
    kind(element_kind).with_pseudo(pseudo)
}

/// Returns a selector step for an application-defined type tag.
#[must_use]
pub fn type_tag(type_tag: TypeTag) -> SelectorStep {
    SelectorStep::type_tag(type_tag)
}

/// Returns a selector step for an application-defined type tag with one pseudoclass.
#[must_use]
pub fn type_tag_when(owner: TypeTag, pseudo: PseudoClassId) -> SelectorStep {
    type_tag(owner).with_pseudo(pseudo)
}

/// Returns a selector step for an owner-local template part tag.
#[must_use]
pub fn part(part_tag: PartTag) -> SelectorStep {
    SelectorStep::part_tag(part_tag)
}

/// Returns a selector step for a styleable template slot.
#[must_use]
pub fn slot(slot: TemplateSlot) -> Option<SelectorStep> {
    slot.part_tag().map(part)
}

/// Returns a child-path selector from an owner type to a part tag.
#[must_use]
pub fn child_part(owner: TypeTag, part_tag: PartTag) -> Selector {
    Selector::child(type_tag(owner), part(part_tag))
}

/// Returns a descendant selector from an owner type to a part tag.
#[must_use]
pub fn descendant_part(owner: TypeTag, part_tag: PartTag) -> Selector {
    Selector::descendant(type_tag(owner), part(part_tag))
}

/// Returns a child-path selector from an owner kind to a styleable template slot.
#[must_use]
pub fn child_slot(owner: ElementKind, slot: TemplateSlot) -> Option<Selector> {
    slot.part_tag()
        .map(|part_tag| child_part(owner.type_tag(), part_tag))
}

/// Returns a descendant selector from an owner kind to a styleable template slot.
#[must_use]
pub fn descendant_slot(owner: ElementKind, slot: TemplateSlot) -> Option<Selector> {
    slot.part_tag()
        .map(|part_tag| descendant_part(owner.type_tag(), part_tag))
}

/// Returns a button selector step.
#[must_use]
pub fn button() -> SelectorStep {
    type_tag(BUTTON_TYPE)
}

/// Returns a button selector step with one pseudoclass.
#[must_use]
pub fn button_when(pseudo: PseudoClassId) -> SelectorStep {
    type_tag_when(BUTTON_TYPE, pseudo)
}

/// Returns a button selector step for the hovered pseudoclass.
#[must_use]
pub fn button_hovered() -> SelectorStep {
    button_when(HOVERED)
}

/// Returns a button selector step for the pressed pseudoclass.
#[must_use]
pub fn button_pressed() -> SelectorStep {
    button_when(PRESSED)
}

/// Returns a selector for a button's content slot.
#[must_use]
pub fn button_content() -> Selector {
    Selector::child(
        button(),
        slot(CONTENT_SLOT).expect("content slot should be styleable"),
    )
}

/// Returns a selector for a button's content slot when the button has one pseudoclass.
#[must_use]
pub fn button_content_when(pseudo: PseudoClassId) -> Selector {
    Selector::child(
        button_when(pseudo),
        slot(CONTENT_SLOT).expect("content slot should be styleable"),
    )
}

/// Returns a toggle selector step.
#[must_use]
pub fn toggle() -> SelectorStep {
    type_tag(TOGGLE_TYPE)
}

/// Returns a toggle selector step with one pseudoclass.
#[must_use]
pub fn toggle_when(pseudo: PseudoClassId) -> SelectorStep {
    type_tag_when(TOGGLE_TYPE, pseudo)
}

/// Returns a toggle selector step for the hovered pseudoclass.
#[must_use]
pub fn toggle_hovered() -> SelectorStep {
    toggle_when(HOVERED)
}

/// Returns a toggle selector step for the pressed pseudoclass.
#[must_use]
pub fn toggle_pressed() -> SelectorStep {
    toggle_when(PRESSED)
}

/// Returns a toggle selector step for the checked pseudoclass.
#[must_use]
pub fn toggle_checked() -> SelectorStep {
    toggle_when(CHECKED)
}

/// Returns a selector for a toggle's content slot.
#[must_use]
pub fn toggle_content() -> Selector {
    Selector::child(
        toggle(),
        slot(CONTENT_SLOT).expect("content slot should be styleable"),
    )
}

/// Returns a selector for a toggle's track slot.
#[must_use]
pub fn toggle_track() -> Selector {
    Selector::child(
        toggle(),
        slot(TOGGLE_TRACK_SLOT).expect("toggle track slot should be styleable"),
    )
}

/// Returns a selector for a toggle's track slot when the toggle has one pseudoclass.
#[must_use]
pub fn toggle_track_when(pseudo: PseudoClassId) -> Selector {
    Selector::child(
        toggle_when(pseudo),
        slot(TOGGLE_TRACK_SLOT).expect("toggle track slot should be styleable"),
    )
}

/// Returns a descendant selector for a toggle's thumb slot.
///
/// The descendant shape lets structurally different templates place the thumb
/// directly under the toggle or under another part such as the track.
#[must_use]
pub fn toggle_thumb() -> Selector {
    Selector::descendant(
        toggle(),
        slot(TOGGLE_THUMB_SLOT).expect("toggle thumb slot should be styleable"),
    )
}

/// Returns the built-in nested selector for a toggle thumb inside the track slot.
#[must_use]
pub fn toggle_thumb_in_track() -> Selector {
    Selector::path([
        toggle(),
        slot(TOGGLE_TRACK_SLOT).expect("toggle track slot should be styleable"),
        slot(TOGGLE_THUMB_SLOT).expect("toggle thumb slot should be styleable"),
    ])
}

/// Returns a descendant selector for a toggle's thumb slot when the toggle has one pseudoclass.
#[must_use]
pub fn toggle_thumb_when(pseudo: PseudoClassId) -> Selector {
    Selector::descendant(
        toggle_when(pseudo),
        slot(TOGGLE_THUMB_SLOT).expect("toggle thumb slot should be styleable"),
    )
}

/// Style subject to inspect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum StyleSubject {
    /// The semantic owner element itself.
    #[default]
    Owner,
    /// A concrete style-subject path under the owner, expressed as template part tags.
    PartPath(Box<[PartTag]>),
}

impl StyleSubject {
    /// Returns the owner subject.
    #[must_use]
    pub const fn owner() -> Self {
        Self::Owner
    }

    /// Returns a one-part subject.
    #[must_use]
    pub fn part(part_tag: PartTag) -> Self {
        Self::PartPath(Box::new([part_tag]))
    }

    /// Returns a one-part subject for a styleable template slot.
    #[must_use]
    pub fn slot(slot: TemplateSlot) -> Option<Self> {
        slot.part_tag().map(Self::part)
    }

    /// Returns a subject for a nested template part path.
    #[must_use]
    pub fn path(path: impl IntoIterator<Item = PartTag>) -> Self {
        Self::PartPath(path.into_iter().collect::<Vec<_>>().into_boxed_slice())
    }

    /// Returns a subject for the common content slot.
    #[must_use]
    pub fn content() -> Self {
        Self::slot(CONTENT_SLOT).expect("content slot should be styleable")
    }

    /// Returns a subject for a button's content slot.
    #[must_use]
    pub fn button_content() -> Self {
        Self::content()
    }

    /// Returns a subject for a toggle's content slot.
    #[must_use]
    pub fn toggle_content() -> Self {
        Self::content()
    }

    /// Returns a subject for a toggle's track slot.
    #[must_use]
    pub fn toggle_track() -> Self {
        Self::slot(TOGGLE_TRACK_SLOT).expect("toggle track slot should be styleable")
    }

    /// Returns a subject for the built-in nested toggle thumb slot path.
    #[must_use]
    pub fn toggle_thumb_in_track() -> Self {
        Self::path([
            TOGGLE_TRACK_SLOT
                .part_tag()
                .expect("toggle track slot should be styleable"),
            TOGGLE_THUMB_SLOT
                .part_tag()
                .expect("toggle thumb slot should be styleable"),
        ])
    }
}

/// Inspector-facing style data for one subject/property query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleInspection {
    /// Subject that was inspected.
    pub subject: StyleSubject,
    /// Property that was inspected.
    pub property: PropertyId,
    /// Registered property name, if available.
    pub property_name: Option<&'static str>,
    /// Selector rules matching the subject.
    pub matching_rules: Box<[StyleRuleInspection]>,
    /// Winning Style-layer source for the property, if any.
    pub winning_source: Option<StyleSourceInspection>,
}

/// Inspector-facing summary of a matching selector rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleRuleInspection {
    /// The selector that matched.
    pub selector: Selector,
    /// Cascade origin of the rule.
    pub origin: StyleOrigin,
    /// Source index used for cascade ordering.
    pub source_index: usize,
    /// Rule insertion order within its source group.
    pub order: u32,
}

impl StyleRuleInspection {
    pub(crate) fn from_rule(rule: &MatchRule) -> Self {
        Self {
            selector: rule.selector().clone(),
            origin: rule.origin(),
            source_index: rule.source_index(),
            order: rule.order(),
        }
    }
}

/// Inspector-facing summary of the source that wins a property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleSourceInspection {
    /// A direct style source wins.
    Direct {
        /// Cascade origin of the direct style.
        origin: StyleOrigin,
        /// Source index used for cascade ordering.
        source_index: usize,
    },
    /// A selector rule wins.
    Rule(StyleRuleInspection),
}
