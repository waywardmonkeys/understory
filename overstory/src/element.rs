// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Semantic elements and selector state.

use alloc::{boxed::Box, vec::Vec};

use understory_property::{DependencyObject, PropertyStore};
use understory_style::{ClassId, MatchState, PartTag, PseudoClassId, StyleCascade, TypeTag};

use crate::{ElementId, widget::Widget};

/// Selector type tag for the root element.
pub const ROOT_TYPE: TypeTag = TypeTag(0);

/// Selector type tag for button controls.
pub const BUTTON_TYPE: TypeTag = TypeTag(1);

/// Selector type tag for text block controls.
pub const TEXT_BLOCK_TYPE: TypeTag = TypeTag(2);

/// Selector type tag for panel controls.
pub const PANEL_TYPE: TypeTag = TypeTag(3);

/// Selector type tag for row controls.
pub const ROW_TYPE: TypeTag = TypeTag(4);

/// Selector type tag for toggle controls.
pub const TOGGLE_TYPE: TypeTag = TypeTag(5);

/// Pseudoclass set when a control is hovered.
pub const HOVERED: PseudoClassId = PseudoClassId(1);

/// Pseudoclass set when a control is pressed.
pub const PRESSED: PseudoClassId = PseudoClassId(2);

/// Pseudoclass set when a control is disabled.
pub const DISABLED: PseudoClassId = PseudoClassId(3);

/// Pseudoclass set when a toggle-like control is checked.
pub const CHECKED: PseudoClassId = PseudoClassId(4);

/// Open semantic kind of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ElementKind {
    type_tag: TypeTag,
    name: &'static str,
}

impl ElementKind {
    /// Root container for a UI instance.
    pub const ROOT: Self = Self::new(ROOT_TYPE, "root");
    /// Built-in button control kind.
    pub const BUTTON: Self = Self::new(BUTTON_TYPE, "button");
    /// Built-in text block control kind.
    pub const TEXT_BLOCK: Self = Self::new(TEXT_BLOCK_TYPE, "text-block");
    /// Built-in panel control kind.
    pub const PANEL: Self = Self::new(PANEL_TYPE, "panel");
    /// Built-in row control kind.
    pub const ROW: Self = Self::new(ROW_TYPE, "row");
    /// Built-in toggle control kind.
    pub const TOGGLE: Self = Self::new(TOGGLE_TYPE, "toggle");

    /// Creates an element kind from an application-defined selector type tag.
    #[must_use]
    pub const fn new(type_tag: TypeTag, name: &'static str) -> Self {
        Self { type_tag, name }
    }

    /// Returns the selector type tag for this element kind.
    #[must_use]
    pub const fn type_tag(self) -> TypeTag {
        self.type_tag
    }

    /// Returns the stable debug/display name for this kind.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }
}

/// Dynamic selector state for a semantic element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementState {
    hovered: bool,
    pressed: bool,
    enabled: bool,
}

impl Default for ElementState {
    fn default() -> Self {
        Self {
            hovered: false,
            pressed: false,
            enabled: true,
        }
    }
}

impl ElementState {
    /// Returns whether the element is hovered.
    #[must_use]
    pub const fn hovered(self) -> bool {
        self.hovered
    }

    /// Returns whether the element is pressed.
    #[must_use]
    pub const fn pressed(self) -> bool {
        self.pressed
    }

    /// Returns whether the element is enabled.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    pub(crate) fn set_hovered(&mut self, hovered: bool) -> bool {
        let changed = self.hovered != hovered;
        self.hovered = hovered;
        changed
    }

    pub(crate) fn set_pressed(&mut self, pressed: bool) -> bool {
        let changed = self.pressed != pressed;
        self.pressed = pressed;
        changed
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) -> bool {
        let changed = self.enabled != enabled;
        self.enabled = enabled;
        changed
    }

    pub(crate) fn pseudos(self) -> Vec<PseudoClassId> {
        let mut pseudos = Vec::new();
        if self.hovered {
            pseudos.push(HOVERED);
        }
        if self.pressed {
            pseudos.push(PRESSED);
        }
        if !self.enabled {
            pseudos.push(DISABLED);
        }
        pseudos
    }
}

/// Stored semantic element.
#[derive(Debug)]
pub(crate) struct Element {
    pub(crate) id: ElementId,
    pub(crate) parent: Option<ElementId>,
    pub(crate) children: Vec<ElementId>,
    pub(crate) kind: ElementKind,
    pub(crate) widget: Option<Box<dyn Widget>>,
    pub(crate) state: ElementState,
    pub(crate) classes: Vec<ClassId>,
    pub(crate) style: Option<StyleCascade>,
    pub(crate) style_owner_state: Option<MatchState>,
    pub(crate) style_part_states: Vec<RetainedStyleSubject>,
    store: PropertyStore<ElementId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedStyleSubject {
    pub(crate) path: Vec<PartTag>,
    pub(crate) state: MatchState,
}

impl Element {
    pub(crate) fn new(
        id: ElementId,
        parent: Option<ElementId>,
        kind: ElementKind,
        widget: Option<Box<dyn Widget>>,
    ) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            kind,
            widget,
            state: ElementState::default(),
            classes: Vec::new(),
            style: None,
            style_owner_state: None,
            style_part_states: Vec::new(),
            store: PropertyStore::new(id),
        }
    }
}

impl DependencyObject<ElementId> for Element {
    fn property_store(&self) -> &PropertyStore<ElementId> {
        &self.store
    }

    fn property_store_mut(&mut self) -> &mut PropertyStore<ElementId> {
        &mut self.store
    }

    fn key(&self) -> ElementId {
        self.id
    }

    fn parent_key(&self) -> Option<ElementId> {
        self.parent
    }
}
