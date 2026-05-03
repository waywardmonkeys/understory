// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Control templates that expand semantic controls into presentation nodes.
//!
//! Fence: this module owns structural visual expansion; it explicitly does not
//! own widget behavior, semantic state changes, or layout policy.

use alloc::boxed::Box;

use kurbo::Rect;
use understory_style::PartTag;

use crate::{ElementId, PresentationNode, PresentationNodeId, PresentationTree};

/// Open identifier for a presentation part kind.
///
/// Built-in controls provide constants for their parts, but embedders can
/// introduce their own part kinds without changing Overstory core enums.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PartKind(&'static str);

impl PartKind {
    /// Creates a part kind from a stable static name.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// Returns the stable part kind name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// Root presentation part kind.
pub const ROOT_PART: PartKind = PartKind::new("root");

/// Built-in button presentation part kind.
pub const BUTTON_PART: PartKind = PartKind::new("button");

/// Generic border/chrome presentation part kind.
pub const BORDER_PART: PartKind = PartKind::new("border");

/// Generic content presenter presentation part kind.
pub const CONTENT_PRESENTER_PART: PartKind = PartKind::new("content-presenter");

/// Built-in text block presentation part kind.
pub const TEXT_BLOCK_PART: PartKind = PartKind::new("text-block");

/// Built-in text input presentation part kind.
pub const TEXT_INPUT_PART: PartKind = PartKind::new("text-input");

/// Built-in text selection highlight presentation part kind.
pub const TEXT_SELECTION_PART: PartKind = PartKind::new("text-selection");

/// Built-in text caret presentation part kind.
pub const TEXT_CARET_PART: PartKind = PartKind::new("text-caret");

/// Built-in row presentation part kind.
pub const ROW_PART: PartKind = PartKind::new("row");

/// Built-in toggle presentation part kind.
pub const TOGGLE_PART: PartKind = PartKind::new("toggle");

/// Built-in toggle track presentation part kind.
pub const TOGGLE_TRACK_PART: PartKind = PartKind::new("toggle-track");

/// Built-in toggle thumb presentation part kind.
pub const TOGGLE_THUMB_PART: PartKind = PartKind::new("toggle-thumb");

/// Open identifier for a semantic template slot.
///
/// Slots are the contract between widget layout/style policy and a selected
/// template. The template remains free to emit any [`PartKind`] for a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TemplateSlot {
    name: &'static str,
    part_tag: Option<PartTag>,
}

impl TemplateSlot {
    /// Creates a template slot from a stable static name.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            part_tag: None,
        }
    }

    /// Returns the stable slot name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Returns the style part tag for this slot, if it participates in part styling.
    #[must_use]
    pub const fn part_tag(self) -> Option<PartTag> {
        self.part_tag
    }

    /// Returns this slot with an owner-local style part tag.
    #[must_use]
    pub const fn with_part_tag(mut self, part_tag: PartTag) -> Self {
        self.part_tag = Some(part_tag);
        self
    }
}

/// Slot for a control's root/container bounds and values.
pub const ROOT_SLOT: TemplateSlot = TemplateSlot::new("root");

/// Style part tag for a control's content presenter.
pub const CONTENT_SLOT_PART_TAG: PartTag = PartTag(1);

/// Slot for a control's content presenter.
pub const CONTENT_SLOT: TemplateSlot =
    TemplateSlot::new("content").with_part_tag(CONTENT_SLOT_PART_TAG);

/// Style part tag for a toggle's track region.
pub const TOGGLE_TRACK_SLOT_PART_TAG: PartTag = PartTag(2);

/// Slot for a toggle's track region.
pub const TOGGLE_TRACK_SLOT: TemplateSlot =
    TemplateSlot::new("toggle-track").with_part_tag(TOGGLE_TRACK_SLOT_PART_TAG);

/// Style part tag for a toggle's thumb region.
pub const TOGGLE_THUMB_SLOT_PART_TAG: PartTag = PartTag(3);

/// Slot for a toggle's thumb region.
pub const TOGGLE_THUMB_SLOT: TemplateSlot =
    TemplateSlot::new("toggle-thumb").with_part_tag(TOGGLE_THUMB_SLOT_PART_TAG);

/// Open identifier for a template-bindable property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TemplateProperty(&'static str);

impl TemplateProperty {
    /// Creates a template property identifier from a stable static name.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// Returns the stable property name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// Bindable background property.
pub const BACKGROUND_PROPERTY: TemplateProperty = TemplateProperty::new("background");

/// Bindable foreground property.
pub const FOREGROUND_PROPERTY: TemplateProperty = TemplateProperty::new("foreground");

/// Bindable border brush property.
pub const BORDER_PROPERTY: TemplateProperty = TemplateProperty::new("border");

/// Bindable border width property.
pub const BORDER_WIDTH_PROPERTY: TemplateProperty = TemplateProperty::new("border-width");

/// Bindable padding property.
pub const PADDING_PROPERTY: TemplateProperty = TemplateProperty::new("padding");

/// Bindable corner radius property.
pub const CORNER_RADIUS_PROPERTY: TemplateProperty = TemplateProperty::new("corner-radius");

/// Bindable content property.
pub const CONTENT_PROPERTY: TemplateProperty = TemplateProperty::new("content");

/// A binding from the templated control to a template part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateBinding {
    /// Property assigned on the template part.
    pub target: TemplateProperty,
    /// Property read from the templated control.
    pub source: TemplateProperty,
}

impl TemplateBinding {
    /// Creates a binding that reads `source` from the templated control and writes `target` to a part.
    #[must_use]
    pub const fn new(target: TemplateProperty, source: TemplateProperty) -> Self {
        Self { target, source }
    }

    /// Passes one property from the templated control through to a template part unchanged.
    #[must_use]
    pub const fn pass(property: TemplateProperty) -> Self {
        Self {
            target: property,
            source: property,
        }
    }
}

/// A node in a control template.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateNode {
    /// Presentation part kind created by this template node.
    pub kind: PartKind,
    /// Semantic slot this node reads bounds and bound values from.
    pub slot: TemplateSlot,
    /// Amount by which this node's bounds are inset from the semantic control bounds.
    pub inset: f64,
    /// Property bindings applied to this template node.
    pub bindings: Box<[TemplateBinding]>,
    /// Child template nodes.
    pub children: Box<[Self]>,
}

impl TemplateNode {
    /// Creates a template node.
    #[must_use]
    pub fn new(
        kind: PartKind,
        bindings: impl Into<Box<[TemplateBinding]>>,
        children: impl Into<Box<[Self]>>,
    ) -> Self {
        Self {
            kind,
            slot: default_slot_for_part(kind),
            inset: 0.0,
            bindings: bindings.into(),
            children: children.into(),
        }
    }

    /// Returns this template node assigned to `slot`.
    #[must_use]
    pub const fn with_slot(mut self, slot: TemplateSlot) -> Self {
        self.slot = slot;
        self
    }

    /// Returns this template node with bounds inset by `inset` logical units.
    #[must_use]
    pub fn with_inset(mut self, inset: f64) -> Self {
        self.inset = inset.max(0.0);
        self
    }
}

/// A control template.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlTemplate {
    root: TemplateNode,
}

impl ControlTemplate {
    /// Creates a control template with the given root node.
    #[must_use]
    pub fn new(root: TemplateNode) -> Self {
        Self { root }
    }

    /// Returns the template root node.
    #[must_use]
    pub const fn root(&self) -> &TemplateNode {
        &self.root
    }
}

/// Returns the built-in button control template.
#[must_use]
pub fn button_template() -> ControlTemplate {
    ControlTemplate::new(TemplateNode::new(
        BUTTON_PART,
        [],
        [TemplateNode::new(
            BORDER_PART,
            [
                TemplateBinding::pass(BACKGROUND_PROPERTY),
                TemplateBinding::pass(BORDER_PROPERTY),
                TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
                TemplateBinding::pass(PADDING_PROPERTY),
                TemplateBinding::pass(CORNER_RADIUS_PROPERTY),
            ],
            [TemplateNode::new(
                CONTENT_PRESENTER_PART,
                [
                    TemplateBinding::pass(CONTENT_PROPERTY),
                    TemplateBinding::pass(FOREGROUND_PROPERTY),
                ],
                [],
            )],
        )],
    ))
}

/// Returns the built-in text block control template.
#[must_use]
pub fn text_block_template() -> ControlTemplate {
    ControlTemplate::new(TemplateNode::new(
        TEXT_BLOCK_PART,
        [
            TemplateBinding::pass(BACKGROUND_PROPERTY),
            TemplateBinding::pass(BORDER_PROPERTY),
            TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
            TemplateBinding::pass(PADDING_PROPERTY),
            TemplateBinding::pass(CORNER_RADIUS_PROPERTY),
        ],
        [TemplateNode::new(
            CONTENT_PRESENTER_PART,
            [
                TemplateBinding::pass(CONTENT_PROPERTY),
                TemplateBinding::pass(FOREGROUND_PROPERTY),
            ],
            [],
        )],
    ))
}

/// Returns the built-in text input control template.
#[must_use]
pub fn text_input_template() -> ControlTemplate {
    ControlTemplate::new(TemplateNode::new(
        TEXT_INPUT_PART,
        [
            TemplateBinding::pass(BACKGROUND_PROPERTY),
            TemplateBinding::pass(BORDER_PROPERTY),
            TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
            TemplateBinding::pass(PADDING_PROPERTY),
            TemplateBinding::pass(CORNER_RADIUS_PROPERTY),
        ],
        [TemplateNode::new(
            CONTENT_PRESENTER_PART,
            [
                TemplateBinding::pass(CONTENT_PROPERTY),
                TemplateBinding::pass(FOREGROUND_PROPERTY),
            ],
            [],
        )],
    ))
}

/// Returns the built-in toggle control template.
#[must_use]
pub fn toggle_template() -> ControlTemplate {
    ControlTemplate::new(TemplateNode::new(
        TOGGLE_PART,
        [],
        [
            TemplateNode::new(
                TOGGLE_TRACK_PART,
                [
                    TemplateBinding::pass(BACKGROUND_PROPERTY),
                    TemplateBinding::pass(BORDER_PROPERTY),
                    TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
                    TemplateBinding::pass(CORNER_RADIUS_PROPERTY),
                ],
                [TemplateNode::new(
                    TOGGLE_THUMB_PART,
                    [
                        TemplateBinding::pass(BACKGROUND_PROPERTY),
                        TemplateBinding::pass(CORNER_RADIUS_PROPERTY),
                    ],
                    [],
                )
                .with_slot(TOGGLE_THUMB_SLOT)],
            )
            .with_slot(TOGGLE_TRACK_SLOT),
            TemplateNode::new(
                CONTENT_PRESENTER_PART,
                [
                    TemplateBinding::pass(CONTENT_PROPERTY),
                    TemplateBinding::pass(FOREGROUND_PROPERTY),
                ],
                [],
            ),
        ],
    ))
}

/// Arranged bounds for one semantic template slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemplateSlotLayout {
    /// Template slot these bounds apply to.
    pub slot: TemplateSlot,
    /// Bounds supplied to that slot.
    pub bounds: Rect,
}

impl TemplateSlotLayout {
    /// Creates arranged bounds for one template slot.
    #[must_use]
    pub const fn new(slot: TemplateSlot, bounds: Rect) -> Self {
        Self { slot, bounds }
    }
}

/// Arranged bounds supplied to a control template.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateLayout {
    /// Bounds of the semantic control and chrome.
    pub container: Rect,
    /// Bounds of the content presenter.
    pub content: Rect,
    /// Additional named slot bounds supplied by the widget layout policy.
    pub slots: Box<[TemplateSlotLayout]>,
}

impl TemplateLayout {
    /// Creates template layout with container and content presenter bounds.
    #[must_use]
    pub fn new(container: Rect, content: Rect) -> Self {
        Self {
            container,
            content,
            slots: Box::from([]),
        }
    }

    /// Returns this layout with additional named slot bounds.
    #[must_use]
    pub fn with_slots(mut self, slots: impl Into<Box<[TemplateSlotLayout]>>) -> Self {
        self.slots = slots.into();
        self
    }

    fn bounds_for(&self, slot: TemplateSlot) -> Rect {
        self.slots
            .iter()
            .find(|part| part.slot == slot)
            .map_or_else(
                || {
                    if slot == CONTENT_SLOT {
                        self.content
                    } else {
                        self.container
                    }
                },
                |part| part.bounds,
            )
    }
}

/// Source of values applied while instantiating a control template.
pub(crate) trait TemplateValueSource {
    /// Enters a template slot before applying bindings for that node.
    fn enter_slot(&mut self, _slot: TemplateSlot) {}

    /// Leaves a template slot after all child template nodes have been emitted.
    fn exit_slot(&mut self, _slot: TemplateSlot) {}

    /// Applies one template binding to a presentation node.
    fn apply(&mut self, node: &mut PresentationNode, slot: TemplateSlot, binding: TemplateBinding);
}

/// Instantiates a selected control template into a presentation tree.
pub(crate) fn instantiate_template(
    tree: &mut PresentationTree,
    parent: PresentationNodeId,
    source: ElementId,
    template: &ControlTemplate,
    layout: TemplateLayout,
    data: &mut impl TemplateValueSource,
) -> PresentationNodeId {
    let root = template.root();
    instantiate_template_node(tree, parent, source, root, &layout, data)
}

fn instantiate_template_node(
    tree: &mut PresentationTree,
    parent: PresentationNodeId,
    source: ElementId,
    template: &TemplateNode,
    layout: &TemplateLayout,
    data: &mut dyn TemplateValueSource,
) -> PresentationNodeId {
    data.enter_slot(template.slot);
    let bounds = layout.bounds_for(template.slot).inset(-template.inset);
    let mut node = PresentationNode::new(source, template.kind, bounds);
    for binding in template.bindings.iter().copied() {
        data.apply(&mut node, template.slot, binding);
    }

    let id = tree.push_child(parent, node);
    for child in template.children.iter() {
        instantiate_template_node(tree, id, source, child, layout, data);
    }
    data.exit_slot(template.slot);
    id
}

fn default_slot_for_part(kind: PartKind) -> TemplateSlot {
    if kind == CONTENT_PRESENTER_PART {
        CONTENT_SLOT
    } else {
        ROOT_SLOT
    }
}
