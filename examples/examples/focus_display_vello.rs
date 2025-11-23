// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Focus + display list + Vello.
//!
//! This example builds a small 3×3 grid of cards, uses
//! `understory_focus::DefaultPolicy` to move focus between them with
//! Tab and arrow keys, and renders the result via
//! `understory_display_vello` into a Vello `Scene`.
//!
//! Run:
//! - `cargo run -p understory_examples --example focus_display_vello`

use kurbo::{Affine, BezPath, Rect};
use ui_events::{
    keyboard::{Key, KeyState, NamedKey},
    pointer::PointerEvent,
};
use understory_display::{
    ClipId, DisplayListBuilder, DisplayPainter, GroupId, ImageId, PaintId, PathId, StrokeId,
};
use understory_display_vello::{ResourceResolver, record_scene};
use understory_examples::display_resources::DisplayResources;
use understory_examples::vello_winit::{VelloDemo, VelloWinitApp};
use understory_focus::{DefaultPolicy, FocusEntry, FocusPolicy, FocusSpace, Navigation, WrapMode};
use understory_responder::focus::FocusState;
use understory_responder::hover::HoverState;
use vello::Scene;
use vello::peniko::{Brush, Color, ImageBrush, color::palette};
use winit::event_loop::EventLoop;

type WidgetId = u8;

/// A single focusable card in the grid.
struct Card {
    id: WidgetId,
    rect: Rect,
    base_color: Color,
    /// Path geometry id for this card's rect.
    path_id: PathId,
    /// Paint ids for different visual states.
    paint_base: PaintId,
    paint_hover: PaintId,
    paint_focus: PaintId,
}

/// Scene state shared between focus, hit-testing, and rendering.
struct FocusDemoScene {
    /// Root widget id for the card grid.
    root_id: WidgetId,
    /// Cards laid out in a 3×3 grid in logical coordinates.
    cards: [Card; 9],
    /// Stroke style used for the focus ring.
    stroke_id: StrokeId,
    /// Geometry and stroke resources referenced by the display list.
    resources: DisplayResources,
}

impl ResourceResolver for FocusDemoScene {
    fn path(&self, id: PathId) -> Option<BezPath> {
        self.resources.path(id).cloned()
    }

    fn image(&self, _id: ImageId) -> Option<ImageBrush> {
        None
    }

    fn stroke(&self, id: StrokeId) -> Option<kurbo::Stroke> {
        self.resources.stroke(id).cloned()
    }

    fn paint(&self, id: PaintId) -> Option<Brush> {
        // Focus ring paint: a single shared white outline.
        if id == PaintId(1) {
            return Some(Brush::Solid(palette::css::WHITE));
        }

        // Map paint ids back to card + variant and derive a color from the
        // card's base color. The ids themselves are chosen when building the
        // display list.
        for card in &self.cards {
            if id == card.paint_base {
                return Some(Brush::Solid(card.base_color.with_alpha(0.6)));
            }
            if id == card.paint_hover {
                return Some(Brush::Solid(card.base_color.with_alpha(0.8)));
            }
            if id == card.paint_focus {
                return Some(Brush::Solid(card.base_color.with_alpha(1.0)));
            }
        }

        None
    }

    fn clip_path(&self, _id: ClipId) -> Option<BezPath> {
        None
    }
}

fn build_demo_scene() -> FocusDemoScene {
    // Lay out nine cards in a 3×3 grid in logical space.
    let card_w = 140.0;
    let card_h = 80.0;
    let h_gap = 20.0;
    let v_gap = 16.0;
    let left = 40.0;
    let top = 32.0;

    let mut resources = DisplayResources::new();

    let cards: [Card; 9] = std::array::from_fn(|i| {
        let row = i / 3;
        let col = i % 3;
        let x0 = left + (card_w + h_gap) * col as f64;
        let y0 = top + (card_h + v_gap) * row as f64;
        let rect = Rect::new(x0, y0, x0 + card_w, y0 + card_h);

        let base_color = match i {
            // Row 1.
            0 => palette::css::DEEP_SKY_BLUE,
            1 => palette::css::CORAL,
            2 => palette::css::MEDIUM_SEA_GREEN,
            // Row 2.
            3 => palette::css::GOLD,
            4 => palette::css::ORANGE,
            5 => palette::css::TURQUOISE,
            // Row 3.
            6 => palette::css::MEDIUM_ORCHID,
            7 => palette::css::PLUM,
            8 => palette::css::GREEN_YELLOW,
            _ => palette::css::GRAY,
        };

        // Allocate ids for this card's geometry and paints.
        let path_id = resources.add_rect_path(rect);
        let base = PaintId(10 + (i as u32) * 3);
        let hover = PaintId(base.0 + 1);
        let focus = PaintId(base.0 + 2);

        Card {
            id: i as WidgetId + 1,
            rect,
            base_color,
            path_id,
            paint_base: base,
            paint_hover: hover,
            paint_focus: focus,
        }
    });

    let stroke_id = resources.add_stroke(kurbo::Stroke::new(3.0));

    FocusDemoScene {
        root_id: 0,
        cards,
        stroke_id,
        resources,
    }
}

struct Demo {
    scene: FocusDemoScene,
    hover: HoverState<WidgetId>,
    focus: FocusState<WidgetId>,
}

impl VelloDemo for Demo {
    fn window_title(&self) -> &'static str {
        "Understory Focus + Display + Vello"
    }

    fn initial_logical_size(&self) -> (f64, f64) {
        (520.0, 360.0)
    }

    fn handle_pointer_event(&mut self, e: PointerEvent) {
        use ui_events::pointer::PointerEvent::*;

        match e {
            Move(update) => {
                let pos = update.current.logical_position();
                let hit = hit_test(pos.x, pos.y, &self.scene.cards);
                if let Some(card_id) = hit {
                    let path = [self.scene.root_id, card_id];
                    self.hover.update_path(&path);
                } else {
                    self.hover.clear();
                }
            }
            Down(button) => {
                let pos = button.state.logical_position();
                let hit = hit_test(pos.x, pos.y, &self.scene.cards);
                if let Some(card_id) = hit {
                    let path = [self.scene.root_id, card_id];
                    // Update both hover and focus to the clicked card.
                    self.hover.update_path(&path);
                    self.focus.update_path(&path);
                }
            }
            Leave(_) | Cancel(_) => {
                self.hover.clear();
            }
            _ => {}
        }
    }

    fn handle_keyboard_event(&mut self, ev: ui_events::keyboard::KeyboardEvent) {
        if ev.state != KeyState::Down {
            return;
        }

        let nav = match ev.key {
            Key::Named(NamedKey::Tab) => {
                if ev.modifiers.shift() {
                    Navigation::Prev
                } else {
                    Navigation::Next
                }
            }
            Key::Named(NamedKey::ArrowLeft) => Navigation::Left,
            Key::Named(NamedKey::ArrowRight) => Navigation::Right,
            Key::Named(NamedKey::ArrowUp) => Navigation::Up,
            Key::Named(NamedKey::ArrowDown) => Navigation::Down,
            _ => return,
        };

        // Build a FocusSpace over the nine cards using widget ids.
        let mut entries = Vec::with_capacity(self.scene.cards.len());
        for card in &self.scene.cards {
            entries.push(FocusEntry {
                id: card.id,
                rect: card.rect,
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            });
        }
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy {
            wrap: WrapMode::Scope,
        };

        // Origin is the currently focused leaf, or the first card.
        let origin = self
            .focus
            .current_path()
            .last()
            .copied()
            .unwrap_or(self.scene.cards[0].id);

        if let Some(next) = policy.next(origin, nav, &space) {
            let path = [self.scene.root_id, next];
            self.focus.update_path(&path);
        }
    }

    fn rebuild_scene(&mut self, scene: &mut Scene, scale_factor: f64) {
        // Build a fresh display list each frame based on hover/focus state.
        let hovered_leaf = self.hover.current_path().last().copied();
        let focused_leaf = self.focus.current_path().last().copied();

        let mut builder = DisplayListBuilder::new(GroupId(0));
        for card in &self.scene.cards {
            let paint_id = if Some(card.id) == focused_leaf {
                card.paint_focus
            } else if Some(card.id) == hovered_leaf {
                card.paint_hover
            } else {
                card.paint_base
            };

            // Base card.
            builder.fill_path(0, card.rect, card.path_id, paint_id, None);

            // Focus ring: only draw when focused.
            if Some(card.id) == focused_leaf {
                builder.stroke_path(
                    1,
                    card.rect,
                    card.path_id,
                    self.scene.stroke_id,
                    PaintId(1),
                    None,
                );
            }
        }

        let list = builder.finish();
        let xf = Affine::scale(scale_factor);
        record_scene(&list, &self.scene, scene, xf);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene = build_demo_scene();
    let demo = Demo {
        scene,
        hover: HoverState::new(),
        focus: FocusState::new(),
    };
    let mut app = VelloWinitApp::new(demo);

    let event_loop = EventLoop::new()?;
    event_loop
        .run_app(&mut app)
        .expect("Couldn't run event loop");
    Ok(())
}

fn hit_test(x: f64, y: f64, cards: &[Card; 9]) -> Option<WidgetId> {
    for card in cards {
        let r = card.rect;
        if x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1 {
            return Some(card.id);
        }
    }
    None
}
