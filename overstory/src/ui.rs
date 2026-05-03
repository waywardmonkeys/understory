// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Top-level semantic UI container.

use alloc::{vec, vec::Vec};

use imaging::record;
use invalidation::{ChannelSet, EagerPolicy, InvalidationTracker};
use kurbo::{Insets, Point, Rect, Size};
use peniko::Brush;
use ui_events::keyboard::KeyboardEvent;
use ui_events::pointer::{
    PointerButton, PointerButtonEvent, PointerEvent, PointerInfo, PointerUpdate,
};
use ui_input_state::InputState;
use understory_box_tree::{
    LocalNode, NodeFlags, NodeId as BoxNodeId, QueryFilter, Tree as BoxTree,
};
use understory_event_state::{
    click::{ClickResult, ClickState},
    hover::{HoverEvent, HoverState},
};
use understory_property::{
    DependencyObject, DependencyObjectExt, Property, PropertyMetadata, PropertyRegistry,
};
use understory_responder::{
    dispatcher,
    router::{Router, path_from_dispatch},
    types::{DepthKey, Dispatch, Localizer, Outcome, Phase, ResolvedHitRef, WidgetLookup},
};
use understory_style::{
    ClassId, MatchState, PartTag, PseudoClassId, ResolveCx, SelectorInputs, SelectorInputsOwned,
    StyleCascade, Theme, ThemeBuilder,
};

use crate::element::{Element, RetainedStyleSubject};
use crate::style::{StyleInspection, StyleRuleInspection, StyleSourceInspection, StyleSubject};
use crate::template::{TemplateBinding, TemplateValueSource};
use crate::{
    ARRANGE, AppendSpec, Button, ElementId, ElementKind, ElementState, MEASURE, Panel,
    PresentationNode, PresentationNodeId, PresentationTree, ROOT_PART, Row, STYLE, TEMPLATE,
    TemplateSlot, TextBlock, TextContent, TextStyle, TextSystem, Toggle, UiProperties, VISUAL,
    widget::{KeyboardEventCx, PointerEventCx, Widget},
};

/// Retained semantic UI runtime.
///
/// `Ui` owns semantic elements and coordinates invalidation across the
/// style-template-measure-arrange-visual pipeline.
#[derive(Debug)]
pub struct Ui {
    elements: Vec<Element>,
    registry: PropertyRegistry,
    properties: UiProperties,
    theme: Theme,
    invalidation: InvalidationTracker<ElementId>,
    text: TextSystem,
    hover: HoverState<ElementId>,
    clicks: ClickState<ElementId>,
    pressed: Option<ElementId>,
    focused: Option<ElementId>,
    input: InputState,
    responder: ResponderState,
    presentation: PresentationTree,
    presentation_viewport: Option<Size>,
    box_tree: BoxTree,
    box_targets: Vec<BoxTarget>,
    scene: record::Scene,
    scene_scale_factor: f64,
    scene_valid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoxTarget {
    box_node: BoxNodeId,
    element: ElementId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HitTarget {
    element: ElementId,
    path: Vec<ElementId>,
}

type ResponderDispatch = Dispatch<ElementId, ElementId, ()>;
type ElementRouter = Router<ElementId, ElementWidgetLookup>;

#[derive(Clone, Debug, Default)]
struct PointerRoute {
    target: Option<ElementId>,
    dispatches: Vec<ResponderDispatch>,
}

#[derive(Debug)]
struct ResponderState {
    pointer_capture: Option<ElementId>,
    router: ElementRouter,
}

impl ResponderState {
    fn new() -> Self {
        Self {
            pointer_capture: None,
            router: Router::new(ElementWidgetLookup),
        }
    }

    fn capture_pointer(&mut self, target: ElementId) {
        self.pointer_capture = Some(target);
        self.router.capture(Some(target));
    }

    fn release_pointer(&mut self) {
        self.pointer_capture = None;
        self.router.capture(None);
    }

    fn has_pointer_capture(&self) -> bool {
        self.pointer_capture.is_some()
    }
}

#[derive(Clone, Copy, Debug)]
struct ElementWidgetLookup;

impl WidgetLookup<ElementId> for ElementWidgetLookup {
    type WidgetId = ElementId;

    fn widget_of(&self, node: &ElementId) -> Option<Self::WidgetId> {
        Some(*node)
    }
}

pub(crate) struct TemplateValueResolver<'a> {
    ui: &'a Ui,
    element: ElementId,
    content: Option<TextContent>,
    state_stack: Vec<StyleFrame>,
    retained_subjects: Vec<RetainedStyleSubject>,
}

#[derive(Clone, Debug)]
struct StyleFrame {
    state: Option<MatchState>,
    path: Vec<PartTag>,
}

impl<'a> TemplateValueResolver<'a> {
    fn new(
        ui: &'a Ui,
        element: ElementId,
        content: Option<TextContent>,
        extra_pseudos: &[PseudoClassId],
    ) -> Self {
        let owner_state = if extra_pseudos.is_empty() {
            ui.element(element)
                .style_owner_state
                .or_else(|| ui.owner_style_state(element, extra_pseudos))
        } else {
            ui.owner_style_state(element, extra_pseudos)
        };
        Self {
            ui,
            element,
            content,
            state_stack: vec![StyleFrame {
                state: owner_state,
                path: Vec::new(),
            }],
            retained_subjects: Vec::new(),
        }
    }

    fn current_state(&self) -> Option<MatchState> {
        self.state_stack.last().and_then(|frame| frame.state)
    }

    fn resolve<T>(&self, property: Property<T>) -> T
    where
        T: Clone + 'static,
    {
        self.ui
            .resolve_with_state(self.element, self.current_state(), property)
    }

    fn text_style(&self) -> TextStyle {
        let properties = self.ui.properties;
        TextStyle::new(
            self.resolve::<f64>(properties.font_size),
            self.resolve::<alloc::boxed::Box<str>>(properties.font_family),
        )
    }

    pub(crate) fn into_retained_subjects(self) -> Vec<RetainedStyleSubject> {
        self.retained_subjects
    }
}

impl TemplateValueSource for TemplateValueResolver<'_> {
    fn enter_slot(&mut self, slot: TemplateSlot) {
        let parent_frame = self.state_stack.last().cloned().unwrap_or(StyleFrame {
            state: None,
            path: Vec::new(),
        });
        let parent_state = parent_frame.state;
        let mut path = parent_frame.path;
        let state = match (
            self.ui.element(self.element).style.as_ref(),
            parent_state,
            slot.part_tag(),
        ) {
            (Some(style), Some(parent_state), Some(part_tag)) => {
                let inputs = SelectorInputs::with_part(None, Some(part_tag), &[], &[]);
                let state = style.enter_subject(parent_state, &inputs);
                path.push(part_tag);
                self.retained_subjects.push(RetainedStyleSubject {
                    path: path.clone(),
                    state,
                });
                Some(state)
            }
            _ => parent_state,
        };
        self.state_stack.push(StyleFrame { state, path });
    }

    fn exit_slot(&mut self, _slot: TemplateSlot) {
        let _ = self.state_stack.pop();
    }

    fn apply(
        &mut self,
        node: &mut PresentationNode,
        _slot: TemplateSlot,
        binding: TemplateBinding,
    ) {
        if binding.target == crate::BACKGROUND_PROPERTY {
            if let Some(value) = self.resolve_brush(binding.source) {
                node.background = value;
            }
        } else if binding.target == crate::FOREGROUND_PROPERTY {
            if let Some(value) = self.resolve_brush(binding.source) {
                node.foreground = value;
            }
        } else if binding.target == crate::BORDER_PROPERTY {
            if let Some(value) = self.resolve_brush(binding.source) {
                node.border = value;
            }
        } else if binding.target == crate::BORDER_WIDTH_PROPERTY {
            if let Some(value) = self.resolve_f64(binding.source) {
                node.border_width = value;
            }
        } else if binding.target == crate::PADDING_PROPERTY {
            if binding.source == crate::PADDING_PROPERTY {
                node.padding = Some(self.resolve::<Insets>(self.ui.properties.padding));
            }
        } else if binding.target == crate::CORNER_RADIUS_PROPERTY {
            if let Some(value) = self.resolve_f64(binding.source) {
                node.corner_radius = value;
            }
        } else if binding.target == crate::CONTENT_PROPERTY
            && binding.source == crate::CONTENT_PROPERTY
        {
            self.apply_content(node);
        }
    }
}

impl TemplateValueResolver<'_> {
    fn resolve_brush(&self, source: crate::TemplateProperty) -> Option<Option<Brush>> {
        let properties = self.ui.properties;
        if source == crate::BACKGROUND_PROPERTY {
            Some(self.resolve::<Option<Brush>>(properties.background))
        } else if source == crate::FOREGROUND_PROPERTY {
            Some(self.resolve::<Option<Brush>>(properties.foreground))
        } else if source == crate::BORDER_PROPERTY {
            Some(self.resolve::<Option<Brush>>(properties.border))
        } else {
            None
        }
    }

    fn resolve_f64(&self, source: crate::TemplateProperty) -> Option<f64> {
        let properties = self.ui.properties;
        if source == crate::BORDER_WIDTH_PROPERTY {
            Some(self.resolve::<f64>(properties.border_width))
        } else if source == crate::CORNER_RADIUS_PROPERTY {
            Some(self.resolve::<f64>(properties.corner_radius))
        } else {
            None
        }
    }

    fn apply_content(&self, node: &mut PresentationNode) {
        node.text = self.content.clone();
        node.text_style = self.text_style();
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    /// Creates an empty semantic UI runtime.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = PropertyRegistry::new();
        let properties = UiProperties::register(&mut registry);
        let theme = ThemeBuilder::new().build();

        let mut invalidation = InvalidationTracker::new();
        invalidation
            .add_cascade(STYLE, TEMPLATE)
            .expect("style-to-template cascade should be acyclic");
        invalidation
            .add_cascade(TEMPLATE, MEASURE)
            .expect("template-to-measure cascade should be acyclic");
        invalidation
            .add_cascade(MEASURE, ARRANGE)
            .expect("measure-to-arrange cascade should be acyclic");
        invalidation
            .add_cascade(ARRANGE, VISUAL)
            .expect("arrange-to-visual cascade should be acyclic");

        let root = ElementId::from_raw(0);
        let mut ui = Self {
            elements: vec![Element::new(root, None, ElementKind::ROOT, None)],
            registry,
            properties,
            theme,
            invalidation,
            text: TextSystem::new(),
            hover: HoverState::new(),
            clicks: ClickState::new(),
            pressed: None,
            focused: None,
            input: InputState::default(),
            responder: ResponderState::new(),
            presentation: PresentationTree::new(),
            presentation_viewport: None,
            box_tree: BoxTree::new(),
            box_targets: Vec::new(),
            scene: record::Scene::new(),
            scene_scale_factor: 1.0,
            scene_valid: false,
        };
        ui.mark_channels(root, UiProperties::all_channels());
        ui
    }

    /// Returns the root element.
    #[must_use]
    pub const fn root(&self) -> ElementId {
        ElementId::from_raw(0)
    }

    /// Returns the registered built-in UI properties.
    #[must_use]
    pub const fn properties(&self) -> UiProperties {
        self.properties
    }

    /// Returns the property registry used for this UI.
    #[must_use]
    pub const fn registry(&self) -> &PropertyRegistry {
        &self.registry
    }

    /// Registers an application-defined dependency property for this UI.
    ///
    /// The returned typed property handle can be used with [`Ui::set_local`],
    /// [`Ui::resolve`], [`understory_style::StyleBuilder::set`], and
    /// [`crate::compose::WidgetSpec::set`].
    ///
    /// # Panics
    ///
    /// Panics if a property with `name` is already registered, or if the
    /// underlying property registry has reached its capacity.
    pub fn register_property<T>(
        &mut self,
        name: &'static str,
        metadata: PropertyMetadata<T>,
    ) -> Property<T>
    where
        T: Clone + 'static,
    {
        self.registry.register(name, metadata)
    }

    /// Adds a semantic button under `parent`.
    pub fn add_button(&mut self, parent: ElementId, content: impl Into<TextContent>) -> ElementId {
        self.append(parent, Button::new(content))
    }

    /// Adds a text block under `parent`.
    pub fn add_text_block(
        &mut self,
        parent: ElementId,
        content: impl Into<TextContent>,
    ) -> ElementId {
        self.append(parent, TextBlock::new(content))
    }

    /// Adds a panel under `parent`.
    pub fn add_panel(&mut self, parent: ElementId) -> ElementId {
        self.append(parent, Panel::new())
    }

    /// Adds a row under `parent`.
    pub fn add_row(&mut self, parent: ElementId) -> ElementId {
        self.append(parent, Row::new())
    }

    /// Adds a toggle under `parent`.
    pub fn add_toggle(&mut self, parent: ElementId, content: impl Into<TextContent>) -> ElementId {
        self.append(parent, Toggle::new(content))
    }

    /// Appends a configured widget spec under `parent`.
    pub fn append_spec(&mut self, parent: ElementId, spec: impl AppendSpec) -> ElementId {
        spec.append_to(self, parent)
    }

    /// Appends a widget under `parent`.
    pub fn append<W>(&mut self, parent: ElementId, widget: W) -> ElementId
    where
        W: Widget + 'static,
    {
        assert!(self.is_alive(parent), "parent element must be live");
        let id = ElementId::from_raw(
            u32::try_from(self.elements.len()).expect("element count should fit in u32"),
        );
        let kind = widget.kind();
        self.elements.push(Element::new(
            id,
            Some(parent),
            kind,
            Some(alloc::boxed::Box::new(widget)),
        ));
        self.elements[parent.index()].children.push(id);
        self.mark_channels(id, STYLE.into_set());
        id
    }

    /// Returns a typed widget reference for `id`, if the element hosts that widget type.
    #[must_use]
    pub fn widget<W>(&self, id: ElementId) -> Option<&W>
    where
        W: Widget + 'static,
    {
        self.elements
            .get(id.index())?
            .widget
            .as_deref()?
            .as_any()
            .downcast_ref()
    }

    /// Updates a typed widget and invalidates downstream presentation work.
    pub fn update_widget<W, R>(
        &mut self,
        id: ElementId,
        update: impl FnOnce(&mut W) -> R,
    ) -> Option<R>
    where
        W: Widget + 'static,
    {
        let result = {
            let widget = self
                .elements
                .get_mut(id.index())?
                .widget
                .as_deref_mut()?
                .as_any_mut()
                .downcast_mut()?;
            update(widget)
        };
        self.restyle_or_mark_style(id);
        self.mark_channels(id, MEASURE.into_set());
        Some(result)
    }

    /// Activates an element's widget, returning whether retained widget state changed.
    pub fn activate(&mut self, id: ElementId) -> bool {
        let changed = {
            let Some(widget) = self
                .elements
                .get_mut(id.index())
                .and_then(|element| element.widget.as_deref_mut())
            else {
                return false;
            };
            widget.activate()
        };
        if changed {
            self.restyle_or_mark_style(id);
            self.mark_channels(id, MEASURE.into_set());
        }
        changed
    }

    /// Returns the currently hovered element.
    #[must_use]
    pub fn hovered(&self) -> Option<ElementId> {
        self.hover.current_path().last().copied()
    }

    /// Returns the currently pressed element.
    #[must_use]
    pub const fn pressed(&self) -> Option<ElementId> {
        self.pressed
    }

    /// Returns the currently focused element.
    #[must_use]
    pub const fn focused(&self) -> Option<ElementId> {
        self.focused
    }

    /// Sets keyboard focus to `id`.
    ///
    /// Returns `true` when focus changed. Disabled elements and elements
    /// without widgets are ignored for now.
    pub fn focus(&mut self, id: ElementId) -> bool {
        if !self.element_focusable(id) || self.focused == Some(id) {
            return false;
        }
        self.focused = Some(id);
        self.responder.router.set_focus(Some(id));
        true
    }

    /// Returns retained frame-oriented input state.
    #[must_use]
    pub const fn input(&self) -> &InputState {
        &self.input
    }

    /// Clears per-frame input transitions.
    ///
    /// Call this once the host has completed its frame/update pass.
    pub fn clear_input_frame(&mut self) {
        self.input.clear_frame();
    }

    /// Applies one `ui-events` pointer event to retained UI interaction state.
    ///
    /// Returns `true` when the event changed retained state or activated a widget.
    pub fn pointer_event(&mut self, viewport: Size, event: &PointerEvent) -> bool {
        self.input
            .primary_pointer
            .process_pointer_event(event.clone());

        if !event.is_primary_pointer() {
            return false;
        }

        match event {
            PointerEvent::Move(PointerUpdate {
                pointer, current, ..
            }) => self.pointer_move(viewport, current.logical_point(), *pointer, event),
            PointerEvent::Down(PointerButtonEvent {
                button,
                pointer,
                state,
            }) if *button == Some(PointerButton::Primary) => {
                self.pointer_down(viewport, state.logical_point(), state.time, *pointer, event)
            }
            PointerEvent::Up(PointerButtonEvent {
                button,
                pointer,
                state,
            }) if *button == Some(PointerButton::Primary) => {
                self.pointer_up(viewport, state.logical_point(), state.time, *pointer, event)
            }
            PointerEvent::Leave(pointer) | PointerEvent::Cancel(pointer) => {
                self.pointer_cancel(*pointer, event)
            }
            PointerEvent::Down(_)
            | PointerEvent::Up(_)
            | PointerEvent::Enter(_)
            | PointerEvent::Scroll(_)
            | PointerEvent::Gesture(_) => false,
        }
    }

    /// Applies one `ui-events` keyboard event to the focused element.
    ///
    /// Returns `true` when the event changed retained state or activated a widget.
    pub fn keyboard_event(&mut self, event: &KeyboardEvent) -> bool {
        self.input.keyboard.process_keyboard_event(event.clone());

        let Some(focused) = self.focused.filter(|id| self.element_focusable(*id)) else {
            return false;
        };
        let path = self.semantic_path(focused);
        let router = Router::<ElementId, ElementWidgetLookup>::new(ElementWidgetLookup);
        let hit = ResolvedHitRef {
            node: focused,
            path: Some(&path),
            depth_key: DepthKey::Z(0),
            localizer: Localizer::new(),
            meta: (),
        };
        let dispatches = router.handle_with_hits(&[hit]);
        self.dispatch_keyboard_event(&dispatches, event)
    }

    /// Returns the topmost hit-testable element at `point`, rebuilding presentation if needed.
    pub fn hit_test(&mut self, viewport: Size, point: Point) -> Option<ElementId> {
        self.hit_target(viewport, point)
            .map(|target| target.element)
    }

    /// Returns `true` if `id` refers to a live element.
    #[must_use]
    pub fn is_alive(&self, id: ElementId) -> bool {
        id.index() < self.elements.len()
    }

    /// Returns the semantic kind for `id`.
    #[must_use]
    pub fn kind(&self, id: ElementId) -> Option<ElementKind> {
        self.elements.get(id.index()).map(|element| element.kind)
    }

    /// Returns the selector state for `id`.
    #[must_use]
    pub fn state(&self, id: ElementId) -> Option<ElementState> {
        self.elements.get(id.index()).map(|element| element.state)
    }

    /// Adds a selector class to an element.
    pub fn add_class(&mut self, id: ElementId, class: ClassId) {
        let element = self.element_mut(id);
        if element.classes.binary_search(&class).is_err() {
            element.classes.push(class);
            element.classes.sort();
            element.classes.dedup();
            self.restyle_or_mark_style(id);
        }
    }

    /// Adds an application-defined pseudoclass to an element.
    ///
    /// Pseudoclasses represent dynamic state such as selected, expanded, or
    /// invalid. For durable category labels, prefer [`Ui::add_class`].
    pub fn add_pseudo(&mut self, id: ElementId, pseudo: PseudoClassId) {
        self.set_pseudo(id, pseudo, true);
    }

    /// Removes an application-defined pseudoclass from an element.
    pub fn remove_pseudo(&mut self, id: ElementId, pseudo: PseudoClassId) {
        self.set_pseudo(id, pseudo, false);
    }

    /// Sets whether an application-defined pseudoclass is present on an element.
    pub fn set_pseudo(&mut self, id: ElementId, pseudo: PseudoClassId, active: bool) {
        let pseudos = &mut self.element_mut(id).custom_pseudos;
        let changed = match pseudos.binary_search(&pseudo) {
            Ok(index) => {
                if active {
                    false
                } else {
                    pseudos.remove(index);
                    true
                }
            }
            Err(index) => {
                if active {
                    pseudos.insert(index, pseudo);
                    true
                } else {
                    false
                }
            }
        };
        if changed {
            self.restyle_or_mark_style(id);
        }
    }

    /// Assigns a style cascade to an element.
    pub fn set_style(&mut self, id: ElementId, style: StyleCascade) {
        let element = self.element_mut(id);
        element.style = Some(style);
        element.style_owner_state = None;
        element.style_part_states.clear();
        self.mark_channels(id, STYLE.into_set());
    }

    /// Sets a local dependency property value.
    pub fn set_local<T>(&mut self, id: ElementId, property: Property<T>, value: T)
    where
        T: Clone + PartialEq + 'static,
    {
        let channels = {
            let registry = &self.registry;
            self.elements[id.index()].set_local_notifying(property, value, registry)
        };
        self.mark_channels(id, channels);
    }

    /// Clears a local dependency property value.
    pub fn clear_local<T>(&mut self, id: ElementId, property: Property<T>)
    where
        T: Clone + PartialEq + 'static,
    {
        let channels = {
            let registry = &self.registry;
            self.elements[id.index()].clear_local_notifying(property, registry)
        };
        self.mark_channels(id, channels);
    }

    /// Sets hover state for an element.
    pub fn set_hovered(&mut self, id: ElementId, hovered: bool) {
        if hovered {
            self.apply_hover_path(&[id]);
        } else if self.hovered() == Some(id) {
            self.clear_hover();
        } else {
            self.set_hovered_state(id, false);
        }
    }

    /// Sets pressed state for an element.
    pub fn set_pressed(&mut self, id: ElementId, pressed: bool) {
        if pressed {
            self.set_pressed_element(Some(id));
        } else if self.pressed == Some(id) {
            self.set_pressed_element(None);
        } else {
            self.set_pressed_state(id, false);
        }
    }

    /// Sets enabled state and the matching dependency property for an element.
    pub fn set_enabled(&mut self, id: ElementId, enabled: bool) {
        let state_changed = self.element_mut(id).state.set_enabled(enabled);
        let mut channels = {
            let registry = &self.registry;
            self.elements[id.index()].set_local_notifying(
                self.properties.enabled,
                enabled,
                registry,
            )
        };
        if state_changed {
            channels.remove(STYLE);
            self.mark_channels(id, channels);
            self.restyle_or_mark_style(id);
        } else {
            self.mark_channels(id, channels);
        }
    }

    /// Resolves a dependency property through local, style, inheritance, and default precedence.
    ///
    /// # Panics
    ///
    /// Panics if `id` is not live or `property` is not registered.
    #[must_use]
    pub fn resolve<T>(&self, id: ElementId, property: Property<T>) -> T
    where
        T: Clone + 'static,
    {
        self.resolve_with_part(id, None, property)
    }

    /// Resolves a dependency property for a template slot part.
    ///
    /// Slot-part style rules can override unscoped rules while sharing
    /// the same element type, classes, and pseudoclasses.
    ///
    /// # Panics
    ///
    /// Panics if `id` is not live or `property` is not registered.
    #[must_use]
    pub fn resolve_slot<T>(&self, id: ElementId, slot: TemplateSlot, property: Property<T>) -> T
    where
        T: Clone + 'static,
    {
        self.resolve_with_part(id, slot.part_tag(), property)
    }

    /// Returns inspector-facing style information for one element subject and property.
    ///
    /// This reports only Style-layer rules and direct style sources. Local
    /// dependency-property values, inheritance, theme fallback, and defaults are
    /// still applied by [`Ui::resolve`], but they are outside the style cascade
    /// and therefore do not appear as a winning style source here.
    #[must_use]
    pub fn inspect_style<T>(
        &self,
        id: ElementId,
        subject: StyleSubject,
        property: Property<T>,
    ) -> Option<StyleInspection>
    where
        T: Clone + 'static,
    {
        let element = self.element(id);
        let style = element.style.as_ref()?;
        let state = self.style_state_for_subject(id, &subject)?;
        let matching_rules = style
            .matching_rules(state)
            .map(StyleRuleInspection::from_rule)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let winning_source =
            style
                .winning_source(state, property)
                .map(|source| match source.rule() {
                    Some(rule) => StyleSourceInspection::Rule(StyleRuleInspection::from_rule(rule)),
                    None => StyleSourceInspection::Direct {
                        origin: source.origin(),
                        source_index: source.source_index(),
                    },
                });

        Some(StyleInspection {
            subject,
            property: property.id(),
            property_name: self.registry.name(property.id()),
            matching_rules,
            winning_source,
        })
    }

    fn resolve_with_part<T>(
        &self,
        id: ElementId,
        part_tag: Option<PartTag>,
        property: Property<T>,
    ) -> T
    where
        T: Clone + 'static,
    {
        self.resolve_with_part_and_pseudos(id, part_tag, &[], property)
    }

    fn resolve_with_part_and_pseudos<T>(
        &self,
        id: ElementId,
        part_tag: Option<PartTag>,
        extra_pseudos: &[PseudoClassId],
        property: Property<T>,
    ) -> T
    where
        T: Clone + 'static,
    {
        let state = self
            .owner_style_state(id, extra_pseudos)
            .map(|owner_state| {
                let element = self.element(id);
                match (element.style.as_ref(), part_tag) {
                    (Some(style), Some(part_tag)) => {
                        let inputs = SelectorInputs::with_part(None, Some(part_tag), &[], &[]);
                        style.enter_subject(owner_state, &inputs)
                    }
                    _ => owner_state,
                }
            });
        self.resolve_with_state(id, state, property)
    }

    fn owner_style_state(
        &self,
        id: ElementId,
        extra_pseudos: &[PseudoClassId],
    ) -> Option<MatchState> {
        let element = self.element(id);
        let style = element.style.as_ref()?;
        let owner_inputs = self.owner_selector_inputs(id, extra_pseudos);
        Some(style.enter_subject(style.root_state(), &owner_inputs.as_inputs()))
    }

    fn style_state_for_subject(&self, id: ElementId, subject: &StyleSubject) -> Option<MatchState> {
        let element = self.element(id);
        let style = element.style.as_ref()?;
        let mut state = element
            .style_owner_state
            .or_else(|| self.owner_style_state(id, &[]))?;
        match subject {
            StyleSubject::Owner => Some(state),
            StyleSubject::PartPath(path) => {
                for part_tag in path.iter().copied() {
                    let inputs = SelectorInputs::with_part(None, Some(part_tag), &[], &[]);
                    state = style.enter_subject(state, &inputs);
                }
                Some(state)
            }
        }
    }

    fn owner_selector_inputs(
        &self,
        id: ElementId,
        extra_pseudos: &[PseudoClassId],
    ) -> SelectorInputsOwned {
        let element = self.element(id);
        let pseudos = self.selector_pseudos(element, extra_pseudos);
        SelectorInputsOwned::new(
            Some(element.kind.type_tag()),
            element.classes.iter().copied(),
            pseudos,
        )
    }

    fn refresh_retained_owner_style_state(&mut self, id: ElementId) {
        let state = self.owner_style_state(id, &[]);
        let element = self.element_mut(id);
        element.style_owner_state = state;
        element.style_part_states.clear();
    }

    fn restyle_or_mark_style(&mut self, id: ElementId) {
        if self.element(id).style.is_some() && !self.restyle_element(id) {
            self.mark_channels(id, STYLE.into_set());
        }
    }

    fn restyle_element(&mut self, id: ElementId) -> bool {
        let Some(style) = self.element(id).style.clone() else {
            return false;
        };
        let Some(old_owner_state) = self.element(id).style_owner_state else {
            return false;
        };

        let owner_inputs = self.owner_selector_inputs(id, &[]);
        let owner_restyle = style.restyle_subject(
            &self.registry,
            old_owner_state,
            style.root_state(),
            &owner_inputs.as_inputs(),
        );
        let new_owner_state = owner_restyle.state();
        let mut changed_channels = owner_restyle.changed_channels();

        let old_part_states = self.element(id).style_part_states.clone();
        let new_part_states = old_part_states
            .into_iter()
            .map(|old_subject| {
                let (state, channels) =
                    self.restyle_part_subject(&style, new_owner_state, &old_subject);
                changed_channels |= channels;
                RetainedStyleSubject {
                    path: old_subject.path,
                    state,
                }
            })
            .collect::<Vec<_>>();

        let element = self.element_mut(id);
        element.style_owner_state = Some(new_owner_state);
        element.style_part_states = new_part_states;

        if !changed_channels.is_empty() {
            self.mark_channels(id, changed_channels);
        }
        true
    }

    fn restyle_part_subject(
        &self,
        style: &StyleCascade,
        owner_state: MatchState,
        old_subject: &RetainedStyleSubject,
    ) -> (MatchState, ChannelSet) {
        let Some((&subject_part, parent_path)) = old_subject.path.split_last() else {
            return (owner_state, ChannelSet::empty());
        };
        let mut parent_state = owner_state;
        for part_tag in parent_path {
            let inputs = SelectorInputs::with_part(None, Some(*part_tag), &[], &[]);
            parent_state = style.enter_subject(parent_state, &inputs);
        }

        let inputs = SelectorInputs::with_part(None, Some(subject_part), &[], &[]);
        let restyle =
            style.restyle_subject(&self.registry, old_subject.state, parent_state, &inputs);
        (restyle.state(), restyle.changed_channels())
    }

    fn resolve_with_state<T>(
        &self,
        id: ElementId,
        state: Option<MatchState>,
        property: Property<T>,
    ) -> T
    where
        T: Clone + 'static,
    {
        let element = self.element(id);
        let style = element.style.as_ref().map(|style| {
            let state = state.unwrap_or_else(|| style.root_state());
            (style, state)
        });
        let cx = ResolveCx::new(&self.registry, &self.theme, |key: ElementId| {
            self.elements
                .get(key.index())
                .map(|element| (element.property_store(), element.parent))
        });

        cx.get_value(element, property, style)
    }

    /// Returns whether `id` is invalidated in `channel`.
    #[must_use]
    pub fn is_invalidated(&self, id: ElementId, channel: invalidation::Channel) -> bool {
        self.invalidation.is_invalidated(id, channel)
    }

    /// Returns the current presentation tree, rebuilding template, measure, or arrange work if needed.
    pub fn presentation(&mut self, viewport: Size) -> &PresentationTree {
        if self.presentation_viewport != Some(viewport)
            || self.presentation.root().is_none()
            || self.invalidation.has_invalidated(STYLE)
            || self.invalidation.has_invalidated(TEMPLATE)
            || self.invalidation.has_invalidated(MEASURE)
            || self.invalidation.has_invalidated(ARRANGE)
            || self.invalidation.has_invalidated(VISUAL)
        {
            self.presentation = self.build_presentation(viewport);
            self.presentation_viewport = Some(viewport);
            self.rebuild_box_tree();
            self.drain_channel(STYLE);
            self.drain_channel(TEMPLATE);
            self.drain_channel(MEASURE);
            self.drain_channel(ARRANGE);
            self.drain_channel(VISUAL);
        }

        &self.presentation
    }

    /// Returns a cached imaging scene for the current UI state and viewport.
    pub fn paint_scene(&mut self, viewport: Size) -> &record::Scene {
        self.paint_scene_with_scale(viewport, 1.0)
    }

    /// Returns a cached imaging scene scaled from logical UI units to physical pixels.
    pub fn paint_scene_with_scale(&mut self, viewport: Size, scale_factor: f64) -> &record::Scene {
        let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        let viewport_changed = self.presentation_viewport != Some(viewport);
        self.presentation(viewport);

        if viewport_changed
            || !self.scene_valid
            || self.scene_scale_factor != scale_factor
            || self.invalidation.has_invalidated(VISUAL)
        {
            self.scene = crate::lower_presentation_with_scale(
                &self.presentation,
                &mut self.text,
                scale_factor,
            );
            self.scene_scale_factor = scale_factor;
            self.scene_valid = true;
            self.drain_channel(VISUAL);
        }

        &self.scene
    }

    fn element(&self, id: ElementId) -> &Element {
        self.elements
            .get(id.index())
            .expect("element id should be live")
    }

    fn element_mut(&mut self, id: ElementId) -> &mut Element {
        self.elements
            .get_mut(id.index())
            .expect("element id should be live")
    }

    fn mark_channels(&mut self, id: ElementId, channels: ChannelSet) {
        for channel in channels {
            self.invalidation.mark_with(id, channel, &EagerPolicy);
        }
        if channels.contains(VISUAL) || self.invalidation.has_invalidated(VISUAL) {
            self.scene_valid = false;
        }
    }

    fn drain_channel(&mut self, channel: invalidation::Channel) {
        let _ = self
            .invalidation
            .drain(channel)
            .deterministic()
            .run()
            .count();
    }

    fn build_presentation(&mut self, viewport: Size) -> PresentationTree {
        let mut tree = PresentationTree::new();
        let root = self.root();
        let properties = self.properties;
        let root_background = self.resolve::<Option<Brush>>(root, properties.background);
        let root_padding = self.resolve::<Insets>(root, properties.padding);
        let mut root_node = PresentationNode::new(
            root,
            ROOT_PART,
            Rect::from_origin_size((0.0, 0.0), viewport),
        );
        root_node.background = root_background;
        let root_presentation = tree.push(root_node);

        self.push_root_children(&mut tree, root_presentation, root_padding, viewport);

        tree
    }

    pub(crate) fn measure_text(
        &mut self,
        content: &TextContent,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Size {
        self.text.measure_with_style(content, style, max_width)
    }

    pub(crate) fn resolve_text_style(&self, element: ElementId) -> TextStyle {
        let properties = self.properties;
        TextStyle::new(
            self.resolve::<f64>(element, properties.font_size),
            self.resolve::<alloc::boxed::Box<str>>(element, properties.font_family),
        )
    }

    pub(crate) fn template_values(
        &self,
        element: ElementId,
        content: Option<TextContent>,
    ) -> TemplateValueResolver<'_> {
        self.template_values_with_pseudos(element, content, &[])
    }

    pub(crate) fn template_values_with_pseudos(
        &self,
        element: ElementId,
        content: Option<TextContent>,
        extra_pseudos: &[PseudoClassId],
    ) -> TemplateValueResolver<'_> {
        TemplateValueResolver::new(self, element, content, extra_pseudos)
    }

    pub(crate) fn children(&self, parent: ElementId) -> Vec<ElementId> {
        self.element(parent).children.clone()
    }

    pub(crate) fn measure_child(&mut self, element: ElementId, available: Size) -> Size {
        self.measure_widget(element, available)
    }

    pub(crate) fn present_child(
        &mut self,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId {
        self.push_widget_presentation(tree, parent, element, bounds)
    }

    fn push_root_children(
        &mut self,
        tree: &mut PresentationTree,
        root_presentation: PresentationNodeId,
        root_padding: Insets,
        viewport: Size,
    ) {
        let properties = self.properties;
        let spacing = self.resolve::<f64>(self.root(), properties.spacing);
        let width = (viewport.width - root_padding.x_value()).max(0.0);
        let mut cursor_y = root_padding.y0;
        for (count, child) in self.children(self.root()).into_iter().enumerate() {
            if count > 0 {
                cursor_y += spacing;
            }
            let size = self.measure_child(child, Size::new(width, f64::INFINITY));
            let bounds =
                Rect::from_origin_size((root_padding.x0, cursor_y), (size.width, size.height));
            self.present_child(tree, root_presentation, child, bounds);
            cursor_y += size.height;
        }
    }

    fn rebuild_box_tree(&mut self) {
        self.box_tree = BoxTree::new();
        self.box_targets.clear();
        if let Some(root) = self.presentation.root() {
            self.push_presentation_box(None, root);
            let _ = self.box_tree.commit();
        }
    }

    fn push_presentation_box(
        &mut self,
        parent: Option<BoxNodeId>,
        presentation_id: PresentationNodeId,
    ) -> BoxNodeId {
        let node = self
            .presentation
            .node(presentation_id)
            .expect("presentation ids should be live");
        let source = node.source;
        let bounds = node.bounds;
        let children = node.children.clone();
        let flags = if self.element_hit_testable(source) {
            NodeFlags::VISIBLE | NodeFlags::PICKABLE
        } else {
            NodeFlags::VISIBLE
        };
        let box_node = self.box_tree.insert(
            parent,
            LocalNode {
                local_bounds: bounds,
                z_index: i32::try_from(presentation_id.raw()).unwrap_or(i32::MAX),
                flags,
                ..LocalNode::default()
            },
        );
        self.box_targets.push(BoxTarget {
            box_node,
            element: source,
        });
        for child in children {
            self.push_presentation_box(Some(box_node), child);
        }
        box_node
    }

    fn measure_widget(&mut self, element: ElementId, available: Size) -> Size {
        let widget = self.take_widget(element);
        let size = widget.measure(self, element, available);
        self.put_widget(element, widget);
        size
    }

    fn push_widget_presentation(
        &mut self,
        tree: &mut PresentationTree,
        parent: PresentationNodeId,
        element: ElementId,
        bounds: Rect,
    ) -> PresentationNodeId {
        self.refresh_retained_owner_style_state(element);
        let widget = self.take_widget(element);
        let id = widget.present(self, tree, parent, element, bounds);
        self.put_widget(element, widget);
        id
    }

    pub(crate) fn retain_style_subjects(
        &mut self,
        element: ElementId,
        subjects: Vec<RetainedStyleSubject>,
    ) {
        self.element_mut(element).style_part_states = subjects;
    }

    fn take_widget(&mut self, element: ElementId) -> alloc::boxed::Box<dyn Widget> {
        self.elements[element.index()]
            .widget
            .take()
            .expect("non-root elements should have widgets")
    }

    fn put_widget(&mut self, element: ElementId, widget: alloc::boxed::Box<dyn Widget>) {
        self.elements[element.index()].widget = Some(widget);
    }

    fn widget_hit_testable(&self, element: ElementId) -> bool {
        self.elements
            .get(element.index())
            .and_then(|element| element.widget.as_deref())
            .is_some_and(Widget::hit_testable)
    }

    fn element_hit_testable(&self, element: ElementId) -> bool {
        self.state(element).is_some_and(ElementState::enabled) && self.widget_hit_testable(element)
    }

    fn element_focusable(&self, element: ElementId) -> bool {
        self.state(element).is_some_and(ElementState::enabled)
            && self
                .elements
                .get(element.index())
                .is_some_and(|element| element.widget.is_some())
    }

    fn pointer_move(
        &mut self,
        viewport: Size,
        point: Point,
        pointer: PointerInfo,
        event: &PointerEvent,
    ) -> bool {
        let hit = self.hit_target(viewport, point);
        let route = self.pointer_route_from_hit(hit.as_ref());
        let hover_route = self.hover_route_from_hit(hit.as_ref());
        let mut changed = self.apply_hover_route(&hover_route);
        self.clicks.on_move(pointer_id(pointer), point);
        if hit.is_none() && !self.responder.has_pointer_capture() {
            self.clicks.cancel(pointer_id(pointer));
            changed |= self.set_pressed_element(None);
        }
        changed |= self.dispatch_pointer_event(&route.dispatches, event, None);
        changed
    }

    fn pointer_down(
        &mut self,
        viewport: Size,
        point: Point,
        timestamp: u64,
        pointer: PointerInfo,
        event: &PointerEvent,
    ) -> bool {
        let hit = self.hit_target(viewport, point);
        let route = self.pointer_route_from_hit(hit.as_ref());
        let hover_route = self.hover_route_from_hit(hit.as_ref());
        let mut changed = self.apply_hover_route(&hover_route);
        if let Some(target) = hit.as_ref().map(|target| target.element) {
            changed |= self.focus(target);
            self.clicks.on_down(
                pointer_id(pointer),
                Some(primary_button()),
                target,
                point,
                timestamp_ms(timestamp),
            );
            self.responder.capture_pointer(target);
            changed |= self.set_pressed_element(Some(target));
        } else {
            self.clicks.cancel(pointer_id(pointer));
            changed |= self.set_pressed_element(None);
        }
        changed |= self.dispatch_pointer_event(&route.dispatches, event, None);
        changed
    }

    fn pointer_up(
        &mut self,
        viewport: Size,
        point: Point,
        timestamp: u64,
        pointer: PointerInfo,
        event: &PointerEvent,
    ) -> bool {
        let hit = self.hit_target(viewport, point);
        let route = self.pointer_route_from_hit(hit.as_ref());
        let hover_route = self.hover_route_from_hit(hit.as_ref());
        let mut changed = self.apply_hover_route(&hover_route);
        let click = if let Some(target) = hit.as_ref().map(|target| target.element) {
            self.clicks.on_up(
                pointer_id(pointer),
                Some(primary_button()),
                &target,
                point,
                timestamp_ms(timestamp),
            )
        } else {
            self.clicks.cancel(pointer_id(pointer));
            ClickResult::Suppressed(self.pressed)
        };
        changed |= self.set_pressed_element(None);
        let clicked = match click {
            ClickResult::Click(id) => Some(id),
            ClickResult::Suppressed(_) => None,
        };
        changed |= self.dispatch_pointer_event(&route.dispatches, event, clicked);
        self.responder.release_pointer();
        changed
    }

    fn pointer_cancel(&mut self, pointer: PointerInfo, event: &PointerEvent) -> bool {
        let route = self.pointer_route_from_hit(None);
        self.clicks.cancel(pointer_id(pointer));
        let mut changed = self.clear_hover();
        changed |= self.set_pressed_element(None);
        changed |= self.dispatch_pointer_event(&route.dispatches, event, None);
        self.responder.release_pointer();
        changed
    }

    #[cfg(test)]
    fn pointer_route(&mut self, viewport: Size, point: Point) -> PointerRoute {
        let hit = self.hit_target(viewport, point);
        self.pointer_route_from_hit(hit.as_ref())
    }

    fn pointer_route_from_hit(&self, hit: Option<&HitTarget>) -> PointerRoute {
        self.route_hit(&self.responder.router, hit)
    }

    fn hover_route_from_hit(&self, hit: Option<&HitTarget>) -> PointerRoute {
        let router = Router::<ElementId, ElementWidgetLookup>::new(ElementWidgetLookup);
        self.route_hit(&router, hit)
    }

    fn route_hit(&self, router: &ElementRouter, target: Option<&HitTarget>) -> PointerRoute {
        let dispatches = if let Some(target) = target {
            let hit = ResolvedHitRef {
                node: target.element,
                path: Some(&target.path),
                depth_key: DepthKey::Z(0),
                localizer: Localizer::new(),
                meta: (),
            };
            router.handle_with_hits(&[hit])
        } else {
            let hits: [ResolvedHitRef<'_, ElementId, ()>; 0] = [];
            router.handle_with_hits(&hits)
        };
        let target = dispatches
            .iter()
            .find(|dispatch| matches!(dispatch.phase, Phase::Target))
            .map(|dispatch| dispatch.node);
        PointerRoute { target, dispatches }
    }

    fn semantic_path(&self, target: ElementId) -> Vec<ElementId> {
        let mut path = Vec::new();
        let mut current = Some(target);
        while let Some(id) = current {
            path.push(id);
            current = self.elements[id.index()].parent;
        }
        path.reverse();
        path
    }

    fn apply_hover_route(&mut self, route: &PointerRoute) -> bool {
        if route.target.is_some() {
            let path = path_from_dispatch(&route.dispatches);
            self.apply_hover_path(&path)
        } else {
            self.clear_hover()
        }
    }

    fn dispatch_pointer_event(
        &mut self,
        dispatches: &[ResponderDispatch],
        event: &PointerEvent,
        clicked: Option<ElementId>,
    ) -> bool {
        let input = core::mem::take(&mut self.input);
        let mut changed = false;
        dispatcher::run(dispatches, &mut changed, |dispatch, changed| {
            if self.elements[dispatch.node.index()].widget.is_none() {
                return Outcome::Continue;
            }

            let mut cx = PointerEventCx::new(
                dispatch.node,
                dispatch.phase,
                clicked == Some(dispatch.node) && matches!(dispatch.phase, Phase::Target),
                &input,
            );
            let mut widget = self.take_widget(dispatch.node);
            let outcome = widget.pointer_event(&mut cx, event);
            self.put_widget(dispatch.node, widget);
            if cx.changed() {
                self.restyle_or_mark_style(dispatch.node);
                self.mark_channels(dispatch.node, MEASURE.into_set());
                *changed = true;
            }
            if cx.activate_requested() {
                *changed |= self.activate(dispatch.node);
            }
            outcome
        });
        self.input = input;
        changed
    }

    fn dispatch_keyboard_event(
        &mut self,
        dispatches: &[ResponderDispatch],
        event: &KeyboardEvent,
    ) -> bool {
        let input = core::mem::take(&mut self.input);
        let mut changed = false;
        dispatcher::run(dispatches, &mut changed, |dispatch, changed| {
            if self.elements[dispatch.node.index()].widget.is_none() {
                return Outcome::Continue;
            }

            let mut cx = KeyboardEventCx::new(dispatch.node, dispatch.phase, &input);
            let mut widget = self.take_widget(dispatch.node);
            let outcome = widget.keyboard_event(&mut cx, event);
            self.put_widget(dispatch.node, widget);
            if cx.changed() {
                self.restyle_or_mark_style(dispatch.node);
                self.mark_channels(dispatch.node, MEASURE.into_set());
                *changed = true;
            }
            if cx.activate_requested() {
                *changed |= self.activate(dispatch.node);
            }
            outcome
        });
        self.input = input;
        changed
    }

    fn apply_hover_path(&mut self, path: &[ElementId]) -> bool {
        let events = self.hover.update_path(path);
        self.apply_hover_events(events)
    }

    fn clear_hover(&mut self) -> bool {
        let events = self.hover.clear();
        self.apply_hover_events(events)
    }

    fn apply_hover_events(&mut self, events: Vec<HoverEvent<ElementId>>) -> bool {
        let changed = !events.is_empty();
        for event in events {
            match event {
                HoverEvent::Enter(id) => self.set_hovered_state(id, true),
                HoverEvent::Leave(id) => self.set_hovered_state(id, false),
            }
        }
        changed
    }

    fn hit_target(&mut self, viewport: Size, point: Point) -> Option<HitTarget> {
        self.presentation(viewport);
        let hit = self
            .box_tree
            .hit_test_point(point, QueryFilter::new().visible().pickable())?;
        let element = self.element_for_box_node(hit.node)?;
        let mut path = Vec::new();
        for box_node in hit.path {
            if let Some(element) = self.element_for_box_node(box_node)
                && path.last() != Some(&element)
            {
                path.push(element);
            }
        }
        if path.last() != Some(&element) {
            path.push(element);
        }
        Some(HitTarget { element, path })
    }

    fn element_for_box_node(&self, box_node: BoxNodeId) -> Option<ElementId> {
        self.box_targets
            .iter()
            .find(|target| target.box_node == box_node)
            .map(|target| target.element)
    }

    fn set_pressed_element(&mut self, pressed: Option<ElementId>) -> bool {
        if self.pressed == pressed {
            return false;
        }
        if let Some(previous) = self.pressed {
            self.set_pressed_state(previous, false);
        }
        self.pressed = pressed;
        if let Some(current) = pressed {
            self.set_pressed_state(current, true);
        }
        true
    }

    fn set_hovered_state(&mut self, id: ElementId, hovered: bool) {
        if self.element_mut(id).state.set_hovered(hovered) {
            self.restyle_or_mark_style(id);
        }
    }

    fn set_pressed_state(&mut self, id: ElementId, pressed: bool) {
        if self.element_mut(id).state.set_pressed(pressed) {
            self.restyle_or_mark_style(id);
        }
    }

    fn selector_pseudos(
        &self,
        element: &Element,
        extra_pseudos: &[PseudoClassId],
    ) -> Vec<PseudoClassId> {
        let mut pseudos = element.state.pseudos();
        pseudos.extend_from_slice(&element.custom_pseudos);
        if let Some(widget) = element.widget.as_deref() {
            widget.append_selector_pseudos(&mut pseudos);
        }
        pseudos.extend_from_slice(extra_pseudos);
        pseudos.sort();
        pseudos.dedup();
        pseudos
    }
}

fn pointer_id(pointer: PointerInfo) -> Option<understory_event_state::click::PointerId> {
    pointer
        .pointer_id
        .map(ui_events::pointer::PointerId::get_inner)
}

fn primary_button() -> understory_event_state::click::Button {
    1
}

fn timestamp_ms(timestamp_ns: u64) -> u64 {
    timestamp_ns / 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::rc::Rc;
    use core::cell::RefCell;
    use ui_events::keyboard::{Code, Key, KeyboardEvent, NamedKey};
    use ui_events::pointer::{PointerId, PointerInfo, PointerState, PointerType};

    #[test]
    fn semantic_marks_cascade_to_visual() {
        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");

        assert!(ui.is_invalidated(id, STYLE));
        assert!(ui.is_invalidated(id, TEMPLATE));
        assert!(ui.is_invalidated(id, MEASURE));
        assert!(ui.is_invalidated(id, ARRANGE));
        assert!(ui.is_invalidated(id, VISUAL));
    }

    #[test]
    fn semantic_button_uses_open_widget_kind() {
        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");

        assert_eq!(ui.kind(id), Some(ElementKind::BUTTON));
    }

    #[test]
    fn button_template_is_measured_and_arranged() {
        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        ui.set_local(id, ui.properties().padding, Insets::new(4.0, 2.0, 6.0, 8.0));

        let tree = ui.presentation(Size::new(200.0, 100.0));
        assert_eq!(tree.nodes().len(), 4);

        let button = &tree.nodes()[1];
        assert_eq!(button.kind, crate::BUTTON_PART);
        assert_eq!(button.bounds.x0, 0.0);
        assert_eq!(button.bounds.y0, 0.0);
        assert!(button.bounds.width() > 10.0);
        assert!(button.bounds.height() > 10.0);

        let content = &tree.nodes()[3];
        assert_eq!(content.kind, crate::CONTENT_PRESENTER_PART);
        assert_eq!(content.bounds.x0, 4.0);
        assert_eq!(content.bounds.y0, 2.0);
        assert!(content.bounds.width() > 0.0);
        assert!(content.bounds.height() > 0.0);
    }

    #[derive(Debug)]
    struct BadgeWidget;

    impl Widget for BadgeWidget {
        fn kind(&self) -> ElementKind {
            ElementKind::new(understory_style::TypeTag(900), "test-badge")
        }

        fn measure(&self, _ui: &mut Ui, _element: ElementId, _available: Size) -> Size {
            Size::new(30.0, 12.0)
        }

        fn present(
            &self,
            _ui: &mut Ui,
            tree: &mut PresentationTree,
            parent: PresentationNodeId,
            element: ElementId,
            bounds: Rect,
        ) -> PresentationNodeId {
            const BADGE_PART: crate::PartKind = crate::PartKind::new("test-badge");
            tree.push_child(parent, PresentationNode::new(element, BADGE_PART, bounds))
        }

        fn as_any(&self) -> &dyn core::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
            self
        }
    }

    #[derive(Debug)]
    struct ActivatingWidget {
        activations: Rc<RefCell<usize>>,
    }

    impl Widget for ActivatingWidget {
        fn kind(&self) -> ElementKind {
            ElementKind::new(understory_style::TypeTag(901), "test-activating")
        }

        fn measure(&self, _ui: &mut Ui, _element: ElementId, _available: Size) -> Size {
            Size::new(24.0, 24.0)
        }

        fn present(
            &self,
            _ui: &mut Ui,
            tree: &mut PresentationTree,
            parent: PresentationNodeId,
            element: ElementId,
            bounds: Rect,
        ) -> PresentationNodeId {
            const ACTIVATING_PART: crate::PartKind = crate::PartKind::new("test-activating");
            tree.push_child(
                parent,
                PresentationNode::new(element, ACTIVATING_PART, bounds),
            )
        }

        fn hit_testable(&self) -> bool {
            true
        }

        fn activate(&mut self) -> bool {
            *self.activations.borrow_mut() += 1;
            true
        }

        fn pointer_event(&mut self, cx: &mut PointerEventCx<'_>, _event: &PointerEvent) -> Outcome {
            if cx.is_target() && cx.clicked() {
                assert!(cx.input().primary_pointer.is_primary_just_released());
                cx.activate();
            }
            Outcome::Continue
        }

        fn as_any(&self) -> &dyn core::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
            self
        }
    }

    #[derive(Debug)]
    struct KeyboardMutatingWidget {
        width: f64,
    }

    impl Widget for KeyboardMutatingWidget {
        fn kind(&self) -> ElementKind {
            ElementKind::new(understory_style::TypeTag(902), "test-keyboard-mutating")
        }

        fn measure(&self, _ui: &mut Ui, _element: ElementId, _available: Size) -> Size {
            Size::new(self.width, 12.0)
        }

        fn present(
            &self,
            _ui: &mut Ui,
            tree: &mut PresentationTree,
            parent: PresentationNodeId,
            element: ElementId,
            bounds: Rect,
        ) -> PresentationNodeId {
            const MUTATING_PART: crate::PartKind = crate::PartKind::new("test-keyboard-mutating");
            tree.push_child(
                parent,
                PresentationNode::new(element, MUTATING_PART, bounds),
            )
        }

        fn keyboard_event(
            &mut self,
            cx: &mut KeyboardEventCx<'_>,
            event: &KeyboardEvent,
        ) -> Outcome {
            if cx.is_target()
                && event.state == ui_events::keyboard::KeyState::Down
                && matches!(&event.key, Key::Character(value) if value == "w")
            {
                self.width += 10.0;
                cx.mark_changed();
            }
            Outcome::Continue
        }

        fn as_any(&self) -> &dyn core::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
            self
        }
    }

    #[test]
    fn custom_widget_kind_measures_and_presents_without_core_enum_case() {
        let mut ui = Ui::new();
        let id = ui.append(ui.root(), BadgeWidget);

        let has_badge_node = ui
            .presentation(Size::new(100.0, 100.0))
            .nodes()
            .iter()
            .any(|node| {
                node.source == id
                    && node.kind == crate::PartKind::new("test-badge")
                    && node.bounds.width() == 30.0
                    && node.bounds.height() == 12.0
            });

        assert_eq!(
            ui.kind(id),
            Some(ElementKind::new(
                understory_style::TypeTag(900),
                "test-badge"
            ))
        );
        assert!(has_badge_node);
        assert_eq!(
            ui.hit_test(Size::new(100.0, 100.0), Point::new(1.0, 1.0)),
            None
        );
    }

    #[test]
    fn widget_pointer_event_can_request_activation() {
        let mut ui = Ui::new();
        let viewport = Size::new(100.0, 100.0);
        let activations = Rc::new(RefCell::new(0));
        let id = ui.append(
            ui.root(),
            ActivatingWidget {
                activations: activations.clone(),
            },
        );
        let bounds = ui
            .presentation(viewport)
            .nodes()
            .iter()
            .find(|node| node.source == id)
            .map(|node| node.bounds)
            .expect("activating widget should be presented");
        let point = bounds.center();

        assert!(ui.pointer_event(viewport, &pointer_down(point)));
        assert!(ui.pointer_event(viewport, &pointer_up(point)));

        assert_eq!(*activations.borrow(), 1);
    }

    #[test]
    fn widget_keyboard_event_can_mark_retained_state_changed() {
        let mut ui = Ui::new();
        let viewport = Size::new(100.0, 100.0);
        let id = ui.append(ui.root(), KeyboardMutatingWidget { width: 20.0 });

        let initial_width = ui.presentation(viewport).nodes()[1].bounds.width();
        assert_eq!(initial_width, 20.0);
        assert!(ui.focus(id));
        assert!(ui.keyboard_event(&KeyboardEvent::key_down(
            Key::Character("w".into()),
            Code::KeyW,
        )));

        let updated_width = ui.presentation(viewport).nodes()[1].bounds.width();
        assert_eq!(updated_width, 30.0);
    }

    #[test]
    fn text_block_uses_text_template_and_text_style() {
        let mut ui = Ui::new();
        let id = ui.add_text_block(ui.root(), "A short label");
        ui.set_local(id, ui.properties().font_size, 22.0);

        let (text_matches, font_size) = ui
            .presentation(Size::new(200.0, 100.0))
            .nodes()
            .iter()
            .find(|node| node.kind == crate::CONTENT_PRESENTER_PART)
            .map(|node| {
                (
                    node.text
                        .as_ref()
                        .is_some_and(|text| text.as_str() == "A short label"),
                    node.text_style.font_size(),
                )
            })
            .expect("text block should emit a content presenter");

        assert_eq!(ui.kind(id), Some(ElementKind::TEXT_BLOCK));
        assert!(text_matches);
        assert_eq!(font_size, 22.0);
    }

    #[test]
    fn typed_widget_update_invalidates_measure_pipeline() {
        let mut ui = Ui::new();
        let id = ui.add_text_block(ui.root(), "Short");
        let initial_width = ui.presentation(Size::new(400.0, 100.0)).nodes()[1]
            .bounds
            .width();

        ui.update_widget::<TextBlock, _>(id, |widget| {
            widget.set_content("A much longer block of text");
        })
        .expect("element should host a text block");

        assert!(ui.is_invalidated(id, MEASURE));
        assert!(ui.is_invalidated(id, ARRANGE));
        assert!(ui.is_invalidated(id, VISUAL));
        assert_eq!(
            ui.widget::<TextBlock>(id)
                .map(TextBlock::content)
                .map(TextContent::as_str),
            Some("A much longer block of text")
        );

        let updated_width = ui.presentation(Size::new(400.0, 100.0)).nodes()[1]
            .bounds
            .width();
        assert!(updated_width > initial_width);
    }

    #[test]
    fn row_widget_owns_horizontal_child_layout() {
        let mut ui = Ui::new();
        let row = ui.add_row(ui.root());
        ui.set_local(row, ui.properties().spacing, 5.0);
        let first = ui.add_text_block(row, "One");
        let second = ui.add_text_block(row, "Two");

        let positions = ui
            .presentation(Size::new(300.0, 100.0))
            .nodes()
            .iter()
            .filter(|node| node.source == first || node.source == second)
            .filter(|node| node.kind == crate::TEXT_BLOCK_PART)
            .map(|node| (node.source, node.bounds))
            .collect::<Vec<_>>();
        let first_bounds = positions
            .iter()
            .find(|(source, _)| *source == first)
            .map(|(_, bounds)| *bounds)
            .expect("first text block should be presented");
        let second_bounds = positions
            .iter()
            .find(|(source, _)| *source == second)
            .map(|(_, bounds)| *bounds)
            .expect("second text block should be presented");

        assert_eq!(first_bounds.y0, second_bounds.y0);
        assert_eq!(second_bounds.x0, first_bounds.x1 + 5.0);
    }

    #[test]
    fn toggle_hit_test_and_activation_update_widget_state() {
        let mut ui = Ui::new();
        let id = ui.add_toggle(ui.root(), "Enable sync");
        ui.set_local(
            id,
            ui.properties().background,
            Some(peniko::Color::from_rgb8(0x20, 0x25, 0x2d).into()),
        );
        ui.set_local(
            id,
            ui.properties().foreground,
            Some(peniko::Color::from_rgb8(0x6c, 0xd4, 0x92).into()),
        );

        let initial_thumb_x = ui
            .presentation(Size::new(240.0, 80.0))
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_THUMB_PART)
            .map(|node| node.bounds.x0)
            .expect("toggle should emit a thumb");

        assert_eq!(
            ui.hit_test(Size::new(240.0, 80.0), Point::new(2.0, 2.0)),
            Some(id)
        );
        assert!(ui.activate(id));
        assert!(ui.is_invalidated(id, MEASURE));
        assert!(ui.widget::<Toggle>(id).is_some_and(Toggle::checked));

        let updated_thumb_x = ui
            .presentation(Size::new(240.0, 80.0))
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_THUMB_PART)
            .map(|node| node.bounds.x0)
            .expect("toggle should emit a thumb after activation");
        assert!(updated_thumb_x > initial_thumb_x);
    }

    #[test]
    fn pointer_interaction_retains_hover_press_and_activates() {
        let mut ui = Ui::new();
        let viewport = Size::new(240.0, 80.0);
        let id = ui.add_toggle(ui.root(), "Enable sync");
        let bounds = ui
            .presentation(viewport)
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_PART)
            .map(|node| node.bounds)
            .expect("toggle should be presented");
        let point = bounds.center();

        assert!(ui.pointer_event(viewport, &pointer_move(point)));
        assert_eq!(ui.hovered(), Some(id));
        assert!(ui.state(id).is_some_and(ElementState::hovered));

        assert!(ui.pointer_event(viewport, &pointer_down(point)));
        assert_eq!(ui.pressed(), Some(id));
        assert!(ui.state(id).is_some_and(ElementState::pressed));

        assert!(ui.pointer_event(viewport, &pointer_up(point)));
        assert_eq!(ui.pressed(), None);
        assert!(ui.widget::<Toggle>(id).is_some_and(Toggle::checked));
        assert!(!ui.state(id).is_some_and(ElementState::pressed));

        assert!(ui.pointer_event(viewport, &PointerEvent::Leave(primary_pointer())));
        assert_eq!(ui.hovered(), None);
        assert!(!ui.state(id).is_some_and(ElementState::hovered));
    }

    #[test]
    fn pointer_route_uses_responder_capture_target_bubble() {
        let mut ui = Ui::new();
        let viewport = Size::new(240.0, 80.0);
        let id = ui.add_button(ui.root(), "Respond");
        let bounds = ui
            .presentation(viewport)
            .nodes()
            .iter()
            .find(|node| node.kind == crate::BUTTON_PART)
            .map(|node| node.bounds)
            .expect("button should be presented");

        let route = ui.pointer_route(viewport, bounds.center());
        let phases = route
            .dispatches
            .iter()
            .map(|dispatch| dispatch.phase)
            .collect::<Vec<_>>();
        let nodes = route
            .dispatches
            .iter()
            .map(|dispatch| dispatch.node)
            .collect::<Vec<_>>();

        assert_eq!(route.target, Some(id));
        assert_eq!(phases, vec![Phase::Capture, Phase::Target, Phase::Bubble]);
        assert_eq!(nodes, vec![ui.root(), id, ui.root()]);
    }

    #[test]
    fn pointer_capture_routes_outside_hits_until_release() {
        let mut ui = Ui::new();
        let viewport = Size::new(240.0, 80.0);
        let id = ui.add_toggle(ui.root(), "Capture");
        let bounds = ui
            .presentation(viewport)
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_PART)
            .map(|node| node.bounds)
            .expect("toggle should be presented");
        let inside = bounds.center();
        let outside = Point::new(viewport.width - 1.0, viewport.height - 1.0);

        assert!(ui.pointer_event(viewport, &pointer_down(inside)));
        assert_eq!(ui.pointer_route(viewport, outside).target, Some(id));

        assert!(ui.pointer_event(viewport, &pointer_move(outside)));
        assert_eq!(ui.pressed(), Some(id));
        assert_eq!(ui.hovered(), None);

        assert!(ui.pointer_event(viewport, &pointer_up(outside)));
        assert_eq!(ui.pressed(), None);
        assert!(!ui.widget::<Toggle>(id).is_some_and(Toggle::checked));
        assert_eq!(ui.pointer_route(viewport, outside).target, None);
    }

    #[test]
    fn keyboard_event_routes_to_focused_widget() {
        let mut ui = Ui::new();
        let viewport = Size::new(240.0, 80.0);
        let id = ui.add_toggle(ui.root(), "Keyboard");
        let bounds = ui
            .presentation(viewport)
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_PART)
            .map(|node| node.bounds)
            .expect("toggle should be presented");

        assert!(ui.pointer_event(viewport, &pointer_down(bounds.center())));
        assert_eq!(ui.focused(), Some(id));
        assert!(ui.keyboard_event(&KeyboardEvent::key_down(
            Key::Named(NamedKey::Enter),
            Code::Enter,
        )));

        assert!(ui.widget::<Toggle>(id).is_some_and(Toggle::checked));
    }

    #[test]
    fn input_state_tracks_and_clears_frame_transitions() {
        let mut ui = Ui::new();
        let viewport = Size::new(240.0, 80.0);
        let id = ui.add_toggle(ui.root(), "Input");
        let bounds = ui
            .presentation(viewport)
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_PART)
            .map(|node| node.bounds)
            .expect("toggle should be presented");

        assert!(ui.pointer_event(viewport, &pointer_down(bounds.center())));
        assert!(ui.input().primary_pointer.is_primary_just_pressed());

        ui.clear_input_frame();
        assert!(!ui.input().primary_pointer.is_primary_just_pressed());

        assert!(ui.keyboard_event(&KeyboardEvent::key_down(
            Key::Named(NamedKey::Enter),
            Code::Enter,
        )));
        assert!(
            ui.input()
                .keyboard
                .key_just_pressed(Key::Named(NamedKey::Enter))
        );
        assert!(ui.widget::<Toggle>(id).is_some_and(Toggle::checked));

        ui.clear_input_frame();
        assert!(
            !ui.input()
                .keyboard
                .key_just_pressed(Key::Named(NamedKey::Enter))
        );
    }

    fn primary_pointer() -> PointerInfo {
        PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        }
    }

    fn pointer_state(point: Point) -> PointerState {
        let mut state = PointerState::default();
        state.position.x = point.x;
        state.position.y = point.y;
        state.scale_factor = 1.0;
        state
    }

    fn pointer_move(point: Point) -> PointerEvent {
        PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(point),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        })
    }

    fn pointer_down(point: Point) -> PointerEvent {
        PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(point),
        })
    }

    fn pointer_up(point: Point) -> PointerEvent {
        PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(point),
        })
    }

    #[test]
    fn disabled_widget_is_not_hit_testable() {
        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Disabled");
        ui.set_enabled(id, false);
        let tree = ui.presentation(Size::new(240.0, 80.0));
        let button_bounds = tree
            .nodes()
            .iter()
            .find(|node| node.kind == crate::BUTTON_PART)
            .map(|node| node.bounds)
            .expect("button should be presented");

        assert_eq!(
            ui.hit_test(Size::new(240.0, 80.0), button_bounds.center()),
            None
        );
    }

    #[test]
    fn hit_testing_uses_presentation_stack_order() {
        let mut ui = Ui::new();
        let first = ui.add_button(ui.root(), "First");
        let second = ui.add_button(ui.root(), "Second");
        ui.set_local(ui.root(), ui.properties().spacing, -8.0);
        let tree = ui.presentation(Size::new(240.0, 80.0));
        let first_bounds = tree
            .nodes()
            .iter()
            .find(|node| node.source == first && node.kind == crate::BUTTON_PART)
            .map(|node| node.bounds)
            .expect("first button should be presented");
        let second_bounds = tree
            .nodes()
            .iter()
            .find(|node| node.source == second && node.kind == crate::BUTTON_PART)
            .map(|node| node.bounds)
            .expect("second button should be presented");
        let overlap = first_bounds.intersect(second_bounds);

        assert!(overlap.area() > 0.0);
        assert_eq!(
            ui.hit_test(Size::new(240.0, 80.0), overlap.center()),
            Some(second)
        );
    }

    #[test]
    fn toggle_template_supplies_styleable_subparts() {
        let mut ui = Ui::new();
        let id = ui.add_toggle(ui.root(), "Sync");
        let background: Brush = peniko::Color::from_rgb8(0x24, 0x30, 0x3a).into();
        let foreground: Brush = peniko::Color::from_rgb8(0x7a, 0xe5, 0xa1).into();
        set_test_toggle_style(&mut ui, id, background.clone(), foreground.clone());

        let initial = ui.presentation(Size::new(240.0, 80.0));
        let track = initial
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_TRACK_PART)
            .expect("toggle template should emit a track part");
        let thumb = initial
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_THUMB_PART)
            .expect("toggle template should emit a thumb part");
        let content = initial
            .nodes()
            .iter()
            .find(|node| node.kind == crate::CONTENT_PRESENTER_PART)
            .expect("toggle template should emit a content presenter");

        assert_eq!(track.background, Some(background.clone()));
        assert_eq!(thumb.background, Some(foreground.clone()));
        assert_eq!(content.text.as_ref().map(TextContent::as_str), Some("Sync"));

        assert!(ui.activate(id));
        let checked = ui.presentation(Size::new(240.0, 80.0));
        let checked_track = checked
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_TRACK_PART)
            .expect("checked toggle should emit a track part");
        let checked_thumb = checked
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_THUMB_PART)
            .expect("checked toggle should emit a thumb part");

        assert_eq!(checked_track.background, Some(foreground));
        assert_eq!(checked_thumb.background, Some(background));
    }

    #[test]
    fn custom_toggle_template_uses_slots_not_builtin_part_names() {
        const CUSTOM_TOGGLE_PART: crate::PartKind = crate::PartKind::new("custom-toggle");
        const CUSTOM_GROOVE_PART: crate::PartKind = crate::PartKind::new("custom-groove");
        const CUSTOM_KNOB_PART: crate::PartKind = crate::PartKind::new("custom-knob");
        const CUSTOM_LABEL_PART: crate::PartKind = crate::PartKind::new("custom-label");

        let mut ui = Ui::new();
        let id = ui.add_toggle(ui.root(), "Custom");
        let background: Brush = peniko::Color::from_rgb8(0x18, 0x20, 0x28).into();
        let foreground: Brush = peniko::Color::from_rgb8(0xf1, 0xc4, 0x53).into();
        set_test_toggle_style(&mut ui, id, background.clone(), foreground.clone());
        ui.set_local(
            id,
            ui.properties().toggle_template,
            crate::ControlTemplate::new(crate::TemplateNode::new(
                CUSTOM_TOGGLE_PART,
                [],
                [
                    crate::TemplateNode::new(
                        CUSTOM_GROOVE_PART,
                        [TemplateBinding::pass(crate::BACKGROUND_PROPERTY)],
                        [],
                    )
                    .with_slot(crate::TOGGLE_TRACK_SLOT),
                    crate::TemplateNode::new(
                        CUSTOM_KNOB_PART,
                        [TemplateBinding::pass(crate::BACKGROUND_PROPERTY)],
                        [],
                    )
                    .with_slot(crate::TOGGLE_THUMB_SLOT),
                    crate::TemplateNode::new(
                        CUSTOM_LABEL_PART,
                        [TemplateBinding::pass(crate::CONTENT_PROPERTY)],
                        [],
                    )
                    .with_slot(crate::CONTENT_SLOT),
                ],
            )),
        );

        let tree = ui.presentation(Size::new(240.0, 80.0));
        let groove = tree
            .nodes()
            .iter()
            .find(|node| node.kind == CUSTOM_GROOVE_PART)
            .expect("custom template should emit its own track part");
        let knob = tree
            .nodes()
            .iter()
            .find(|node| node.kind == CUSTOM_KNOB_PART)
            .expect("custom template should emit its own thumb part");
        let label = tree
            .nodes()
            .iter()
            .find(|node| node.kind == CUSTOM_LABEL_PART)
            .expect("custom template should emit its own label part");

        assert_eq!(groove.background, Some(background.clone()));
        assert_eq!(knob.background, Some(foreground));
        assert_eq!(label.text.as_ref().map(TextContent::as_str), Some("Custom"));
        assert!(knob.bounds.x0 > groove.bounds.x0);
        assert!(label.bounds.x0 > groove.bounds.x1);
    }

    #[test]
    fn template_style_state_follows_structural_slot_path() {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        const CUSTOM_TOGGLE_PART: crate::PartKind = crate::PartKind::new("path-toggle");
        const CUSTOM_GROOVE_PART: crate::PartKind = crate::PartKind::new("path-groove");
        const CUSTOM_KNOB_PART: crate::PartKind = crate::PartKind::new("path-knob");

        let mut ui = Ui::new();
        let id = ui.add_toggle(ui.root(), "Path");
        let props = ui.properties();
        let fallback: Brush = peniko::Color::from_rgb8(0xc4, 0x58, 0x7d).into();
        let nested: Brush = peniko::Color::from_rgb8(0x50, 0xb4, 0x88).into();
        let fallback_thumb = StyleBuilder::new()
            .set(props.background, Some(fallback.clone()))
            .build();
        let nested_thumb = StyleBuilder::new()
            .set(props.background, Some(nested.clone()))
            .build();

        ui.set_style(
            id,
            StyleCascadeBuilder::new()
                .push_rules(
                    StyleOrigin::Sheet,
                    [
                        (crate::style::toggle_thumb(), fallback_thumb),
                        (crate::style::toggle_thumb_in_track(), nested_thumb),
                    ],
                )
                .build(),
        );

        let tree = ui.presentation(Size::new(240.0, 80.0));
        let built_in_thumb = tree
            .nodes()
            .iter()
            .find(|node| node.kind == crate::TOGGLE_THUMB_PART)
            .expect("built-in toggle template should emit a nested thumb");
        assert_eq!(built_in_thumb.background, Some(nested));

        ui.set_local(
            id,
            props.toggle_template,
            crate::ControlTemplate::new(crate::TemplateNode::new(
                CUSTOM_TOGGLE_PART,
                [],
                [
                    crate::TemplateNode::new(
                        CUSTOM_GROOVE_PART,
                        [TemplateBinding::pass(crate::BACKGROUND_PROPERTY)],
                        [],
                    )
                    .with_slot(crate::TOGGLE_TRACK_SLOT),
                    crate::TemplateNode::new(
                        CUSTOM_KNOB_PART,
                        [TemplateBinding::pass(crate::BACKGROUND_PROPERTY)],
                        [],
                    )
                    .with_slot(crate::TOGGLE_THUMB_SLOT),
                ],
            )),
        );

        let tree = ui.presentation(Size::new(240.0, 80.0));
        let custom_knob = tree
            .nodes()
            .iter()
            .find(|node| node.kind == CUSTOM_KNOB_PART)
            .expect("custom template should emit a sibling thumb");
        assert_eq!(custom_knob.background, Some(fallback));
    }

    fn set_test_toggle_style(ui: &mut Ui, id: ElementId, background: Brush, foreground: Brush) {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        let props = ui.properties();
        let content = StyleBuilder::new()
            .set(props.foreground, Some(foreground.clone()))
            .build();
        let track = StyleBuilder::new()
            .set(props.background, Some(background.clone()))
            .build();
        let thumb = StyleBuilder::new()
            .set(props.background, Some(foreground.clone()))
            .build();
        let checked_track = StyleBuilder::new()
            .set(props.background, Some(foreground))
            .build();
        let checked_thumb = StyleBuilder::new()
            .set(props.background, Some(background))
            .build();

        ui.set_style(
            id,
            StyleCascadeBuilder::new()
                .push_rules(
                    StyleOrigin::Sheet,
                    [
                        (crate::style::toggle_content(), content),
                        (crate::style::toggle_track(), track),
                        (crate::style::toggle_thumb(), thumb),
                        (
                            crate::style::toggle_track_when(crate::CHECKED),
                            checked_track,
                        ),
                        (
                            crate::style::toggle_thumb_when(crate::CHECKED),
                            checked_thumb,
                        ),
                    ],
                )
                .build(),
        );
    }

    #[test]
    fn root_padding_and_spacing_arrange_children() {
        let mut ui = Ui::new();
        let props = ui.properties();
        ui.set_local(ui.root(), props.padding, Insets::new(11.0, 13.0, 0.0, 0.0));
        ui.set_local(ui.root(), props.spacing, 7.0);
        ui.add_button(ui.root(), "One");
        ui.add_button(ui.root(), "Two");

        let tree = ui.presentation(Size::new(200.0, 100.0));
        let first = &tree.nodes()[1];
        let second = &tree.nodes()[4];

        assert_eq!(first.bounds.x0, 11.0);
        assert_eq!(first.bounds.y0, 13.0);
        assert_eq!(second.bounds.x0, 11.0);
        assert_eq!(second.bounds.y0, first.bounds.y1 + 7.0);
    }

    #[test]
    fn button_min_width_expands_and_centers_content() {
        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Go");
        ui.set_local(id, ui.properties().min_width, 120.0);

        let tree = ui.presentation(Size::new(200.0, 100.0));
        let button = &tree.nodes()[1];
        let content = &tree.nodes()[3];

        assert_eq!(button.bounds.width(), 120.0);
        assert!(content.bounds.x0 > button.bounds.x0);
        assert!(content.bounds.x1 < button.bounds.x1);
        assert_eq!(
            content.bounds.x0 - button.bounds.x0,
            button.bounds.x1 - content.bounds.x1
        );
    }

    #[test]
    fn paint_scene_with_scale_records_physical_transform() {
        use imaging::record::{Command, Draw};

        let mut ui = Ui::new();
        ui.set_local(
            ui.root(),
            ui.properties().background,
            Some(peniko::Color::from_rgb8(0x10, 0x12, 0x14).into()),
        );

        let scene = ui.paint_scene_with_scale(Size::new(100.0, 50.0), 2.0);
        let Command::Draw(draw_id) = scene.commands()[0] else {
            panic!("expected root background draw command");
        };
        let Draw::Fill { transform, .. } = scene.draw_op(draw_id) else {
            panic!("expected root background fill");
        };

        assert_eq!(transform.as_coeffs(), [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn button_template_declares_semantic_parts() {
        let template = crate::button_template();
        let root = template.root();

        assert_eq!(root.kind, crate::BUTTON_PART);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].kind, crate::BORDER_PART);
        assert_eq!(
            root.children[0].bindings.as_ref(),
            &[
                TemplateBinding::pass(crate::BACKGROUND_PROPERTY),
                TemplateBinding::pass(crate::BORDER_PROPERTY),
                TemplateBinding::pass(crate::BORDER_WIDTH_PROPERTY),
                TemplateBinding::pass(crate::PADDING_PROPERTY),
                TemplateBinding::pass(crate::CORNER_RADIUS_PROPERTY),
            ],
        );
        assert_eq!(root.children[0].children.len(), 1);
        assert_eq!(
            root.children[0].children[0].kind,
            crate::CONTENT_PRESENTER_PART
        );
        assert_eq!(
            root.children[0].children[0].bindings.as_ref(),
            &[
                TemplateBinding::pass(crate::CONTENT_PROPERTY),
                TemplateBinding::pass(crate::FOREGROUND_PROPERTY),
            ],
        );
    }

    #[test]
    fn style_can_select_structurally_different_button_template() {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        const ACCENT_PART: crate::PartKind = crate::PartKind::new("test-accent");

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        let props = ui.properties();
        let alternate = crate::ControlTemplate::new(crate::TemplateNode::new(
            crate::BUTTON_PART,
            [],
            [crate::TemplateNode::new(
                crate::BORDER_PART,
                [
                    TemplateBinding::pass(crate::BACKGROUND_PROPERTY),
                    TemplateBinding::pass(crate::BORDER_PROPERTY),
                    TemplateBinding::pass(crate::BORDER_WIDTH_PROPERTY),
                    TemplateBinding::pass(crate::PADDING_PROPERTY),
                    TemplateBinding::pass(crate::CORNER_RADIUS_PROPERTY),
                ],
                [
                    crate::TemplateNode::new(
                        ACCENT_PART,
                        [TemplateBinding::pass(crate::BACKGROUND_PROPERTY)],
                        [],
                    ),
                    crate::TemplateNode::new(
                        crate::CONTENT_PRESENTER_PART,
                        [
                            TemplateBinding::pass(crate::CONTENT_PROPERTY),
                            TemplateBinding::pass(crate::FOREGROUND_PROPERTY),
                        ],
                        [],
                    ),
                ],
            )],
        ));
        let hovered_template = StyleBuilder::new()
            .set(props.template, alternate.clone())
            .build();
        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                crate::style::button_hovered(),
                hovered_template,
            )
            .build();

        ui.set_style(id, cascade);
        ui.set_hovered(id, true);

        assert_eq!(ui.resolve(id, props.template), alternate);
        let tree = ui.presentation(Size::new(200.0, 100.0));
        assert!(tree.nodes().iter().any(|node| node.kind == ACCENT_PART));
        assert_eq!(tree.nodes().len(), 5);
    }

    #[test]
    fn template_node_inset_shrinks_part_bounds() {
        const INNER_PART: crate::PartKind = crate::PartKind::new("test-inner-border");

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        let template = crate::ControlTemplate::new(crate::TemplateNode::new(
            crate::BUTTON_PART,
            [],
            [crate::TemplateNode::new(
                INNER_PART,
                [],
                [crate::TemplateNode::new(
                    crate::CONTENT_PRESENTER_PART,
                    [
                        TemplateBinding::pass(crate::CONTENT_PROPERTY),
                        TemplateBinding::pass(crate::FOREGROUND_PROPERTY),
                    ],
                    [],
                )],
            )
            .with_inset(3.0)],
        ));
        ui.set_local(id, ui.properties().template, template);

        let tree = ui.presentation(Size::new(200.0, 100.0));
        let button = &tree.nodes()[1];
        let inner = tree
            .nodes()
            .iter()
            .find(|node| node.kind == INNER_PART)
            .expect("template should instantiate inner part");

        assert_eq!(inner.bounds.x0, button.bounds.x0 + 3.0);
        assert_eq!(inner.bounds.y0, button.bounds.y0 + 3.0);
        assert_eq!(inner.bounds.x1, button.bounds.x1 - 3.0);
        assert_eq!(inner.bounds.y1, button.bounds.y1 - 3.0);
    }

    #[test]
    fn template_binding_can_map_compatible_source_to_different_target() {
        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        let background: Brush = peniko::Color::from_rgb8(0x1d, 0x4e, 0x89).into();
        let border: Brush = peniko::Color::from_rgb8(0xff, 0xc8, 0x57).into();
        let template = crate::ControlTemplate::new(crate::TemplateNode::new(
            crate::BUTTON_PART,
            [],
            [crate::TemplateNode::new(
                crate::BORDER_PART,
                [
                    TemplateBinding::new(crate::BACKGROUND_PROPERTY, crate::BORDER_PROPERTY),
                    TemplateBinding::new(crate::BORDER_PROPERTY, crate::BACKGROUND_PROPERTY),
                ],
                [],
            )],
        ));

        let props = ui.properties();
        ui.set_local(id, props.background, Some(background.clone()));
        ui.set_local(id, props.border, Some(border.clone()));
        ui.set_local(id, props.template, template);

        let tree = ui.presentation(Size::new(200.0, 100.0));
        let border_node = tree
            .nodes()
            .iter()
            .find(|node| node.kind == crate::BORDER_PART)
            .expect("custom template should emit a border part");

        assert_eq!(border_node.background, Some(border));
        assert_eq!(border_node.border, Some(background));
    }

    #[test]
    fn paint_scene_emits_valid_imaging() {
        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        ui.set_local(
            id,
            ui.properties().background,
            Some(peniko::Color::from_rgb8(0x2a, 0x6f, 0xdb).into()),
        );

        let scene = ui.paint_scene(Size::new(200.0, 100.0));

        assert!(scene.validate().is_ok());
        assert!(!scene.commands().is_empty());
    }

    #[test]
    fn paint_scene_emits_border_stroke() {
        use imaging::record::{Command, Draw};

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        ui.set_local(
            id,
            ui.properties().border,
            Some(peniko::Color::from_rgb8(0x00, 0x00, 0x00).into()),
        );
        ui.set_local(id, ui.properties().border_width, 1.0);

        let scene = ui.paint_scene(Size::new(200.0, 100.0));

        assert!(scene.commands().iter().any(|command| {
            let Command::Draw(draw_id) = command else {
                return false;
            };
            matches!(scene.draw_op(*draw_id), Draw::Stroke { .. })
        }));
    }

    #[test]
    fn hovered_state_resolves_through_style_cascade() {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        let props = ui.properties();
        let base: Brush = peniko::Color::from_rgb8(0x22, 0x22, 0x22).into();
        let hover: Brush = peniko::Color::from_rgb8(0x2a, 0x6f, 0xdb).into();

        let base_style = StyleBuilder::new()
            .set(props.background, Some(base.clone()))
            .build();
        let hover_style = StyleBuilder::new()
            .set(props.background, Some(hover.clone()))
            .build();
        let cascade = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Base, base_style)
            .push_rule(
                StyleOrigin::Sheet,
                crate::style::button_hovered(),
                hover_style,
            )
            .build();

        ui.set_style(id, cascade);
        assert_eq!(ui.resolve(id, props.background), Some(base));

        ui.set_hovered(id, true);
        assert_eq!(ui.resolve(id, props.background), Some(hover));
        assert!(ui.is_invalidated(id, VISUAL));
    }

    #[test]
    fn custom_properties_and_pseudos_resolve_through_style_cascade() {
        use understory_property::PropertyMetadataBuilder;
        use understory_style::{PseudoClassId, StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        const SELECTED: PseudoClassId = PseudoClassId(44);

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        let emphasis = ui.register_property(
            "Demo.Emphasis",
            PropertyMetadataBuilder::new(0.0_f64)
                .affects_channels(VISUAL.into_set())
                .build(),
        );
        let selected_hover = StyleBuilder::new().set(emphasis, 0.75).build();
        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                crate::style::button().with_pseudos([crate::HOVERED, SELECTED]),
                selected_hover,
            )
            .build();

        ui.set_style(id, cascade);
        assert_eq!(ui.resolve(id, emphasis), 0.0);

        ui.add_pseudo(id, SELECTED);
        assert_eq!(ui.resolve(id, emphasis), 0.0);

        ui.set_hovered(id, true);
        assert_eq!(ui.resolve(id, emphasis), 0.75);
        assert!(ui.is_invalidated(id, VISUAL));

        ui.remove_pseudo(id, SELECTED);
        assert_eq!(ui.resolve(id, emphasis), 0.0);
    }

    #[test]
    fn restyle_subject_marks_precise_channels_after_presentation() {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        let props = ui.properties();
        let base: Brush = peniko::Color::from_rgb8(0x22, 0x22, 0x22).into();
        let hover: Brush = peniko::Color::from_rgb8(0x2a, 0x6f, 0xdb).into();

        let base_style = StyleBuilder::new()
            .set(props.background, Some(base.clone()))
            .build();
        let hover_style = StyleBuilder::new()
            .set(props.background, Some(hover.clone()))
            .build();
        let cascade = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Base, base_style)
            .push_rule(
                StyleOrigin::Sheet,
                crate::style::button_hovered(),
                hover_style,
            )
            .build();

        ui.set_style(id, cascade);
        let tree = ui.presentation(Size::new(200.0, 100.0));
        let border = tree
            .nodes()
            .iter()
            .find(|node| node.kind == crate::BORDER_PART)
            .expect("button template should emit a border");
        assert_eq!(border.background, Some(base));
        assert!(!ui.is_invalidated(id, STYLE));
        assert!(!ui.is_invalidated(id, TEMPLATE));
        assert!(!ui.is_invalidated(id, MEASURE));
        assert!(!ui.is_invalidated(id, ARRANGE));
        assert!(!ui.is_invalidated(id, VISUAL));

        ui.set_hovered(id, true);

        assert!(!ui.is_invalidated(id, STYLE));
        assert!(!ui.is_invalidated(id, TEMPLATE));
        assert!(!ui.is_invalidated(id, MEASURE));
        assert!(!ui.is_invalidated(id, ARRANGE));
        assert!(ui.is_invalidated(id, VISUAL));

        let tree = ui.presentation(Size::new(200.0, 100.0));
        let border = tree
            .nodes()
            .iter()
            .find(|node| node.kind == crate::BORDER_PART)
            .expect("button template should still emit a border");
        assert_eq!(border.background, Some(hover));
    }

    #[test]
    fn restyle_subject_marks_template_part_changes_from_owner_state() {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Save");
        let props = ui.properties();
        let base: Brush = peniko::Color::from_rgb8(0x44, 0x44, 0x44).into();
        let hover: Brush = peniko::Color::from_rgb8(0xe0, 0xf2, 0xff).into();

        let base_content = StyleBuilder::new()
            .set(props.foreground, Some(base.clone()))
            .build();
        let hover_content = StyleBuilder::new()
            .set(props.foreground, Some(hover.clone()))
            .build();
        let cascade = StyleCascadeBuilder::new()
            .push_rules(
                StyleOrigin::Sheet,
                [
                    (crate::style::button_content(), base_content),
                    (
                        crate::style::button_content_when(crate::HOVERED),
                        hover_content,
                    ),
                ],
            )
            .build();

        ui.set_style(id, cascade);
        let tree = ui.presentation(Size::new(200.0, 100.0));
        let content = tree
            .nodes()
            .iter()
            .find(|node| node.kind == crate::CONTENT_PRESENTER_PART)
            .expect("button template should emit content");
        assert_eq!(content.foreground, Some(base));

        ui.set_hovered(id, true);

        assert!(!ui.is_invalidated(id, STYLE));
        assert!(!ui.is_invalidated(id, TEMPLATE));
        assert!(!ui.is_invalidated(id, MEASURE));
        assert!(!ui.is_invalidated(id, ARRANGE));
        assert!(ui.is_invalidated(id, VISUAL));

        let tree = ui.presentation(Size::new(200.0, 100.0));
        let content = tree
            .nodes()
            .iter()
            .find(|node| node.kind == crate::CONTENT_PRESENTER_PART)
            .expect("button template should still emit content");
        assert_eq!(content.foreground, Some(hover));
    }

    #[test]
    fn style_inspection_reports_matching_rules_and_winning_source() {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Inspect");
        let props = ui.properties();
        let base: Brush = peniko::Color::from_rgb8(0x22, 0x22, 0x22).into();
        let hover: Brush = peniko::Color::from_rgb8(0x2a, 0x6f, 0xdb).into();

        let base_style = StyleBuilder::new()
            .set(props.background, Some(base))
            .build();
        let hover_style = StyleBuilder::new()
            .set(props.background, Some(hover))
            .build();
        let cascade = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Base, base_style)
            .push_rule(
                StyleOrigin::Sheet,
                crate::style::button_hovered(),
                hover_style,
            )
            .build();

        ui.set_style(id, cascade);
        let base_inspection = ui
            .inspect_style(id, StyleSubject::owner(), props.background)
            .expect("button should have a style cascade");
        assert_eq!(base_inspection.property_name, Some("Background"));
        assert!(base_inspection.matching_rules.is_empty());
        assert_eq!(
            base_inspection.winning_source,
            Some(StyleSourceInspection::Direct {
                origin: StyleOrigin::Base,
                source_index: 0,
            })
        );

        ui.presentation(Size::new(200.0, 100.0));
        ui.set_hovered(id, true);

        let hover_inspection = ui
            .inspect_style(id, StyleSubject::owner(), props.background)
            .expect("button should still have a style cascade");
        let Some(StyleSourceInspection::Rule(rule)) = hover_inspection.winning_source else {
            panic!("hovered button background should be won by a selector rule");
        };
        assert_eq!(rule.origin, StyleOrigin::Sheet);
        assert_eq!(rule.selector, crate::style::button_hovered().into());
        assert_eq!(hover_inspection.matching_rules.len(), 1);
    }

    #[test]
    fn style_inspection_can_explain_template_slot_subjects() {
        use understory_style::{StyleBuilder, StyleCascadeBuilder, StyleOrigin};

        let mut ui = Ui::new();
        let id = ui.add_button(ui.root(), "Inspect");
        let props = ui.properties();
        let content_foreground: Brush = peniko::Color::from_rgb8(0xdc, 0xe4, 0xee).into();
        let content_style = StyleBuilder::new()
            .set(props.foreground, Some(content_foreground))
            .build();
        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                crate::style::button_content(),
                content_style,
            )
            .build();

        ui.set_style(id, cascade);

        let inspection = ui
            .inspect_style(id, StyleSubject::content(), props.foreground)
            .expect("button should have a style cascade");
        assert_eq!(inspection.property_name, Some("Foreground"));
        assert_eq!(inspection.matching_rules.len(), 1);
        let Some(StyleSourceInspection::Rule(rule)) = inspection.winning_source else {
            panic!("content foreground should be won by a selector rule");
        };
        assert_eq!(rule.selector, crate::style::button_content());
    }
}
