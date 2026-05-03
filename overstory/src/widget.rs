// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Open widget behavior attached to semantic elements.

use alloc::{string::String, vec::Vec};
use core::{any::Any, cell::Cell, num::NonZeroU64};

use kurbo::{Insets, Point, Rect, Size};
use parley::PlainEditor;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};
use ui_events::pointer::{PointerButton, PointerEvent};
use ui_input_state::InputState;
use understory_responder::types::{Outcome, Phase};
use understory_style::PseudoClassId;
use understory_timing::{TimerDuration, TimerId, TimerInstant, TimerQueue, TimerRepeat};

use crate::{
    ElementId, ElementKind, PresentationNode, PresentationNodeId, PresentationTree, ROW_PART,
    TEXT_CARET_PART, TEXT_SELECTION_PART, TOGGLE_THUMB_SLOT, TOGGLE_TRACK_SLOT, TemplateLayout,
    TemplateSlotLayout, TextContent, TextSystem, Ui,
};

const TEXT_INPUT_BLINK_INTERVAL: TimerDuration = 500_000_000;

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
        &mut self,
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

    /// Handles a focus change for this widget.
    fn focus_event(&mut self, _cx: &mut FocusEventCx<'_>, _focused: bool) -> Outcome {
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
#[derive(Debug)]
pub struct PointerEventCx<'a> {
    element: ElementId,
    phase: Phase,
    clicked: bool,
    input: &'a InputState,
    text: &'a mut TextSystem,
    timers: &'a mut TimerQueue<ElementId>,
    activate_requested: bool,
    changed: bool,
}

impl<'a> PointerEventCx<'a> {
    pub(crate) fn new(
        element: ElementId,
        phase: Phase,
        clicked: bool,
        input: &'a InputState,
        text: &'a mut TextSystem,
        timers: &'a mut TimerQueue<ElementId>,
    ) -> Self {
        Self {
            element,
            phase,
            clicked,
            input,
            text,
            timers,
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

    /// Returns the shared text system for text editing operations.
    pub fn text(&mut self) -> &mut TextSystem {
        self.text
    }

    /// Schedules a timer targeted at the receiving widget.
    pub fn schedule_timer(
        &mut self,
        now: TimerInstant,
        delay: TimerDuration,
        repeat: TimerRepeat,
    ) -> TimerId {
        self.timers.schedule(self.element, now, delay, repeat)
    }

    /// Cancels a timer previously scheduled for a widget.
    pub fn cancel_timer(&mut self, timer: TimerId) -> bool {
        self.timers.cancel(timer)
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
#[derive(Debug)]
pub struct KeyboardEventCx<'a> {
    element: ElementId,
    phase: Phase,
    input: &'a InputState,
    text: &'a mut TextSystem,
    timers: &'a mut TimerQueue<ElementId>,
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

/// Per-widget context for focus changes.
#[derive(Debug)]
pub struct FocusEventCx<'a> {
    element: ElementId,
    now: Option<TimerInstant>,
    timers: &'a mut TimerQueue<ElementId>,
    changed: bool,
}

impl<'a> FocusEventCx<'a> {
    pub(crate) const fn new(
        element: ElementId,
        now: Option<TimerInstant>,
        timers: &'a mut TimerQueue<ElementId>,
    ) -> Self {
        Self {
            element,
            now,
            timers,
            changed: false,
        }
    }

    /// Returns the element receiving the focus change.
    #[must_use]
    pub const fn element(&self) -> ElementId {
        self.element
    }

    /// Returns the host timestamp associated with this focus change, if supplied.
    #[must_use]
    pub const fn now(&self) -> Option<TimerInstant> {
        self.now
    }

    /// Schedules a timer targeted at the receiving widget when a timestamp is available.
    pub fn schedule_timer(&mut self, delay: TimerDuration, repeat: TimerRepeat) -> Option<TimerId> {
        self.now
            .map(|now| self.timers.schedule(self.element, now, delay, repeat))
    }

    /// Cancels a timer previously scheduled for a widget.
    pub fn cancel_timer(&mut self, timer: TimerId) -> bool {
        self.timers.cancel(timer)
    }

    /// Marks the receiving widget as changed.
    pub fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub(crate) const fn changed(&self) -> bool {
        self.changed
    }
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
    pub(crate) fn new(
        element: ElementId,
        phase: Phase,
        input: &'a InputState,
        text: &'a mut TextSystem,
        timers: &'a mut TimerQueue<ElementId>,
    ) -> Self {
        Self {
            element,
            phase,
            input,
            text,
            timers,
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

    /// Returns the shared text system for text editing operations.
    pub fn text(&mut self) -> &mut TextSystem {
        self.text
    }

    /// Schedules a timer targeted at the receiving widget.
    pub fn schedule_timer(
        &mut self,
        now: TimerInstant,
        delay: TimerDuration,
        repeat: TimerRepeat,
    ) -> TimerId {
        self.timers.schedule(self.element, now, delay, repeat)
    }

    /// Cancels a timer previously scheduled for a widget.
    pub fn cancel_timer(&mut self, timer: TimerId) -> bool {
        self.timers.cancel(timer)
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
        &mut self,
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
        &mut self,
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

/// Editable plain text input backed by Parley.
pub struct TextInput {
    editor: PlainEditor<peniko::Brush>,
    cached_cursor_rect: Option<Rect>,
    cached_selection_rects: Vec<Rect>,
    last_content_width: Cell<Option<f32>>,
    last_content_origin: Cell<Point>,
    editor_layout_width: Option<f32>,
    editor_text_style: Option<crate::TextStyle>,
    placeholder: Option<TextContent>,
    single_line: bool,
    cursor_visible: bool,
    blink_timer: Option<TimerId>,
}

impl core::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextInput")
            .field("text_len", &self.editor.raw_text().len())
            .field("selection_rects", &self.cached_selection_rects.len())
            .field("placeholder", &self.placeholder)
            .field("single_line", &self.single_line)
            .field("cursor_visible", &self.cursor_visible)
            .finish_non_exhaustive()
    }
}

impl TextInput {
    /// Creates an empty text input.
    #[must_use]
    pub fn new() -> Self {
        Self {
            editor: PlainEditor::new(16.0),
            cached_cursor_rect: None,
            cached_selection_rects: Vec::new(),
            last_content_width: Cell::new(None),
            last_content_origin: Cell::new(Point::ORIGIN),
            editor_layout_width: None,
            editor_text_style: None,
            placeholder: None,
            single_line: true,
            cursor_visible: true,
            blink_timer: None,
        }
    }

    /// Creates a text input with initial text.
    #[must_use]
    pub fn with_text(mut self, text: &str) -> Self {
        self.set_text(text);
        self
    }

    /// Returns the current committed text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.editor.raw_text()
    }

    /// Replaces the text buffer.
    pub fn set_text(&mut self, text: &str) {
        self.editor.set_text(text);
    }

    /// Sets placeholder text shown while the input is empty.
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<TextContent>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Restricts editing to a single line.
    #[must_use]
    pub fn single_line(mut self, single_line: bool) -> Self {
        self.single_line = single_line;
        self
    }

    fn display_content(&self) -> Option<TextContent> {
        if self.editor.raw_text().is_empty() {
            self.placeholder.clone()
        } else {
            Some(TextContent::from(self.editor.raw_text()))
        }
    }

    fn move_cursor_to_view_point(&mut self, point: Point, text: &mut TextSystem) {
        let origin = self.last_content_origin.get();
        let local_x = f64_to_f32(point.x - origin.x);
        let local_y = f64_to_f32(point.y - origin.y);
        text.with_plain_editor(&mut self.editor, |driver| {
            driver.move_to_point(local_x, local_y);
        });
        self.cursor_visible = true;
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextInput {
    fn kind(&self) -> ElementKind {
        ElementKind::TEXT_INPUT
    }

    fn measure(&self, ui: &mut Ui, element: ElementId, available: Size) -> Size {
        let props = ui.properties();
        let padding = ui.resolve::<Insets>(element, props.padding);
        let min_width = ui.resolve::<f64>(element, props.min_width).max(160.0);
        let text_style = ui.resolve_text_style(element);
        let max_text_width = crate::text::text_width_f32(available.width - padding.x_value());
        let content = self
            .display_content()
            .unwrap_or_else(|| TextContent::from(" "));
        let content_size = ui.measure_text(&content, &text_style, Some(max_text_width));
        Size::new(
            (content_size.width + padding.x_value()).max(min_width),
            content_size.height + padding.y_value(),
        )
    }

    fn present(
        &mut self,
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
        let content = self.display_content();
        let content_width = crate::text::text_width_f32(bounds.width() - padding.x_value());
        let editor_content_size = if self.editor_layout_width == Some(content_width)
            && self.editor_text_style.as_ref() == Some(&text_style)
        {
            ui.plain_editor_layout_size(&mut self.editor)
        } else {
            self.editor_layout_width = Some(content_width);
            self.editor_text_style = Some(text_style.clone());
            ui.refresh_plain_editor_layout(&mut self.editor, content_width, &text_style)
        };
        let content_size = if self.editor.raw_text().is_empty() {
            content.as_ref().map_or(Size::ZERO, |content| {
                ui.measure_text(content, &text_style, Some(max_text_width))
            })
        } else {
            editor_content_size
        };
        let content_bounds = Rect::from_origin_size(
            (bounds.x0 + padding.x0, bounds.y0 + padding.y0),
            content_size,
        );
        self.last_content_width.set(Some(content_width));
        self.last_content_origin.set(content_bounds.origin());
        self.cached_cursor_rect = self
            .editor
            .cursor_geometry(2.0)
            .map(|rect| Rect::new(rect.x0, rect.y0, rect.x1, rect.y1));
        let mut selection_rects = Vec::new();
        self.editor.selection_geometry_with(|rect, _line| {
            selection_rects.push(Rect::new(rect.x0, rect.y0, rect.x1, rect.y1));
        });
        self.cached_selection_rects = selection_rects;

        let template = ui.resolve(element, props.text_input_template);
        let mut values = ui.template_values(element, content);
        let id = crate::template::instantiate_template(
            tree,
            parent,
            element,
            &template,
            TemplateLayout::new(bounds, content_bounds),
            &mut values,
        );
        let focused = ui.focused() == Some(element);
        let selection_brush =
            peniko::Brush::Solid(peniko::Color::from_rgba8(0x67, 0x9c, 0xff, 0x66));
        for selection in &self.cached_selection_rects {
            let mut node = PresentationNode::new(
                element,
                TEXT_SELECTION_PART,
                *selection + content_bounds.origin().to_vec2(),
            );
            node.background = Some(selection_brush.clone());
            tree.push_child(id, node);
        }
        if focused
            && self.cursor_visible
            && let Some(cursor) = self.cached_cursor_rect
        {
            let mut node = PresentationNode::new(
                element,
                TEXT_CARET_PART,
                cursor + content_bounds.origin().to_vec2(),
            );
            node.background = ui.resolve::<Option<peniko::Brush>>(element, props.foreground);
            tree.push_child(id, node);
        }
        let subjects = values.into_retained_subjects();
        ui.retain_style_subjects(element, subjects);
        id
    }

    fn hit_testable(&self) -> bool {
        true
    }

    fn keyboard_event(&mut self, cx: &mut KeyboardEventCx<'_>, event: &KeyboardEvent) -> Outcome {
        if !cx.is_target() || event.state != KeyState::Down || event.is_composing {
            return Outcome::Continue;
        }

        let action_modifier = event.modifiers.ctrl() || event.modifiers.meta();
        let changed = cx
            .text()
            .with_plain_editor(&mut self.editor, |driver| match &event.key {
                Key::Character(text) if action_modifier && text.eq_ignore_ascii_case("a") => {
                    driver.select_all();
                    true
                }
                Key::Character(text) if !action_modifier => {
                    if self.single_line && (text.contains('\n') || text.contains('\r')) {
                        false
                    } else {
                        driver.insert_or_replace_selection(text);
                        true
                    }
                }
                Key::Named(NamedKey::Enter) if !self.single_line => {
                    driver.insert_or_replace_selection("\n");
                    true
                }
                Key::Named(NamedKey::Backspace) => {
                    if action_modifier {
                        driver.backdelete_word();
                    } else {
                        driver.backdelete();
                    }
                    true
                }
                Key::Named(NamedKey::Delete) => {
                    if action_modifier {
                        driver.delete_word();
                    } else {
                        driver.delete();
                    }
                    true
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    if event.modifiers.shift() {
                        if action_modifier {
                            driver.select_word_left();
                        } else {
                            driver.select_left();
                        }
                    } else if action_modifier {
                        driver.move_word_left();
                    } else {
                        driver.move_left();
                    }
                    true
                }
                Key::Named(NamedKey::ArrowRight) => {
                    if event.modifiers.shift() {
                        if action_modifier {
                            driver.select_word_right();
                        } else {
                            driver.select_right();
                        }
                    } else if action_modifier {
                        driver.move_word_right();
                    } else {
                        driver.move_right();
                    }
                    true
                }
                Key::Named(NamedKey::Home) => {
                    if event.modifiers.shift() {
                        driver.select_to_line_start();
                    } else {
                        driver.move_to_line_start();
                    }
                    true
                }
                Key::Named(NamedKey::End) => {
                    if event.modifiers.shift() {
                        driver.select_to_line_end();
                    } else {
                        driver.move_to_line_end();
                    }
                    true
                }
                _ => false,
            });
        if changed {
            self.cursor_visible = true;
            cx.mark_changed();
        }
        Outcome::Continue
    }

    fn pointer_event(&mut self, cx: &mut PointerEventCx<'_>, event: &PointerEvent) -> Outcome {
        let PointerEvent::Down(button) = event else {
            return Outcome::Continue;
        };
        if !cx.is_target() || button.button != Some(PointerButton::Primary) {
            return Outcome::Continue;
        }
        self.move_cursor_to_view_point(button.state.logical_point(), cx.text());
        cx.mark_changed();
        Outcome::Continue
    }

    fn timer_event(&mut self, cx: &mut TimerEventCx, timer: TimerId) -> Outcome {
        if self.blink_timer != Some(timer) {
            cx.cancel_rearm();
            return Outcome::Continue;
        }
        self.cursor_visible = !self.cursor_visible;
        cx.mark_changed();
        Outcome::Continue
    }

    fn focus_event(&mut self, cx: &mut FocusEventCx<'_>, focused: bool) -> Outcome {
        if focused {
            self.cursor_visible = true;
            if self.blink_timer.is_none() {
                let interval =
                    NonZeroU64::new(TEXT_INPUT_BLINK_INTERVAL).expect("blink interval is nonzero");
                self.blink_timer =
                    cx.schedule_timer(TEXT_INPUT_BLINK_INTERVAL, TimerRepeat::coalescing(interval));
            }
            cx.mark_changed();
        } else {
            if let Some(timer) = self.blink_timer.take() {
                let _ = cx.cancel_timer(timer);
            }
            self.cursor_visible = true;
            cx.mark_changed();
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
        &mut self,
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
        &mut self,
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
        &mut self,
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

#[expect(
    clippy::cast_possible_truncation,
    reason = "Pointer coordinates passed to Parley are clamped to the f32 range used by its editor API."
)]
fn f64_to_f32(value: f64) -> f32 {
    value.max(f64::from(f32::MIN)).min(f64::from(f32::MAX)) as f32
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
