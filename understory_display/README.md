# Understory Display List

`understory_display` defines a small, `no_std` display list that higher layers
can build and diff between frames. It is intended to be the common render
target for UI widgets, SVG‑like content, and other 2D scenes in Understory.

The crate does **not** perform any rendering itself; adapters such as
`understory_display_vello` consume a `DisplayList` and turn it into backend‑
specific draw commands.

## Core ideas

- A render operation is represented by [`Op`](src/ops.rs) with a stable [`OpId`](src/ids.rs),
  group id, z value, optional semantic region id, and a bounding [`Rect`](https://docs.rs/kurbo/latest/kurbo/struct.Rect.html).
- Ops are collected into a [`DisplayList`](src/list.rs), which you can diff
  against another list using [`diff`](src/diff_impl.rs) to obtain a [`Diff`](src/diff_impl.rs)
  describing inserts, removes, moves, and replacements.
- A coarse damage API lets you ask which ops intersect a set of dirty rectangles
  via [`DisplayList::culled_by_damage`](src/list.rs).
- A [`ResourceSnapshot`](src/list.rs) lists referenced glyph runs and images so
  backends can manage their own caches.

The types are simple POD structs and enums, designed to be easy to log, test,
and move between threads.

## Why a display list?

Higher layers could in principle draw directly into a backend (for example
Vello), but a recorded, diffable display list has a few concrete advantages:

- **Incremental updates** – Stable op ids and `diff` let backends update GPU
  state or caches incrementally instead of rebuilding everything every frame.
- **Damage‑driven rendering** – Per‑op bounds and `culled_by_damage` make it
  easy to combine geometry damage (for example from a box tree) with repaint
  decisions.
- **Backend independence** – A single `DisplayList` can feed multiple adapters
  (Vello, CPU renderer, screenshot tool, etc.) without callers knowing about
  concrete GPU types.
- **Debugging and tests** – Recorded ops are easy to log, snapshot, and assert
  against in golden tests, which is much harder with “fire‑and‑forget”
  drawing APIs.

If other ecosystems grow their own recording abstractions (for example
AnyRender’s recorders, or a future `vello_api` with a recordable scene format),
`understory_display` is intended to stay compatible at the conceptual level: a
flat sequence of paint ops with ids, bounds, and grouping/clip structure. In
practice, diffability and damage integration depend on having stable
identifiers, explicit bounds, and visible insert/remove/move/rewrite
operations; if an external recorder exposes those invariants, it should be
possible to build adapters between that representation and `DisplayList`.

## Using the builder

Most callers construct a display list via [`DisplayListBuilder`](src/list.rs)
and the [`DisplayPainter`](src/list.rs) trait:

```rust
use kurbo::Rect;
use understory_display::{
    DisplayListBuilder, DisplayPainter, GroupId, PaintId, PathId,
};

fn build_simple_list() -> understory_display::DisplayList {
    let rect = Rect::new(0.0, 0.0, 100.0, 50.0);

    // Group id is an opaque handle used for grouping ops.
    let mut builder = DisplayListBuilder::new(GroupId(0));

    // In a real app, `path_id` and `paint_id` would come from a resource arena.
    let path_id = PathId(1);
    let paint_id = PaintId(1);

    builder.fill_path(0, rect, path_id, paint_id, None);
    builder.finish()
}
```

The resulting `DisplayList` can then be passed to a backend adapter such as
`understory_display_vello` for rendering.

## Adapters and examples

- [`understory_display_vello`](../understory_display_vello/README.md) records a
  `DisplayList` into a Vello `Scene`.
- The `understory_examples` crate includes:
  - `display_vello_basics` – simple display list + Vello wiring.
  - `focus_display_vello` – focus navigation rendered via a display list.
  - `responder_display_vello` – full stack demo with box tree + responder +
    display list + Vello.

