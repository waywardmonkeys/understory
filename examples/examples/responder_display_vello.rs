// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Full stack demo: box tree + responder + display list + Vello.
//!
//! This example builds a tiny box tree scene, uses the responder to derive
//! hover state from hits, builds an `understory_display::DisplayList`, and
//! renders it via `understory_display_vello` into a Vello `Scene`.
//!
//!
//! Run:
//! - `cargo run -p understory_examples --example responder_display_vello`

use core::cell::Cell;
use std::collections::HashMap;

use kurbo::{Affine, BezPath, Point, Rect};
use ui_events::pointer::PointerEvent;
use understory_box_tree::{LocalNode, NodeFlags, NodeId, QueryFilter, Tree};
use understory_display::{
    ClipId, DisplayList, DisplayListBuilder, DisplayPainter, GroupId, ImageId, PaintId, PathId,
    StrokeId,
};
use understory_display_vello::{ResourceResolver, record_scene};
use understory_examples::display_resources::DisplayResources;
use understory_examples::vello_winit::{VelloDemo, VelloWinitApp};
use understory_responder::adapters::box_tree::top_hit_for_point;
use understory_responder::hover::{HoverState, path_from_dispatch};
use understory_responder::router::Router;
use understory_responder::types::{ParentLookup, WidgetLookup};
use vello::Scene;
use vello::peniko::{Brush, Color, ImageBrush};
use winit::event_loop::EventLoop;

/// Static scene description: three nodes in a box tree + render info.
#[derive(Clone)]
struct SceneSlots {
    slots: Vec<Slot>,
}

#[derive(Clone)]
struct Slot {
    node: NodeId,
    rect: Rect,
}

/// Resources shared between hit testing and display recording.
struct DemoResources {
    slots: SceneSlots,
    /// Node currently hovered (leaf of the hover path), if any.
    hover_leaf: Cell<Option<NodeId>>,
    /// Geometry resources referenced by the display list.
    resources: DisplayResources,
}

impl ResourceResolver for DemoResources {
    fn path(&self, id: PathId) -> Option<BezPath> {
        self.resources.path(id).cloned()
    }

    fn image(&self, _id: ImageId) -> Option<ImageBrush> {
        self.resources.image(_id).cloned()
    }

    fn stroke(&self, _id: StrokeId) -> Option<kurbo::Stroke> {
        self.resources.stroke(_id).cloned()
    }

    fn paint(&self, id: PaintId) -> Option<Brush> {
        match id.0 {
            // Main cards.
            1 | 2 | 3 => {
                let idx = (id.0 as usize).wrapping_sub(1);
                let slot = self.slots.slots.get(idx)?;
                let hovered = self.hover_leaf.get() == Some(slot.node);
                let base = match idx {
                    0 => {
                        if hovered {
                            Color::from_rgba8(0x66, 0xcc, 0xff, 0xff)
                        } else {
                            Color::from_rgba8(0x33, 0x99, 0xff, 0x99)
                        }
                    }
                    1 => {
                        if hovered {
                            Color::from_rgba8(0xff, 0xaa, 0x66, 0xff)
                        } else {
                            Color::from_rgba8(0xff, 0x88, 0x33, 0x99)
                        }
                    }
                    2 => {
                        if hovered {
                            Color::from_rgba8(0x7f, 0xe0, 0x7f, 0xff)
                        } else {
                            Color::from_rgba8(0x4c, 0xc9, 0x4c, 0x99)
                        }
                    }
                    _ => return None,
                };
                Some(Brush::Solid(base))
            }
            // Accent stripes: slightly lighter accent per card, more opaque on hover.
            11 | 12 | 13 => {
                let idx = (id.0 as usize).wrapping_sub(11);
                let slot = self.slots.slots.get(idx)?;
                let hovered = self.hover_leaf.get() == Some(slot.node);
                let base = match idx {
                    0 => {
                        if hovered {
                            Color::from_rgba8(0xb3, 0xe5, 0xff, 0xff)
                        } else {
                            Color::from_rgba8(0x80, 0xc7, 0xff, 0xcc)
                        }
                    }
                    1 => {
                        if hovered {
                            Color::from_rgba8(0xff, 0xd1, 0xa3, 0xff)
                        } else {
                            Color::from_rgba8(0xff, 0xb8, 0x7a, 0xcc)
                        }
                    }
                    2 => {
                        if hovered {
                            Color::from_rgba8(0xb2, 0xff, 0xb2, 0xff)
                        } else {
                            Color::from_rgba8(0x8d, 0xf0, 0x8d, 0xcc)
                        }
                    }
                    _ => return None,
                };
                Some(Brush::Solid(base))
            }
            _ => None,
        }
    }

    fn clip_path(&self, _id: ClipId) -> Option<BezPath> {
        self.resources.clip(_id).cloned()
    }
}

/// Track parent relationships for NodeId so the responder can reconstruct paths.
struct Parents {
    map: HashMap<NodeId, NodeId>,
}

impl ParentLookup<NodeId> for Parents {
    fn parent_of(&self, node: &NodeId) -> Option<NodeId> {
        self.map.get(node).copied()
    }
}

struct Lookup;
impl WidgetLookup<NodeId> for Lookup {
    type WidgetId = NodeId;
    fn widget_of(&self, n: &NodeId) -> Option<NodeId> {
        Some(*n)
    }
}

/// Build a simple box tree scene: three overlapping children under a root.
fn build_box_tree() -> (Tree, SceneSlots, Parents) {
    let mut tree = Tree::new();

    // Root node.
    let root_local = LocalNode {
        local_bounds: Rect::new(0.0, 0.0, 400.0, 300.0),
        local_transform: Affine::IDENTITY,
        local_clip: None,
        z_index: 0,
        flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
    };
    let root = tree.insert(None, root_local);

    let rects = [
        Rect::new(30.0, 50.0, 150.0, 150.0),
        Rect::new(130.0, 50.0, 250.0, 150.0),
        Rect::new(230.0, 50.0, 350.0, 150.0),
    ];

    let mut parents = HashMap::new();
    let mut slots = Vec::new();

    for rect in rects {
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: rect,
                local_transform: Affine::IDENTITY,
                local_clip: None,
                z_index: 0,
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
            },
        );
        parents.insert(child, root);
        slots.push(Slot { node: child, rect });
    }

    let _ = tree.commit();

    (tree, SceneSlots { slots }, Parents { map: parents })
}

struct FullStackDemo {
    tree: Tree,
    slots: SceneSlots,
    resources: DemoResources,
    router: Router<NodeId, Lookup, Parents>,
    hover: HoverState<NodeId>,
}

fn build_display_list(slots: &SceneSlots, resources: &mut DisplayResources) -> DisplayList {
    let mut b = DisplayListBuilder::new(GroupId(0));

    for (i, slot) in slots.slots.iter().enumerate() {
        // Card path reused between fill and hover highlighting.
        let card_path = resources.add_rect_path(slot.rect);
        let card_paint = PaintId((i + 1) as u32);

        // Accent stripe near the bottom of the card.
        let r = slot.rect;
        let stripe_bounds = Rect::new(r.x0 + 8.0, r.y1 - 24.0, r.x1 - 8.0, r.y1 - 12.0);
        let stripe_path = resources.add_rect_path(stripe_bounds);
        let stripe_paint = PaintId(11 + i as u32);

        // Base card fill.
        if i == 1 {
            // Middle slot: clip to its top half to demonstrate clips.
            let clip = Rect::new(r.x0, r.y0, r.x1, (r.y0 + r.y1) * 0.5);
            let clip_id = resources.add_clip_rect(clip);
            b.push_clip(0, slot.rect, clip_id, None);
            b.fill_path(0, slot.rect, card_path, card_paint, None);
            b.pop_clip(0, slot.rect, None);
        } else {
            b.fill_path(0, slot.rect, card_path, card_paint, None);
        }

        // Accent stripe.
        b.fill_path(1, stripe_bounds, stripe_path, stripe_paint, None);
    }

    b.finish()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tree, slots, parents) = build_box_tree();
    let resources = DemoResources {
        slots: SceneSlots {
            slots: slots.slots.clone(),
        },
        hover_leaf: Cell::new(None),
        resources: DisplayResources::new(),
    };
    let router: Router<NodeId, Lookup, Parents> = Router::with_parent(Lookup, parents);
    let demo = FullStackDemo {
        tree,
        slots,
        resources,
        router,
        hover: HoverState::new(),
    };

    let mut app = VelloWinitApp::new(demo);

    let event_loop = EventLoop::new()?;
    event_loop
        .run_app(&mut app)
        .expect("Couldn't run event loop");
    Ok(())
}

impl VelloDemo for FullStackDemo {
    fn window_title(&self) -> &'static str {
        "Understory Full Stack: Responder + Display + Vello"
    }

    fn initial_logical_size(&self) -> (f64, f64) {
        (400.0, 300.0)
    }

    fn handle_pointer_event(&mut self, e: PointerEvent) {
        match e {
            PointerEvent::Move(update) => {
                let pos = update.current.logical_position();
                let pt = Point::new(pos.x, pos.y);
                let filter = QueryFilter::new().visible().pickable();
                if let Some(hit) = top_hit_for_point(&self.tree, pt, filter) {
                    let dispatch = self.router.handle_with_hits(&[hit]);
                    let path = path_from_dispatch(&dispatch);
                    self.hover.update_path(&path);
                    let leaf = path.last().copied();
                    self.resources.hover_leaf.set(leaf);
                } else {
                    self.hover.update_path(&[]);
                    self.resources.hover_leaf.set(None);
                }
            }
            PointerEvent::Leave(_) | PointerEvent::Cancel(_) => {
                self.hover.update_path(&[]);
                self.resources.hover_leaf.set(None);
            }
            _ => {}
        }
    }

    fn rebuild_scene(&mut self, scene: &mut Scene, scale_factor: f64) {
        // Clear per-frame geometry cache and rebuild paths for cards/stripes.
        self.resources.resources = DisplayResources::new();
        let list = build_display_list(&self.slots, &mut self.resources.resources);
        let xf = Affine::scale(scale_factor);
        record_scene(&list, &self.resources, scene, xf);
    }
}
