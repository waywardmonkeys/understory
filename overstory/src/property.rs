// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Overstory dependency properties.

use alloc::{borrow::Cow, boxed::Box, string::String};

use invalidation::ChannelSet;
use kurbo::Insets;
use peniko::Brush;
use understory_property::{Property, PropertyMetadataBuilder, PropertyRegistry};

use crate::{
    ARRANGE, ControlTemplate, MEASURE, STYLE, TEMPLATE, VISUAL, button_template,
    text_block_template, toggle_template,
};

/// Text payload for semantic content properties.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextContent(String);

impl TextContent {
    /// Creates text content from an owned string.
    #[must_use]
    pub const fn new(text: String) -> Self {
        Self(text)
    }

    /// Returns the text as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for TextContent {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for TextContent {
    fn from(value: &str) -> Self {
        Self(String::from(value))
    }
}

/// Resolved text styling inputs used by measurement and visual lowering.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    font_size: f64,
    font_family: Box<str>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            font_family: Box::from("system-ui, sans-serif"),
        }
    }
}

impl TextStyle {
    /// Creates text style inputs.
    #[must_use]
    pub fn new(font_size: f64, font_family: impl Into<Box<str>>) -> Self {
        Self {
            font_size: font_size.max(1.0),
            font_family: font_family.into(),
        }
    }

    /// Returns the font size in logical pixels per em.
    #[must_use]
    pub const fn font_size(&self) -> f64 {
        self.font_size
    }

    /// Returns the CSS-like font family list.
    #[must_use]
    pub fn font_family(&self) -> &str {
        self.font_family.as_ref()
    }

    pub(crate) fn family_cow(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.font_family())
    }
}

/// Registered dependency properties used by the first semantic runtime slice.
#[derive(Clone, Copy, Debug)]
pub struct UiProperties {
    /// Whether a control can be interacted with.
    pub enabled: Property<bool>,
    /// Brush used by a control template for background surfaces.
    pub background: Property<Option<Brush>>,
    /// Brush used by a control template for foreground content.
    pub foreground: Property<Option<Brush>>,
    /// Brush used by a control template for chrome strokes.
    pub border: Property<Option<Brush>>,
    /// Stroke width used by chrome strokes.
    pub border_width: Property<f64>,
    /// Interior space reserved around presented content.
    pub padding: Property<Insets>,
    /// Minimum arranged width for controls.
    pub min_width: Property<f64>,
    /// Space inserted between stacked children.
    pub spacing: Property<f64>,
    /// Corner radius for template chrome surfaces.
    pub corner_radius: Property<f64>,
    /// Structural template used to expand a semantic control.
    pub template: Property<ControlTemplate>,
    /// Font size for text-bearing widgets.
    pub font_size: Property<f64>,
    /// Font family for text-bearing widgets.
    pub font_family: Property<Box<str>>,
    /// Default structural template used to expand text blocks.
    pub text_template: Property<ControlTemplate>,
    /// Default structural template used to expand toggles.
    pub toggle_template: Property<ControlTemplate>,
}

impl UiProperties {
    /// Registers the built-in Overstory UI properties.
    pub fn register(registry: &mut PropertyRegistry) -> Self {
        let enabled = registry.register(
            "Enabled",
            PropertyMetadataBuilder::new(true)
                .affects_channels(STYLE.into_set())
                .build(),
        );
        let background = registry.register(
            "Background",
            PropertyMetadataBuilder::new(None::<Brush>)
                .affects_channels(VISUAL.into_set())
                .build(),
        );
        let foreground = registry.register(
            "Foreground",
            PropertyMetadataBuilder::new(None::<Brush>)
                .inherits(true)
                .affects_channels(VISUAL.into_set())
                .build(),
        );
        let border = registry.register(
            "Border",
            PropertyMetadataBuilder::new(None::<Brush>)
                .affects_channels(VISUAL.into_set())
                .build(),
        );
        let border_width = registry.register(
            "BorderWidth",
            PropertyMetadataBuilder::new(0.0_f64)
                .affects_channels(VISUAL.into_set())
                .build(),
        );
        let padding = registry.register(
            "Padding",
            PropertyMetadataBuilder::new(Insets::uniform(0.0))
                .affects_channels(MEASURE.into_set() | ARRANGE.into_set())
                .build(),
        );
        let min_width = registry.register(
            "MinWidth",
            PropertyMetadataBuilder::new(0.0_f64)
                .affects_channels(MEASURE.into_set() | ARRANGE.into_set())
                .build(),
        );
        let spacing = registry.register(
            "Spacing",
            PropertyMetadataBuilder::new(0.0_f64)
                .affects_channels(ARRANGE.into_set())
                .build(),
        );
        let corner_radius = registry.register(
            "CornerRadius",
            PropertyMetadataBuilder::new(0.0_f64)
                .affects_channels(VISUAL.into_set())
                .build(),
        );
        let template = registry.register(
            "ControlTemplate",
            PropertyMetadataBuilder::new(button_template())
                .affects_channels(TEMPLATE.into_set())
                .build(),
        );
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(16.0_f64)
                .inherits(true)
                .coerce(|value| value.max(1.0))
                .affects_channels(MEASURE.into_set() | VISUAL.into_set())
                .build(),
        );
        let font_family = registry.register(
            "FontFamily",
            PropertyMetadataBuilder::new(Box::<str>::from("system-ui, sans-serif"))
                .inherits(true)
                .affects_channels(MEASURE.into_set() | VISUAL.into_set())
                .build(),
        );
        let text_template = registry.register(
            "TextTemplate",
            PropertyMetadataBuilder::new(text_block_template())
                .affects_channels(TEMPLATE.into_set())
                .build(),
        );
        let toggle_template = registry.register(
            "ToggleTemplate",
            PropertyMetadataBuilder::new(toggle_template())
                .affects_channels(TEMPLATE.into_set())
                .build(),
        );

        Self {
            enabled,
            background,
            foreground,
            border,
            border_width,
            padding,
            min_width,
            spacing,
            corner_radius,
            template,
            font_size,
            font_family,
            text_template,
            toggle_template,
        }
    }

    pub(crate) fn all_channels() -> ChannelSet {
        STYLE.into_set()
            | TEMPLATE.into_set()
            | MEASURE.into_set()
            | ARRANGE.into_set()
            | VISUAL.into_set()
    }
}
