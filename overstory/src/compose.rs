// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed authoring specs for appending configured widgets.
//!
//! **Fence:** this module owns ergonomic widget construction; it explicitly
//! does not own widget behavior, style matching, property registration, or
//! layout policy.

use alloc::{boxed::Box, vec::Vec};

use understory_property::Property;
use understory_style::{ClassId, StyleCascade};

use crate::{
    Button, ControlTemplate, ElementId, Panel, Row, TextBlock, TextContent, TextInput, Toggle, Ui,
    Widget,
};

type ElementWrite = Box<dyn FnOnce(&mut Ui, ElementId)>;

/// A typed append operation that realizes one element under a parent.
pub trait AppendSpec {
    /// Appends this spec under `parent`, returning the realized element id.
    fn append_to(self, ui: &mut Ui, parent: ElementId) -> ElementId;
}

/// Creates a configured button append spec.
#[must_use]
pub fn button(content: impl Into<TextContent>) -> WidgetSpec<Button> {
    WidgetSpec::new(Button::new(content))
}

/// Creates a configured text block append spec.
#[must_use]
pub fn text_block(content: impl Into<TextContent>) -> WidgetSpec<TextBlock> {
    WidgetSpec::new(TextBlock::new(content))
}

/// Creates a configured text input append spec.
#[must_use]
pub fn text_input() -> WidgetSpec<TextInput> {
    WidgetSpec::new(TextInput::new())
}

/// Creates a configured panel append spec.
#[must_use]
pub fn panel() -> WidgetSpec<Panel> {
    WidgetSpec::new(Panel::new())
}

/// Creates a configured horizontal row append spec.
#[must_use]
pub fn row() -> WidgetSpec<Row> {
    WidgetSpec::new(Row::new())
}

/// Creates a configured toggle append spec.
#[must_use]
pub fn toggle(content: impl Into<TextContent>) -> WidgetSpec<Toggle> {
    WidgetSpec::new(Toggle::new(content))
}

/// A widget plus initial element configuration.
pub struct WidgetSpec<W> {
    widget: W,
    writes: Vec<ElementWrite>,
}

impl<W> core::fmt::Debug for WidgetSpec<W>
where
    W: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WidgetSpec")
            .field("widget", &self.widget)
            .field("write_count", &self.writes.len())
            .finish()
    }
}

impl<W> WidgetSpec<W> {
    fn new(widget: W) -> Self {
        Self {
            widget,
            writes: Vec::new(),
        }
    }

    /// Sets a local dependency property value after the widget is appended.
    #[must_use]
    pub fn set<T>(mut self, property: Property<T>, value: T) -> Self
    where
        T: Clone + PartialEq + 'static,
    {
        self.writes.push(Box::new(move |ui, id| {
            ui.set_local(id, property, value);
        }));
        self
    }

    /// Adds a selector class after the widget is appended.
    #[must_use]
    pub fn class(mut self, class: ClassId) -> Self {
        self.writes.push(Box::new(move |ui, id| {
            ui.add_class(id, class);
        }));
        self
    }

    /// Assigns a style cascade after the widget is appended.
    #[must_use]
    pub fn style(mut self, style: StyleCascade) -> Self {
        self.writes.push(Box::new(move |ui, id| {
            ui.set_style(id, style);
        }));
        self
    }
}

impl WidgetSpec<Button> {
    /// Sets the control template used by this button.
    #[must_use]
    pub fn template(self, template: ControlTemplate) -> Self {
        self.set_template_property(template, |properties| properties.template)
    }
}

impl WidgetSpec<TextBlock> {
    /// Sets the control template used by this text block.
    #[must_use]
    pub fn template(self, template: ControlTemplate) -> Self {
        self.set_template_property(template, |properties| properties.text_template)
    }
}

impl WidgetSpec<TextInput> {
    /// Sets initial text.
    #[must_use]
    pub fn text(mut self, text: &str) -> Self {
        self.widget.set_text(text);
        self
    }

    /// Sets placeholder text shown while the input is empty.
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<TextContent>) -> Self {
        self.widget = self.widget.placeholder(placeholder);
        self
    }

    /// Sets whether editing is restricted to one line.
    #[must_use]
    pub fn single_line(mut self, single_line: bool) -> Self {
        self.widget = self.widget.single_line(single_line);
        self
    }

    /// Sets the control template used by this text input.
    #[must_use]
    pub fn template(self, template: ControlTemplate) -> Self {
        self.set_template_property(template, |properties| properties.text_input_template)
    }
}

impl WidgetSpec<Toggle> {
    /// Sets the initial checked state.
    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        self.widget.set_checked(checked);
        self
    }

    /// Sets the control template used by this toggle.
    #[must_use]
    pub fn template(self, template: ControlTemplate) -> Self {
        self.set_template_property(template, |properties| properties.toggle_template)
    }
}

impl<W> WidgetSpec<W> {
    fn set_template_property(
        mut self,
        template: ControlTemplate,
        property: impl Fn(crate::UiProperties) -> Property<ControlTemplate> + 'static,
    ) -> Self {
        self.writes.push(Box::new(move |ui, id| {
            ui.set_local(id, property(ui.properties()), template);
        }));
        self
    }
}

impl<W> AppendSpec for WidgetSpec<W>
where
    W: Widget + 'static,
{
    fn append_to(self, ui: &mut Ui, parent: ElementId) -> ElementId {
        let id = ui.append(parent, self.widget);
        for write in self.writes {
            write(ui, id);
        }
        id
    }
}
