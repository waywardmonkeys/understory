# Understory Display → Vello adapter

`understory_display_vello` is a small adapter crate that records an
[`understory_display::DisplayList`](../understory_display/README.md) into a
Vello [`vello::Scene`](https://docs.rs/vello/latest/vello/struct.Scene.html).

It does **not** create windows, surfaces, or GPU devices. Those responsibilities
live in host applications or in the `understory_examples` crate.

## Overview

- You build a `DisplayList` in **logical coordinates** (typically logical
  pixels).
- You implement [`ResourceResolver`](src/lib.rs) to provide paths, paints,
  strokes, images, and (optionally) clip shapes for the ids referenced in the
  list.
- You call [`record_scene`](src/lib.rs) with:
  - the display list,
  - your resolver,
  - a mutable Vello `Scene`,
  - an `Affine` transform that maps logical coordinates into the Vello scene
    (for example, scaling by the window’s scale factor).
- The adapter lowers `FillPath`, `StrokePath`, `Image`, `PushClip`/`PopClip`,
  and `Group` ops into Vello drawing commands and layers.
- `GlyphRun` ops are currently ignored; text will be routed through a dedicated
  text stack that resolves `RunId` into glyph runs.

## Resource resolution

`ResourceResolver` keeps the adapter independent of how geometry, paints, and
images are stored:

```rust
use kurbo::{BezPath, Stroke};
use understory_display::{ClipId, ImageId, PathId, StrokeId};
use understory_display_vello::ResourceResolver;
use vello::peniko::{Brush, ImageBrush};

struct MyResources { /* paths, strokes, images, paints ... */ }

impl ResourceResolver for MyResources {
    fn path(&self, id: PathId) -> Option<BezPath> { /* ... */ }
    fn image(&self, id: ImageId) -> Option<ImageBrush> { /* ... */ }
    fn stroke(&self, id: StrokeId) -> Option<Stroke> { /* ... */ }
    fn paint(&self, id: understory_display::PaintId) -> Option<Brush> { /* ... */ }
    fn clip_path(&self, id: ClipId) -> Option<BezPath> { /* ... */ }
}
```

In small demos, resources can be stored in simple `Vec` arenas keyed by ids. In
larger applications, this is where you would integrate glyph caches, image
atlases, or other backend–specific state.

## Recording a scene

To record a `DisplayList` into a Vello `Scene`, clear the scene, choose a
logical→device transform, and call `record_scene`:

```rust
use kurbo::Affine;
use understory_display::DisplayList;
use understory_display_vello::{record_scene, ResourceResolver};
use vello::Scene;

fn rebuild_scene(
    list: &DisplayList,
    resources: &impl ResourceResolver,
    scene: &mut Scene,
    scale_factor: f64,
) {
    scene.reset();
    let xf = Affine::scale(scale_factor);
    record_scene(list, resources, scene, xf);
}
```

The resulting `Scene` can be rendered using `vello::Renderer::render_to_texture`
or the higher‑level helpers in `vello::util`.

## Examples

The `understory_examples` crate includes two end‑to‑end demos that exercise this
adapter:

- `display_vello_basics` – minimal display list + Vello example with hover
  feedback.
- `responder_display_vello` – full stack demo wiring box tree + responder +
  display list + Vello, including hit testing and hover routing.

Both examples also show how to use `ui-events-winit` to translate `winit`
events into the `ui-events` model and drive interaction in logical
coordinates.

