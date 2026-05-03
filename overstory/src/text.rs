// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text services built on Parley.

use alloc::{borrow::Cow, vec::Vec};

use kurbo::Size;
use parley::{
    FontContext, FontFamily, FontFamilyName, GenericFamily, LayoutContext, PlainEditor,
    PlainEditorDriver, PositionedLayoutItem, StyleProperty,
};
use peniko::{Brush, FontData};

use crate::{TextContent, TextStyle};

/// One positioned glyph in a shaped text run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextGlyph {
    /// Glyph identifier in the run font.
    pub id: u32,
    /// Glyph x position relative to the shaped text origin.
    pub x: f32,
    /// Glyph y position relative to the shaped text origin.
    pub y: f32,
}

/// One shaped glyph run ready for visual lowering.
#[derive(Clone, Debug, PartialEq)]
pub struct TextGlyphRun {
    /// Font backing this run.
    pub font: FontData,
    /// Font size in pixels per em.
    pub font_size: f32,
    /// Normalized variable font coordinates.
    pub normalized_coords: Vec<i16>,
    /// Brush used to paint glyphs.
    pub brush: Brush,
    /// Positioned glyphs in visual order.
    pub glyphs: Vec<TextGlyph>,
}

/// Shared text resources for measuring and shaping text.
pub struct TextSystem {
    font_context: FontContext,
    layout_context: LayoutContext<Brush>,
}

impl core::fmt::Debug for TextSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextSystem").finish_non_exhaustive()
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl TextSystem {
    /// Creates text resources for a UI runtime.
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_context: FontContext::new(),
            layout_context: LayoutContext::new(),
        }
    }

    /// Measures a single unwrapped text run with default text styling.
    #[must_use]
    pub fn measure(&mut self, content: &TextContent) -> Size {
        self.measure_with_style(content, &TextStyle::default(), None)
    }

    /// Measures text using the supplied style and optional wrap width.
    #[must_use]
    pub fn measure_with_style(
        &mut self,
        content: &TextContent,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Size {
        let text = content.as_str();
        if text.is_empty() {
            return Size::ZERO;
        }

        let builder = self.builder(text, Brush::Solid(peniko::Color::BLACK), style);
        let mut layout = builder.build(text);
        layout.break_all_lines(max_width);

        let measured = Size::new(f64::from(layout.width()), f64::from(layout.height()));
        if measured.width > 0.0 && measured.height > 0.0 {
            return measured;
        }

        fontless_fallback_measure(text, style)
    }

    /// Shapes a single unwrapped text run for visual lowering using default text styling.
    #[must_use]
    pub fn shape(&mut self, content: &TextContent, brush: Brush) -> Vec<TextGlyphRun> {
        self.shape_with_style(content, brush, &TextStyle::default(), None)
    }

    /// Shapes text using the supplied style and optional wrap width.
    #[must_use]
    pub fn shape_with_style(
        &mut self,
        content: &TextContent,
        brush: Brush,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Vec<TextGlyphRun> {
        let text = content.as_str();
        if text.is_empty() {
            return Vec::new();
        }

        let builder = self.builder(text, brush, style);
        let mut layout = builder.build(text);
        layout.break_all_lines(max_width);

        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                let run = glyph_run.run();
                let mut glyph_origin_x = glyph_run.offset();
                let baseline_y = glyph_run.baseline();
                let glyphs = glyph_run
                    .glyphs()
                    .map(|glyph| {
                        let shaped = TextGlyph {
                            id: glyph.id,
                            x: glyph_origin_x + glyph.x,
                            y: baseline_y - glyph.y,
                        };
                        glyph_origin_x += glyph.advance;
                        shaped
                    })
                    .collect();

                runs.push(TextGlyphRun {
                    font: run.font().clone(),
                    font_size: run.font_size(),
                    normalized_coords: run.normalized_coords().to_vec(),
                    brush: glyph_run.style().brush.clone(),
                    glyphs,
                });
            }
        }

        runs
    }

    /// Runs a Parley plain editor operation with this text system's shared contexts.
    pub fn with_plain_editor<R>(
        &mut self,
        editor: &mut PlainEditor<Brush>,
        f: impl FnOnce(&mut PlainEditorDriver<'_, Brush>) -> R,
    ) -> R {
        let mut driver = editor.driver(&mut self.font_context, &mut self.layout_context);
        f(&mut driver)
    }

    /// Refreshes a plain editor layout using the shared text contexts and text style.
    pub fn refresh_plain_editor_layout(
        &mut self,
        editor: &mut PlainEditor<Brush>,
        style: &TextStyle,
        width: f32,
    ) -> Size {
        let styles = editor.edit_styles();
        styles.insert(StyleProperty::FontFamily(parsed_font_family(style)));
        styles.insert(StyleProperty::FontSize(font_size_f32(style)));
        editor.set_width(Some(width));

        self.plain_editor_layout_size(editor)
    }

    /// Returns the current plain editor layout size, refreshing only when dirty.
    pub fn plain_editor_layout_size(&mut self, editor: &mut PlainEditor<Brush>) -> Size {
        editor.refresh_layout(&mut self.font_context, &mut self.layout_context);
        let layout = editor
            .try_layout()
            .expect("plain editor layout was just refreshed");
        Size::new(f64::from(layout.full_width()), f64::from(layout.height()))
    }

    fn builder<'a>(
        &'a mut self,
        text: &'a str,
        brush: Brush,
        style: &'a TextStyle,
    ) -> parley::RangedBuilder<'a, Brush> {
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.font_context, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            style.family_cow(),
        )));
        builder.push_default(StyleProperty::FontSize(font_size_f32(style)));
        builder.push_default(StyleProperty::Brush(brush));
        builder
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Parley APIs use f32 font sizes; Overstory constrains font sizes to UI-scale values."
)]
fn font_size_f32(style: &TextStyle) -> f32 {
    style.font_size().max(1.0) as f32
}

fn parsed_font_family(style: &TextStyle) -> FontFamily<'static> {
    let parsed: Vec<_> = FontFamilyName::parse_css_list(style.font_family())
        .filter_map(Result::ok)
        .map(FontFamilyName::into_owned)
        .collect();
    if parsed.is_empty() {
        FontFamily::from(GenericFamily::SystemUi)
    } else {
        FontFamily::List(Cow::Owned(parsed))
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Parley line widths are f32; UI coordinates are clamped to the representable f32 range."
)]
pub(crate) fn text_width_f32(width: f64) -> f32 {
    width.max(1.0).min(f64::from(f32::MAX)) as f32
}

fn fontless_fallback_measure(text: &str, style: &TextStyle) -> Size {
    Size::new(
        text.chars().count() as f64 * style.font_size() * 0.45,
        style.font_size(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_measurement_produces_nonzero_size_for_text() {
        let mut text = TextSystem::new();
        let size = text.measure(&TextContent::from("Save"));

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
    }

    #[test]
    fn text_shaping_produces_glyph_runs_when_fonts_are_available() {
        let mut text = TextSystem::new();
        let runs = text.shape(
            &TextContent::from("Save"),
            Brush::Solid(peniko::Color::BLACK),
        );

        #[cfg(feature = "std")]
        assert!(
            runs.iter().any(|run| !run.glyphs.is_empty()),
            "std text shaping should resolve system fonts"
        );

        #[cfg(not(feature = "std"))]
        if !runs.is_empty() {
            assert!(runs.iter().any(|run| !run.glyphs.is_empty()));
        }
    }
}
