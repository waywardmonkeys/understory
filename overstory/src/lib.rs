// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Overstory: semantic UI runtime policy on top of Understory primitives.
//!
//! **Fence:** this crate owns semantic controls, UI properties, style resolution,
//! template expansion, measure/arrange layout, and visual emission. It
//! explicitly does not own renderer backends, platform event loops, or the
//! reusable structural/input substrate supplied by Understory crates.
//!
//! The initial pipeline is intentionally small:
//!
//! ```text
//! semantic control -> style/property resolution -> template -> measure -> arrange -> imaging scene
//! ```
//!
//! Rendering output is an [`imaging::record::Scene`]. Overstory does not invent a
//! local display-list vocabulary.

#![no_std]

extern crate alloc;

mod channel;
mod element;
mod id;
mod presentation;
mod property;
pub mod style;
mod template;
mod text;
mod ui;
mod visual;
mod widget;

pub use channel::{ARRANGE, MEASURE, STYLE, TEMPLATE, VISUAL};
pub use element::{
    BUTTON_TYPE, CHECKED, DISABLED, ElementKind, ElementState, HOVERED, PANEL_TYPE, PRESSED,
    ROOT_TYPE, ROW_TYPE, TEXT_BLOCK_TYPE, TOGGLE_TYPE,
};
pub use id::ElementId;
pub use presentation::{PresentationNode, PresentationNodeId, PresentationTree};
pub use property::{TextContent, TextStyle, UiProperties};
pub use style::{StyleInspection, StyleRuleInspection, StyleSourceInspection, StyleSubject};
pub use template::{
    BACKGROUND_PROPERTY, BORDER_PART, BORDER_PROPERTY, BORDER_WIDTH_PROPERTY, BUTTON_PART,
    CONTENT_PRESENTER_PART, CONTENT_PROPERTY, CONTENT_SLOT, CONTENT_SLOT_PART_TAG,
    CORNER_RADIUS_PROPERTY, ControlTemplate, FOREGROUND_PROPERTY, PADDING_PROPERTY, PartKind,
    ROOT_PART, ROOT_SLOT, ROW_PART, TEXT_BLOCK_PART, TOGGLE_PART, TOGGLE_THUMB_PART,
    TOGGLE_THUMB_SLOT, TOGGLE_THUMB_SLOT_PART_TAG, TOGGLE_TRACK_PART, TOGGLE_TRACK_SLOT,
    TOGGLE_TRACK_SLOT_PART_TAG, TemplateBinding, TemplateLayout, TemplateNode, TemplateProperty,
    TemplateSlot, TemplateSlotLayout, button_template, text_block_template, toggle_template,
};
pub use text::{TextGlyph, TextGlyphRun, TextSystem};
pub use ui::Ui;
pub use visual::{lower_presentation, lower_presentation_with_scale};
pub use widget::{Button, Panel, PointerEventCx, Row, TextBlock, Toggle, Widget};
