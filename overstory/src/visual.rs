// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Visual lowering into `imaging`.

use alloc::vec::Vec;

use imaging::{
    Painter,
    record::{self, Glyph},
};
use kurbo::{Affine, RoundedRect, Stroke};
use peniko::{Brush, Color, Fill, Style};

use crate::{PresentationTree, TextSystem};

/// Lowers a laid-out presentation tree into an owned imaging scene.
#[must_use]
pub fn lower_presentation(tree: &PresentationTree, text: &mut TextSystem) -> record::Scene {
    lower_presentation_with_scale(tree, text, 1.0)
}

/// Lowers a laid-out presentation tree into an imaging scene scaled to device pixels.
///
/// `scale_factor` converts logical presentation coordinates into physical
/// pixels. Non-finite or non-positive values are treated as `1.0`.
#[must_use]
pub fn lower_presentation_with_scale(
    tree: &PresentationTree,
    text: &mut TextSystem,
    scale_factor: f64,
) -> record::Scene {
    let mut scene = record::Scene::new();
    let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let root_transform = if (scale_factor - 1.0).abs() < f64::EPSILON {
        Affine::IDENTITY
    } else {
        Affine::scale(scale_factor)
    };

    {
        let mut painter = Painter::new(&mut scene);
        for node in tree.nodes() {
            if let Some(brush) = node.background.as_ref() {
                if node.corner_radius > 0.0 {
                    painter
                        .fill(
                            RoundedRect::from_rect(node.bounds, node.corner_radius),
                            brush,
                        )
                        .transform(root_transform)
                        .draw();
                } else {
                    painter
                        .fill(node.bounds, brush)
                        .transform(root_transform)
                        .draw();
                }
            }
            if let Some(brush) = node.border.as_ref()
                && node.border_width > 0.0
            {
                let stroke = Stroke::new(node.border_width);
                if node.corner_radius > 0.0 {
                    painter
                        .stroke(
                            RoundedRect::from_rect(node.bounds, node.corner_radius),
                            &stroke,
                            brush,
                        )
                        .transform(root_transform)
                        .draw();
                } else {
                    painter
                        .stroke(node.bounds, &stroke, brush)
                        .transform(root_transform)
                        .draw();
                }
            }

            let Some(content) = node.text.as_ref() else {
                continue;
            };
            let brush = node
                .foreground
                .clone()
                .unwrap_or_else(|| Brush::Solid(Color::BLACK));
            for run in text.shape_with_style(
                content,
                brush,
                &node.text_style,
                Some(crate::text::text_width_f32(node.bounds.width())),
            ) {
                if run.glyphs.is_empty() {
                    continue;
                }

                let glyphs = run
                    .glyphs
                    .iter()
                    .map(|glyph| Glyph {
                        id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                    })
                    .collect::<Vec<_>>();
                painter
                    .glyphs(&run.font, &run.brush)
                    .transform(root_transform * Affine::translate((node.bounds.x0, node.bounds.y0)))
                    .font_size(run.font_size)
                    .normalized_coords(&run.normalized_coords)
                    .draw(&Style::Fill(Fill::NonZero), &glyphs);
            }
        }
    }

    scene
}
