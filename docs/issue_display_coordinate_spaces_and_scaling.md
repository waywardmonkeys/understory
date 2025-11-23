# Display Coordinate Spaces, Scaling, and Snapping

Status: design note / in-progress thinking.

Owner: understory maintainers.

This note captures current thinking about where the “logical → physical” boundary should live for rendering, and how that interacts with pixel snapping and multi-monitor DPI changes. It is not a final design, but a set of constraints and preferences to keep in mind as we evolve `understory_display` and the Vello adapters.

## Current default: logical display lists, transformed at record time

Right now:

- Layout, box tree, responder, and `understory_display::DisplayList` all operate in a **logical coordinate space** (logical pixels).
- `understory_display_vello::record_scene` takes an additional `Affine` transform parameter which is typically:
  - `Affine::scale(window.scale_factor() as f64, window.scale_factor() as f64)`.
- The Vello adapter applies this transform when recording ops, so Vello sees device/physical coordinates while the display list and hit-testing stay in logical coordinates.

This gives us:

- A display list that does not need to be rebuilt just because the window moves between monitors with different scale factors.
- A single place to plug in backend- and device-specific transforms (including scale, translation, rotation, etc.).

## Why the boundary is tricky

Two real-world pressures make this boundary non-trivial:

1. **Pixel snapping and rasterization details**
   - Crisp 1‑pixel lines, sharp text baselines, and predictable hit boxes often require knowledge of:
     - The scale factor and device’s pixel grid.
     - How the backend rasterizes strokes and fills (e.g., half‑pixel offsets).
   - Those concerns *naturally* live close to the backend, not in layout or the logical display list.

2. **Multi-monitor / dynamic scaling**
   - Moving a window between monitors with different scale factors should not require rebuilding the layout or the entire display list.
   - However, snapping decisions and sometimes even glyph positioning *do* need to respond to the current device-scale and transform.

This leads to a tension:

- Too high in the stack (e.g., baking scale into layout or display lists):
  - You lose the ability to reuse the same display list across devices.
  - You may need to rebuild everything on every DPI change.
- Too low in the stack (e.g., only in the GPU shader):
  - You make it hard for higher layers to reason about bounds and hit-testing.

## Preferred layering (working model)

The bias in Understory is:

- **Logical space** for:
  - Layout and box tree geometry.
  - Hit-testing and responder routing.
  - `understory_display::DisplayList` bounds and semantics.
  - Damage computation and culling.

- **Device space** for:
  - Actual rasterization (Vello, CPU rasterizers, etc.).
  - Pixel snapping decisions.
  - Per-device transforms (scale, translation, possibly rotation).

The “bridge” between the two is:

- A **per-frame device transform** supplied by the backend adapter:
  - For Vello, this is the `Affine` passed to `record_scene`, typically incorporating the DPI scale.
  - Other backends (CPU, AnyRender, etc.) can use the same pattern.

Snapping should be treated as a **backend responsibility**:

- Backends are free to:
  - Adjust positions in device space before rasterization (e.g., snapping to integer pixels).
  - Fold snapping into the device transform if that proves convenient.
- Higher layers should not assume that device-space coordinates are simply “logical * scale”; they should treat the adapter’s transform as the authoritative mapping.

## Implications and open questions

- Display lists and hit-testing:
  - All bounds (`OpHeader::bounds`) and hit tests should remain in logical coordinates.
  - Damage rectangles are expressed in logical space and are mapped to device space by the backend transform at render time.

- Snapping and text:
  - Text shaping and layout may remain mostly in logical space, but where and how to snap glyphs for crisp rendering is backend-specific and may evolve with Vello’s text and filter APIs.

- Multi-monitor moves:
  - When the window’s scale factor changes, the preferred flow is:
    - Keep layout, box tree, and display list in logical space.
    - Update the device transform used by the backend (e.g., new scale).
    - Let snapping and rasterization adjust per-frame based on that transform.

Open questions:

- Do we ever want a “device-aware” variant of bounds or snapping hints in the display list, or is that better left to backends entirely?
- How should we expose “device transform + snapping policy” in a way that can be shared between multiple backends (Vello, CPU, future AnyRender integration) without overcommitting to one renderer’s details?

## Prior art: how other systems handle this

Several existing systems follow a similar pattern of “logical space for layout, device space for rasterization and snapping”:

- **WPF**
  - Uses device-independent units (1/96") for layout and hit testing; all geometry is specified in logical space.
  - The composition target applies a DPI-based transform to map logical → device pixels.
  - Pixel snapping:
    - `UseLayoutRounding` rounds layout results to device pixels.
    - `SnapsToDevicePixels` hints that edges/strokes should align to pixel boundaries.
  - Moving a window between monitors updates the composition transform; layout stays in logical units, but snapping and rasterization change because the device transform changed.

- **Flutter**
  - Widgets and layout measure in logical pixels; `MediaQuery.devicePixelRatio` exposes the scale factor.
  - The engine (Skia) applies the device transform when drawing; pointer coordinates are also logical.
  - Snapping is largely handled in the renderer; authors nudge coordinates or rely on Skia conventions when they need crisp single-pixel strokes.
  - Changing `devicePixelRatio` (e.g. via window move or zoom) updates the transform; logical layout is reused.

- **Web / CSS / Canvas**
  - CSS layout and DOM hit testing are in CSS pixels (logical).
  - The browser maps CSS pixels to device pixels per surface using `devicePixelRatio`.
  - For `<canvas>`, the recommended pattern is: set the backing store size to `css_size * devicePixelRatio`, set a scale on the context, and draw in logical coordinates.
  - Pixel snapping is mostly a renderer concern; authors may adjust coordinates or use hints (`shape-rendering`, etc.) when necessary.

- **Cocoa (iOS / macOS)**
  - Uses points (logical units) for views and hit testing.
  - Views know their `contentScaleFactor`; Core Animation / Core Graphics apply the scale to device pixels.
  - Snapping and antialiasing are handled near the renderer; app code can round rects/lines when it cares about exact pixel alignment.

These examples reinforce the layering we are aiming for:

- Keep layout, hit testing, and display lists in logical space.
- Provide a per-surface or per-frame device transform to backends.
- Let backends apply snapping in device space, using that transform.
- Respond to DPI changes by updating the transform, not by rebuilding logical state.

For now, the plan is:

- Keep display list semantics firmly in logical space.
- Require backends to accept a per-frame device transform and own snapping decisions.
- Revisit this note once Vello’s snapping/text/filter stories are more fully developed, and we have real multi-monitor usage feedback.
