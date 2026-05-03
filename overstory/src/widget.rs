// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Open widget behavior attached to semantic elements.

use alloc::{string::String, vec::Vec};
use core::any::Any;

use kurbo::{Insets, Rect, Size};
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};
use ui_events::pointer::PointerEvent;
use ui_input_state::InputState;
use understory_responder::types::{Outcome, Phase};
use understory_style::PseudoClassId;
use understory_timing::TimerId;

use crate::{
    ElementId, ElementKind, PresentationNode, PresentationNodeId, PresentationTree, ROW_PART,
    TOGGLE_THUMB_SLOT, TOGGLE_TRACK_SLOT, TemplateLayout, TemplateSlotLayout, TextContent, Ui,
};

/// Behavior attached to a semantic element.
///
/// The retained element owns identity, tree position, selector state, and
/// dependency properties. The widget owns kind-specific intrinsic measurement
/// and presentation emission.
pub trait Widget: Any + core::fmt::Debug {
    /// Returns the open semantic kind for this widget.
    fn kind(&self) -> ElementKind;

    /// Measures this widget in logical UI coordinates.
    fn measure(&self, ui: &mut Ui, element: ElementId, available: Size) -> Size;

    /// Emits laid-out presentation nodes for this widget.
    fn present(
        &self,
        ui: &mut Ui,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId;

    /// Returns whether this widget should participate in pointer hit testing.
    #[must_use]
    fn hit_testable(&self) -> bool {
        false
    }

    /// Handles a routed pointer event.
    ///
    /// The event is the raw `ui-events` pointer event supplied to [`Ui`](crate::Ui).
    /// `cx` carries Overstory routing context such as responder phase and whether
    /// this target received a recognized click.
    fn pointer_event(&mut self, _cx: &mut PointerEventCx<'_>, _event: &PointerEvent) -> Outcome {
        Outcome::Continue
    }

    /// Handles a routed keyboard event.
    ///
    /// The event is the raw `ui-events` keyboard event supplied to [`Ui`](crate::Ui).
    /// `cx` carries Overstory routing context for the focused element.
    fn keyboard_event(&mut self, _cx: &mut KeyboardEventCx<'_>, _event: &KeyboardEvent) -> Outcome {
        Outcome::Continue
    }

    /// Handles an expired timer owned by this widget.
    ///
    /// Widgets should compare `timer` with any currently stored timer id and
    /// ignore stale deliveries.
    fn timer_event(&mut self, _cx: &mut TimerEventCx, _timer: TimerId) -> Outcome {
        Outcome::Continue
    }

    /// Activates the widget's primary action.
    ///
    /// Returns `true` when activation changed retained widget state.
    fn activate(&mut self) -> bool {
        false
    }

    /// Appends widget-owned selector pseudoclasses.
    fn append_selector_pseudos(&self, _pseudos: &mut Vec<PseudoClassId>) {}

    /// Returns this widget as `Any` for typed access.
    fn as_any(&self) -> &dyn Any;

    /// Returns this widget as mutable `Any` for typed updates.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Per-widget context for a routed pointer event.
#[derive(Clone, Debug)]
pub struct PointerEventCx<'a> {
    element: ElementId,
    phase: Phase,
    clicked: bool,
    input: &'a InputState,
    activate_requested: bool,
    changed: bool,
}

impl<'a> PointerEventCx<'a> {
    pub(crate) const fn new(
        element: ElementId,
        phase: Phase,
        clicked: bool,
        input: &'a InputState,
    ) -> Self {
        Self {
            element,
            phase,
            clicked,
            input,
            activate_requested: false,
            changed: false,
        }
    }

    /// Returns the element currently receiving the event.
    #[must_use]
    pub const fn element(&self) -> ElementId {
        self.element
    }

    /// Returns the responder phase for this delivery.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// Returns whether this delivery is the target phase.
    #[must_use]
    pub const fn is_target(&self) -> bool {
        matches!(self.phase, Phase::Target)
    }

    /// Returns whether this target received a recognized click.
    #[must_use]
    pub const fn clicked(&self) -> bool {
        self.clicked
    }

    /// Returns frame-oriented input state at the time of dispatch.
    #[must_use]
    pub const fn input(&self) -> &InputState {
        self.input
    }

    /// Requests primary activation after the widget handler returns.
    pub fn activate(&mut self) {
        self.activate_requested = true;
    }

    /// Marks the receiving widget as changed.
    ///
    /// Use this when event handling mutates widget-owned state that can affect
    /// selector state, measurement, arrangement, or visual output.
    pub fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub(crate) const fn activate_requested(&self) -> bool {
        self.activate_requested
    }

    pub(crate) const fn changed(&self) -> bool {
        self.changed
    }
}

/// Per-widget context for a routed keyboard event.
#[derive(Clone, Debug)]
pub struct KeyboardEventCx<'a> {
    element: ElementId,
    phase: Phase,
    input: &'a InputState,
    activate_requested: bool,
    changed: bool,
}

/// Per-widget context for an expired timer.
#[derive(Clone, Debug)]
pub struct TimerEventCx {
    element: ElementId,
    changed: bool,
    rearm: bool,
}

impl TimerEventCx {
    pub(crate) const fn new(element: ElementId) -> Self {
        Self {
            element,
            changed: false,
            rearm: true,
        }
    }

    /// Returns the element currently receiving the timer.
    #[must_use]
    pub const fn element(&self) -> ElementId {
        self.element
    }

    /// Marks the receiving widget as changed.
    pub fn mark_changed(&mut self) {
        self.changed = true;
    }

    /// Prevents a repeating timer from being rearmed after this delivery.
    pub fn cancel_rearm(&mut self) {
        self.rearm = false;
    }

    pub(crate) const fn changed(&self) -> bool {
        self.changed
    }

    pub(crate) const fn should_rearm(&self) -> bool {
        self.rearm
    }
}

impl<'a> KeyboardEventCx<'a> {
    pub(crate) const fn new(element: ElementId, phase: Phase, input: &'a InputState) -> Self {
        Self {
            element,
            phase,
            input,
            activate_requested: false,
            changed: false,
        }
    }

    /// Returns the element currently receiving the event.
    #[must_use]
    pub const fn element(&self) -> ElementId {
        self.element
    }

    /// Returns the responder phase for this delivery.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// Returns whether this delivery is the target phase.
    #[must_use]
    pub const fn is_target(&self) -> bool {
        matches!(self.phase, Phase::Target)
    }

    /// Returns frame-oriented input state at the time of dispatch.
    #[must_use]
    pub const fn input(&self) -> &InputState {
        self.input
    }

    /// Requests primary activation after the widget handler returns.
    pub fn activate(&mut self) {
        self.activate_requested = true;
    }

    /// Marks the receiving widget as changed.
    ///
    /// Use this when event handling mutates widget-owned state that can affect
    /// selector state, measurement, arrangement, or visual output.
    pub fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub(crate) const fn activate_requested(&self) -> bool {
        self.activate_requested
    }

    pub(crate) const fn changed(&self) -> bool {
        self.changed
    }
}

fn is_keyboard_activation(event: &KeyboardEvent) -> bool {
    if event.state != KeyState::Down || event.repeat {
        return false;
    }
    match &event.key {
        Key::Named(NamedKey::Enter) => true,
        Key::Character(text) => text == " ",
        _ => false,
    }
}

/// Interactive text-bearing push button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    content: TextContent,
}

impl Button {
    /// Creates a button with text content.
    #[must_use]
    pub fn new(content: impl Into<TextContent>) -> Self {
        Self {
            content: content.into(),
        }
    }

    /// Returns the button text content.
    #[must_use]
    pub const fn content(&self) -> &TextContent {
        &self.content
    }

    /// Replaces the button text content.
    pub fn set_content(&mut self, content: impl Into<TextContent>) {
        self.content = content.into();
    }
}

impl Widget for Button {
    fn kind(&self) -> ElementKind {
        ElementKind::BUTTON
    }

    fn measure(&self, ui: &mut Ui, element: ElementId, available: Size) -> Size {
        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let min_width = ui.resolve::<f64>(element, props.min_width);
        let text_style = ui.resolve_text_style(element);
        let max_text_width = crate::text::text_width_f32(available.width - padding.x_value());
        let content_size = ui.measure_text(&self.content, &text_style, Some(max_text_width));
        Size::new(
            (content_size.width + padding.x_value()).max(min_width.max(0.0)),
            content_size.height + padding.y_value(),
        )
    }

    fn present(
        &self,
        ui: &mut Ui,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId {
        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let text_style = ui.resolve_text_style(element);
        let content_size = ui.measure_text(&self.content, &text_style, None);
        let available_content_width = (bounds.width() - padding.x_value()).max(0.0);
        let extra_content_width = (available_content_width - content_size.width).max(0.0);
        let content_bounds = Rect::from_origin_size(
            (
                bounds.x0 + padding.x0 + extra_content_width * 0.5,
                bounds.y0 + padding.y0,
            ),
            content_size,
        );
        let template = ui.resolve(element, props.template);
        let mut values = ui.template_values(element, Some(self.content.clone()));
        let id = crate::template::instantiate_template(
            tree,
            parent,
            element,
            &template,
            TemplateLayout::new(bounds, content_bounds),
            &mut values,
        );
        let subjects = values.into_retained_subjects();
        ui.retain_style_subjects(element, subjects);
        id
    }

    fn hit_testable(&self) -> bool {
        true
    }

    fn pointer_event(&mut self, cx: &mut PointerEventCx<'_>, _event: &PointerEvent) -> Outcome {
        if cx.is_target() && cx.clicked() {
            cx.activate();
        }
        Outcome::Continue
    }

    fn keyboard_event(&mut self, cx: &mut KeyboardEventCx<'_>, event: &KeyboardEvent) -> Outcome {
        if cx.is_target() && is_keyboard_activation(event) {
            cx.activate();
        }
        Outcome::Continue
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Wrapped text label/content block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBlock {
    content: TextContent,
}

impl TextBlock {
    /// Creates a text block with text content.
    #[must_use]
    pub fn new(content: impl Into<TextContent>) -> Self {
        Self {
            content: content.into(),
        }
    }

    /// Returns the text content.
    #[must_use]
    pub const fn content(&self) -> &TextContent {
        &self.content
    }

    /// Replaces the text block content.
    pub fn set_content(&mut self, content: impl Into<TextContent>) {
        self.content = content.into();
    }
}

impl Widget for TextBlock {
    fn kind(&self) -> ElementKind {
        ElementKind::TEXT_BLOCK
    }

    fn measure(&self, ui: &mut Ui, element: ElementId, available: Size) -> Size {
        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let min_width = ui.resolve::<f64>(element, props.min_width);
        let text_style = ui.resolve_text_style(element);
        let max_text_width = crate::text::text_width_f32(available.width - padding.x_value());
        let content_size = ui.measure_text(&self.content, &text_style, Some(max_text_width));
        Size::new(
            (content_size.width + padding.x_value()).max(min_width.max(0.0)),
            content_size.height + padding.y_value(),
        )
    }

    fn present(
        &self,
        ui: &mut Ui,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId {
        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let text_style = ui.resolve_text_style(element);
        let max_text_width = crate::text::text_width_f32(bounds.width() - padding.x_value());
        let content_size = ui.measure_text(&self.content, &text_style, Some(max_text_width));
        let content_bounds = Rect::from_origin_size(
            (bounds.x0 + padding.x0, bounds.y0 + padding.y0),
            content_size,
        );
        let template = ui.resolve(element, props.text_template);
        let mut values = ui.template_values(element, Some(self.content.clone()));
        let id = crate::template::instantiate_template(
            tree,
            parent,
            element,
            &template,
            TemplateLayout::new(bounds, content_bounds),
            &mut values,
        );
        let subjects = values.into_retained_subjects();
        ui.retain_style_subjects(element, subjects);
        id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Simple vertical container widget.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Panel;

impl Panel {
    /// Creates a panel.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Widget for Panel {
    fn kind(&self) -> ElementKind {
        ElementKind::PANEL
    }

    fn measure(&self, ui: &mut Ui, element: ElementId, available: Size) -> Size {
        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let spacing = ui.resolve::<f64>(element, props.spacing);
        let child_available = Size::new(
            (available.width - padding.x_value()).max(0.0),
            available.height,
        );
        let mut total_height = padding.y_value();
        let mut max_width: f64 = 0.0;
        for (count, child) in ui.children(element).into_iter().enumerate() {
            let size = ui.measure_child(child, child_available);
            if count > 0 {
                total_height += spacing;
            }
            total_height += size.height;
            max_width = max_width.max(size.width);
        }
        Size::new(max_width + padding.x_value(), total_height)
    }

    fn present(
        &self,
        ui: &mut Ui,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId {
        let mut node = PresentationNode::new(element, ElementKind::PANEL.part_kind(), bounds);
        let props = ui.properties();
        node.background = ui.resolve::<Option<peniko::Brush>>(element, props.background);
        node.border = ui.resolve::<Option<peniko::Brush>>(element, props.border);
        node.border_width = ui.resolve::<f64>(element, props.border_width);
        node.corner_radius = ui.resolve::<f64>(element, props.corner_radius);
        let id = tree.push_child(parent, node);

        let padding = ui.resolve::<Insets>(element, props.padding);
        let spacing = ui.resolve::<f64>(element, props.spacing);
        let child_width = (bounds.width() - padding.x_value()).max(0.0);
        let mut cursor_y = bounds.y0 + padding.y0;
        for (count, child) in ui.children(element).into_iter().enumerate() {
            if count > 0 {
                cursor_y += spacing;
            }
            let size = ui.measure_child(child, Size::new(child_width, f64::INFINITY));
            let child_bounds = Rect::from_origin_size(
                (bounds.x0 + padding.x0, cursor_y),
                (size.width, size.height),
            );
            ui.present_child(tree, id, child, child_bounds);
            cursor_y += size.height;
        }
        id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Simple horizontal container widget.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row;

impl Row {
    /// Creates a row.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Widget for Row {
    fn kind(&self) -> ElementKind {
        ElementKind::ROW
    }

    fn measure(&self, ui: &mut Ui, element: ElementId, available: Size) -> Size {
        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let spacing = ui.resolve::<f64>(element, props.spacing);
        let child_available = Size::new(
            (available.width - padding.x_value()).max(0.0),
            (available.height - padding.y_value()).max(0.0),
        );
        let mut total_width = padding.x_value();
        let mut max_height: f64 = 0.0;
        for (count, child) in ui.children(element).into_iter().enumerate() {
            let size = ui.measure_child(child, child_available);
            if count > 0 {
                total_width += spacing;
            }
            total_width += size.width;
            max_height = max_height.max(size.height);
        }
        Size::new(total_width, max_height + padding.y_value())
    }

    fn present(
        &self,
        ui: &mut Ui,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId {
        let mut node = PresentationNode::new(element, ROW_PART, bounds);
        let props = ui.properties();
        node.background = ui.resolve::<Option<peniko::Brush>>(element, props.background);
        node.border = ui.resolve::<Option<peniko::Brush>>(element, props.border);
        node.border_width = ui.resolve::<f64>(element, props.border_width);
        node.corner_radius = ui.resolve::<f64>(element, props.corner_radius);
        let id = tree.push_child(parent, node);

        let padding = ui.resolve::<Insets>(element, props.padding);
        let spacing = ui.resolve::<f64>(element, props.spacing);
        let child_available = Size::new(
            (bounds.width() - padding.x_value()).max(0.0),
            (bounds.height() - padding.y_value()).max(0.0),
        );
        let mut cursor_x = bounds.x0 + padding.x0;
        for (count, child) in ui.children(element).into_iter().enumerate() {
            if count > 0 {
                cursor_x += spacing;
            }
            let size = ui.measure_child(child, child_available);
            let child_bounds = Rect::from_origin_size(
                (cursor_x, bounds.y0 + padding.y0),
                (size.width, size.height),
            );
            ui.present_child(tree, id, child, child_bounds);
            cursor_x += size.width;
        }
        id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Stateful on/off toggle with text label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toggle {
    content: TextContent,
    checked: bool,
}

impl Toggle {
    /// Creates a toggle with text content.
    #[must_use]
    pub fn new(content: impl Into<TextContent>) -> Self {
        Self {
            content: content.into(),
            checked: false,
        }
    }

    /// Returns the toggle text content.
    #[must_use]
    pub const fn content(&self) -> &TextContent {
        &self.content
    }

    /// Replaces the toggle text content.
    pub fn set_content(&mut self, content: impl Into<TextContent>) {
        self.content = content.into();
    }

    /// Returns whether the toggle is checked.
    #[must_use]
    pub const fn checked(&self) -> bool {
        self.checked
    }

    /// Sets whether the toggle is checked.
    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }
}

impl Widget for Toggle {
    fn kind(&self) -> ElementKind {
        ElementKind::TOGGLE
    }

    fn measure(&self, ui: &mut Ui, element: ElementId, available: Size) -> Size {
        const TRACK_WIDTH: f64 = 42.0;
        const TRACK_HEIGHT: f64 = 24.0;
        const LABEL_GAP: f64 = 10.0;

        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let min_width = ui.resolve::<f64>(element, props.min_width);
        let text_style = ui.resolve_text_style(element);
        let label_width = (available.width - padding.x_value() - TRACK_WIDTH - LABEL_GAP).max(1.0);
        let label_size = ui.measure_text(
            &self.content,
            &text_style,
            Some(crate::text::text_width_f32(label_width)),
        );
        Size::new(
            (padding.x_value() + TRACK_WIDTH + LABEL_GAP + label_size.width).max(min_width),
            padding.y_value() + TRACK_HEIGHT.max(label_size.height),
        )
    }

    fn present(
        &self,
        ui: &mut Ui,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId {
        const TRACK_WIDTH: f64 = 42.0;
        const TRACK_HEIGHT: f64 = 24.0;
        const THUMB_SIZE: f64 = 18.0;
        const LABEL_GAP: f64 = 10.0;

        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let text_style = ui.resolve_text_style(element);
        let label_width = (bounds.width() - padding.x_value() - TRACK_WIDTH - LABEL_GAP).max(1.0);
        let label_size = ui.measure_text(
            &self.content,
            &text_style,
            Some(crate::text::text_width_f32(label_width)),
        );

        let content_y = bounds.y0 + padding.y0;
        let track_y =
            content_y + ((bounds.height() - padding.y_value()) - TRACK_HEIGHT).max(0.0) * 0.5;
        let track_bounds = Rect::from_origin_size(
            (bounds.x0 + padding.x0, track_y),
            (TRACK_WIDTH, TRACK_HEIGHT),
        );
        let thumb_x = if self.checked {
            track_bounds.x1 - THUMB_SIZE - 3.0
        } else {
            track_bounds.x0 + 3.0
        };
        let thumb_y = track_bounds.y0 + (TRACK_HEIGHT - THUMB_SIZE) * 0.5;
        let thumb_bounds = Rect::from_origin_size((thumb_x, thumb_y), (THUMB_SIZE, THUMB_SIZE));

        let label_bounds = Rect::from_origin_size(
            (
                track_bounds.x1 + LABEL_GAP,
                content_y
                    + ((bounds.height() - padding.y_value()) - label_size.height).max(0.0) * 0.5,
            ),
            label_size,
        );
        let template = ui.resolve(element, props.toggle_template);
        let checked_pseudos = if self.checked {
            &[crate::CHECKED][..]
        } else {
            &[][..]
        };
        let mut values =
            ui.template_values_with_pseudos(element, Some(self.content.clone()), checked_pseudos);
        let id = crate::template::instantiate_template(
            tree,
            parent,
            element,
            &template,
            TemplateLayout::new(bounds, label_bounds).with_slots([
                TemplateSlotLayout::new(TOGGLE_TRACK_SLOT, track_bounds),
                TemplateSlotLayout::new(TOGGLE_THUMB_SLOT, thumb_bounds),
            ]),
            &mut values,
        );
        let subjects = values.into_retained_subjects();
        ui.retain_style_subjects(element, subjects);
        id
    }

    fn hit_testable(&self) -> bool {
        true
    }

    fn activate(&mut self) -> bool {
        self.checked = !self.checked;
        true
    }

    fn pointer_event(&mut self, cx: &mut PointerEventCx<'_>, _event: &PointerEvent) -> Outcome {
        if cx.is_target() && cx.clicked() {
            cx.activate();
        }
        Outcome::Continue
    }

    fn keyboard_event(&mut self, cx: &mut KeyboardEventCx<'_>, event: &KeyboardEvent) -> Outcome {
        if cx.is_target() && is_keyboard_activation(event) {
            cx.activate();
        }
        Outcome::Continue
    }

    fn append_selector_pseudos(&self, pseudos: &mut Vec<PseudoClassId>) {
        if self.checked {
            pseudos.push(crate::CHECKED);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl ElementKind {
    pub(crate) fn part_kind(self) -> crate::PartKind {
        crate::PartKind::new(self.name())
    }
}

impl From<&str> for Button {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Button {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for TextBlock {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for TextBlock {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for Toggle {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Toggle {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
