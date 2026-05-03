// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Basic built-in style recipes for Overstory controls.
//!
//! **Fence:** this module owns neutral default style recipes for built-in
//! controls; it explicitly does not own application variants, brand classes,
//! theme selection, or widget behavior.

use peniko::{Brush, Color};
use understory_style::{StyleBuilder, StyleCascade, StyleCascadeBuilder, StyleOrigin};

use crate::{CHECKED, HOVERED, PRESSED, UiProperties, style};

/// Returns a neutral default style for buttons.
#[must_use]
pub fn button_style(properties: UiProperties) -> StyleCascade {
    let base = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x2f, 0x36, 0x42))))
        .set(properties.foreground, Some(brush(rgb(0xf5, 0xf7, 0xfa))))
        .set(properties.border, Some(brush(rgb(0x52, 0x61, 0x73))))
        .set(properties.border_width, 1.0)
        .build();
    let hover = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x39, 0x43, 0x51))))
        .build();
    let pressed = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x22, 0x28, 0x31))))
        .build();

    StyleCascadeBuilder::new()
        .push_style(StyleOrigin::Base, base)
        .push_rules(
            StyleOrigin::Sheet,
            [
                (style::button_hovered(), hover),
                (style::button_pressed(), pressed),
            ],
        )
        .build()
}

/// Returns a neutral default style for toggles, including track and thumb parts.
#[must_use]
pub fn toggle_style(properties: UiProperties) -> StyleCascade {
    let base = StyleBuilder::new()
        .set(properties.foreground, Some(brush(rgb(0xf8, 0xfa, 0xfc))))
        .build();
    let content = StyleBuilder::new()
        .set(properties.foreground, Some(brush(rgb(0xdc, 0xe4, 0xee))))
        .build();
    let track = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x27, 0x2d, 0x36))))
        .set(properties.border, Some(brush(rgb(0x47, 0x55, 0x65))))
        .set(properties.border_width, 1.0)
        .set(properties.corner_radius, 12.0)
        .build();
    let thumb = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0xd9, 0xe1, 0xea))))
        .set(properties.corner_radius, 9.0)
        .build();
    let track_hover = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x31, 0x39, 0x45))))
        .build();
    let track_pressed = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x1d, 0x23, 0x2b))))
        .build();
    let track_checked = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x5a, 0xc8, 0x87))))
        .set(properties.border, Some(brush(rgb(0x8d, 0xef, 0xb2))))
        .build();
    let thumb_checked = StyleBuilder::new()
        .set(properties.background, Some(brush(rgb(0x13, 0x24, 0x1a))))
        .build();

    StyleCascadeBuilder::new()
        .push_style(StyleOrigin::Base, base)
        .push_rules(
            StyleOrigin::Sheet,
            [
                (style::toggle_content(), content),
                (style::toggle_track(), track),
                (style::toggle_thumb(), thumb),
                (style::toggle_track_when(HOVERED), track_hover),
                (style::toggle_track_when(PRESSED), track_pressed),
                (style::toggle_track_when(CHECKED), track_checked),
                (style::toggle_thumb_when(CHECKED), thumb_checked),
            ],
        )
        .build()
}

/// Returns a neutral default style for panels.
#[must_use]
pub fn panel_style(properties: UiProperties) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_style(
            StyleOrigin::Base,
            StyleBuilder::new()
                .set(properties.background, Some(brush(rgb(0x1b, 0x1f, 0x26))))
                .set(properties.border, Some(brush(rgb(0x38, 0x42, 0x50))))
                .set(properties.border_width, 1.0)
                .build(),
        )
        .build()
}

/// Returns a neutral default style for rows.
#[must_use]
pub fn row_style(properties: UiProperties) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_style(
            StyleOrigin::Base,
            StyleBuilder::new()
                .set(properties.background, None::<Brush>)
                .build(),
        )
        .build()
}

/// Returns a neutral default style for text blocks.
#[must_use]
pub fn text_block_style(properties: UiProperties) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_style(
            StyleOrigin::Base,
            StyleBuilder::new()
                .set(properties.foreground, Some(brush(rgb(0xdc, 0xe4, 0xee))))
                .build(),
        )
        .build()
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

fn brush(color: Color) -> Brush {
    Brush::from(color)
}
