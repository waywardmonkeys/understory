// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Windowed angle guide demo using `understory_guide`, `parley`, and `imaging`.
//!
//! Run:
//! - `cargo run -p understory_examples --example angle_guide_demo`

use std::error::Error;
use std::f64::consts::{FRAC_PI_4, PI};
use std::sync::Arc;

use imaging::record::{Glyph, Scene};
use imaging::{PaintSink, Painter, TextureRenderer};
use imaging_vello_hybrid::{TextureTarget, VelloHybridTargetRenderer, wgpu};
use kurbo::{Affine, Circle, Line, Point, Rect, RoundedRect, Stroke};
use parley::{
    Alignment, AlignmentOptions, FontContext, GenericFamily, Layout, LayoutContext, LineHeight,
    PositionedLayoutItem, StyleProperty,
};
use peniko::{Brush, Color, Fill, Style};
use understory_guide::{AngleGuide2D, AngleGuideHit};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const WINDOW_WIDTH: u32 = 1180;
const WINDOW_HEIGHT: u32 = 820;
const PANEL_WIDTH: f64 = 280.0;
const HANDLE_RADIUS: f64 = 11.0;
const RAY_HIT_RADIUS: f64 = 11.0;
const MIN_RAY_LENGTH: f64 = 52.0;
const SNAP_INCREMENT_RAD: f64 = PI / 12.0;

const BG: u32 = 0x0D131BFF;
const PANEL_BG: u32 = 0x141C27F2;
const PANEL_STROKE: u32 = 0x314154FF;
const VERTEX_COLOR: u32 = 0xF8FAFCFF;
const START_RAY: u32 = 0x7DD3FCFF;
const END_RAY: u32 = 0xFCA5A5FF;
const ARC_COLOR: u32 = 0xFDE68AFF;
const BISECTOR: u32 = 0x94A3B8AA;
const HANDLE_HOVER: u32 = 0xF59E0BFF;
const TEXT_BRIGHT: u32 = 0xE2E8F0FF;
const TEXT_DIM: u32 = 0x94A3B8FF;
const SNAP_ON: u32 = 0x34D399FF;
const SNAP_OFF: u32 = 0x64748BFF;
const GUIDE_SHADOW: u32 = 0x02061766;

#[derive(Copy, Clone, Debug, PartialEq)]
enum Interaction {
    Idle,
    DragGuide { last_point: Point },
    DragStartHandle,
    DragEndHandle,
}

struct GpuState {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    blitter: wgpu::util::TextureBlitter,
    target_texture: wgpu::Texture,
    target_view: wgpu::TextureView,
    renderer: VelloHybridTargetRenderer,
}

#[derive(Debug)]
enum RenderState {
    Active { window: Arc<Window> },
    Suspended,
}

struct DemoText {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
}

impl DemoText {
    fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
        }
    }

    fn layout_label(
        &mut self,
        text: &str,
        font_size: f32,
        brush: Brush,
        max_advance: Option<f32>,
    ) -> Layout<Brush> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::Brush(brush));
        builder.push_default(GenericFamily::SystemUi);
        builder.push_default(LineHeight::FontSizeRelative(1.0));
        builder.push_default(StyleProperty::FontSize(font_size));

        let mut layout = builder.build(text);
        layout.break_all_lines(max_advance);
        layout.align(max_advance, Alignment::Start, AlignmentOptions::default());
        layout
    }

    fn draw_layout<S: PaintSink + ?Sized>(
        &self,
        painter: &mut Painter<'_, S>,
        layout: &Layout<Brush>,
        transform: Affine,
    ) {
        let style = Style::Fill(Fill::NonZero);
        for line in layout.lines() {
            for item in line.items() {
                match item {
                    PositionedLayoutItem::GlyphRun(glyph_run) => {
                        let run = glyph_run.run();
                        let glyphs = glyph_run.positioned_glyphs().map(|glyph| Glyph {
                            id: glyph.id,
                            x: glyph.x,
                            y: glyph.y,
                        });
                        painter
                            .glyphs(run.font(), &glyph_run.style().brush)
                            .transform(transform)
                            .font_size(run.font_size())
                            .hint(true)
                            .normalized_coords(run.normalized_coords())
                            .draw(&style, glyphs);
                    }
                    PositionedLayoutItem::InlineBox(_) => {}
                }
            }
        }
    }
}

struct AngleGuideApp {
    render_state: RenderState,
    renderer_error: Option<String>,
    gpu: Option<GpuState>,
    text: DemoText,
    guide: AngleGuide2D,
    hover: Option<AngleGuideHit>,
    interaction: Interaction,
    cursor_position: Option<Point>,
    snap_enabled: bool,
}

impl AngleGuideApp {
    fn new() -> Self {
        Self {
            render_state: RenderState::Suspended,
            renderer_error: None,
            gpu: None,
            text: DemoText::new(),
            guide: AngleGuide2D::new(
                Point::new(720.0, 430.0),
                -FRAC_PI_4,
                FRAC_PI_4,
                190.0,
                240.0,
            ),
            hover: None,
            interaction: Interaction::Idle,
            cursor_position: None,
            snap_enabled: false,
        }
    }

    fn window(&self) -> Option<&Arc<Window>> {
        match &self.render_state {
            RenderState::Active { window } => Some(window),
            RenderState::Suspended => None,
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = self.window() {
            window.request_redraw();
        }
    }

    fn reset_guide(&mut self, size: PhysicalSize<u32>) {
        let center = Point::new(
            PANEL_WIDTH + (f64::from(size.width) - PANEL_WIDTH) * 0.54,
            f64::from(size.height) * 0.56,
        );
        self.guide = AngleGuide2D::new(center, -FRAC_PI_4, FRAC_PI_4, 190.0, 240.0);
    }

    fn sync_window_title(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let hover = match self.hover {
            Some(AngleGuideHit::VertexHandle) => "vertex",
            Some(AngleGuideHit::StartHandle) => "start handle",
            Some(AngleGuideHit::EndHandle) => "end handle",
            Some(AngleGuideHit::StartRay) => "start ray",
            Some(AngleGuideHit::EndRay) => "end ray",
            None => "none",
        };
        let error = self
            .renderer_error
            .as_deref()
            .map(|message| format!(" | renderer: {message}"))
            .unwrap_or_default();
        window.set_title(&format!(
            "angle guide demo | angle: {} | hover: {} | snap: {}{}",
            format_degrees(self.guide.minor_angle_rad().to_degrees()),
            hover,
            if self.snap_enabled { "on" } else { "off" },
            error,
        ));
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        let width = size.width.max(1);
        let height = size.height.max(1);
        if let Some(gpu) = &mut self.gpu {
            let (target_texture, target_view) = create_render_target(&gpu.device, width, height);
            gpu.config.width = width;
            gpu.config.height = height;
            gpu.target_texture = target_texture;
            gpu.target_view = target_view;
            gpu.surface.configure(&gpu.device, &gpu.config);
        }
        self.sync_window_title();
    }

    fn redraw(&mut self) {
        let Some(window) = self.window() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let Some(window) = self.window().cloned() else {
            return;
        };
        let Some(gpu) = &mut self.gpu else {
            return;
        };

        let mut scene = build_scene(
            size,
            self.guide,
            self.hover,
            self.snap_enabled,
            &mut self.text,
        );

        let surface_texture = match gpu.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                match gpu.surface.get_current_texture() {
                    Ok(texture) => texture,
                    Err(error) => {
                        self.renderer_error = Some(format!("surface acquire {error:?}"));
                        self.sync_window_title();
                        return;
                    }
                }
            }
            Err(error) => {
                self.renderer_error = Some(format!("surface acquire {error:?}"));
                self.sync_window_title();
                return;
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        match gpu.renderer.render_source_to_texture(
            &mut scene,
            TextureTarget::new(&gpu.target_view, size.width, size.height),
        ) {
            Ok(()) => {
                let mut encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("angle guide demo surface blit"),
                        });
                gpu.blitter
                    .copy(&gpu.device, &mut encoder, &gpu.target_view, &view);
                gpu.queue.submit([encoder.finish()]);
                window.pre_present_notify();
                surface_texture.present();
                self.renderer_error = None;
            }
            Err(error) => {
                self.renderer_error = Some(format!("{error:?}"));
            }
        }
        self.sync_window_title();
    }

    fn hit_test(&self, point: Point) -> Option<AngleGuideHit> {
        self.guide
            .hit_test(point, RAY_HIT_RADIUS, HANDLE_RADIUS + 4.0)
    }

    fn set_hover(&mut self) {
        self.hover = self.cursor_position.and_then(|point| self.hit_test(point));
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        let point = Point::new(position.x, position.y);
        self.cursor_position = Some(point);

        match self.interaction {
            Interaction::Idle => self.set_hover(),
            Interaction::DragGuide { last_point } => {
                let delta = point - last_point;
                self.guide = AngleGuide2D::new(
                    self.guide.vertex() + delta,
                    self.guide.start_angle_rad(),
                    self.guide.end_angle_rad(),
                    self.guide.start_ray_length(),
                    self.guide.end_ray_length(),
                );
                self.interaction = Interaction::DragGuide { last_point: point };
                self.set_hover();
            }
            Interaction::DragStartHandle => {
                if let Some(next) =
                    drag_handle_guide(self.guide, point, true, self.snap_enabled, MIN_RAY_LENGTH)
                {
                    self.guide = next;
                }
                self.hover = Some(AngleGuideHit::StartHandle);
            }
            Interaction::DragEndHandle => {
                if let Some(next) =
                    drag_handle_guide(self.guide, point, false, self.snap_enabled, MIN_RAY_LENGTH)
                {
                    self.guide = next;
                }
                self.hover = Some(AngleGuideHit::EndHandle);
            }
        }
        self.sync_window_title();
        self.request_redraw();
    }

    fn on_mouse_input(&mut self, button: MouseButton, state: ElementState) {
        let Some(cursor) = self.cursor_position else {
            return;
        };

        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => match self.hit_test(cursor) {
                Some(
                    AngleGuideHit::VertexHandle | AngleGuideHit::StartRay | AngleGuideHit::EndRay,
                ) => {
                    self.interaction = Interaction::DragGuide { last_point: cursor };
                }
                Some(AngleGuideHit::StartHandle) => {
                    self.interaction = Interaction::DragStartHandle;
                }
                Some(AngleGuideHit::EndHandle) => {
                    self.interaction = Interaction::DragEndHandle;
                }
                None => {}
            },
            (MouseButton::Left, ElementState::Released) => {
                self.interaction = Interaction::Idle;
                self.set_hover();
            }
            _ => {}
        }
        self.sync_window_title();
        self.request_redraw();
    }

    fn on_keyboard(&mut self, key: &Key) {
        match key {
            Key::Character(ch) if ch.eq_ignore_ascii_case("s") => {
                self.snap_enabled = !self.snap_enabled;
            }
            Key::Character(ch) if ch.eq_ignore_ascii_case("r") => {
                if let Some(window) = self.window() {
                    self.reset_guide(window.inner_size());
                }
            }
            _ => return,
        }
        self.sync_window_title();
        self.request_redraw();
    }
}

impl ApplicationHandler for AngleGuideApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if matches!(self.render_state, RenderState::Active { .. }) {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("angle guide demo")
                        .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
                        .with_resizable(true),
                )
                .expect("create angle guide demo window"),
        );
        let size = window.inner_size();
        match init_gpu(window.clone(), size) {
            Ok(gpu) => {
                self.gpu = Some(gpu);
                self.renderer_error = None;
            }
            Err(error) => {
                self.gpu = None;
                self.renderer_error = Some(error);
            }
        }
        self.render_state = RenderState::Active { window };
        self.reset_guide(size);
        self.resize(size);
        self.request_redraw();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.render_state = RenderState::Suspended;
        self.gpu = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.resize(size);
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => self.on_cursor_moved(position),
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_input(button, state),
            WindowEvent::CursorLeft { .. } => {
                self.cursor_position = None;
                if matches!(self.interaction, Interaction::Idle) {
                    self.hover = None;
                }
                self.sync_window_title();
                self.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match logical_key {
                Key::Named(NamedKey::Escape) => event_loop.exit(),
                ref key => self.on_keyboard(key),
            },
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }
}

fn drag_handle_guide(
    guide: AngleGuide2D,
    point: Point,
    start_handle: bool,
    snap_enabled: bool,
    min_ray_length: f64,
) -> Option<AngleGuide2D> {
    let vertex = guide.vertex();
    let delta = point - vertex;
    let length = delta.hypot().max(min_ray_length);
    if length <= 0.0 {
        return None;
    }
    let mut angle = delta.atan2();
    if snap_enabled {
        angle = (angle / SNAP_INCREMENT_RAD).round() * SNAP_INCREMENT_RAD;
    }
    Some(if start_handle {
        AngleGuide2D::new(
            vertex,
            angle,
            guide.end_angle_rad(),
            length,
            guide.end_ray_length(),
        )
    } else {
        AngleGuide2D::new(
            vertex,
            guide.start_angle_rad(),
            angle,
            guide.start_ray_length(),
            length,
        )
    })
}

fn build_scene(
    size: PhysicalSize<u32>,
    guide: AngleGuide2D,
    hover: Option<AngleGuideHit>,
    snap_enabled: bool,
    text: &mut DemoText,
) -> Scene {
    let mut scene = Scene::new();
    let mut painter = Painter::new(&mut scene);
    let width = f64::from(size.width);
    let height = f64::from(size.height);

    fill_rect(&mut painter, Rect::new(0.0, 0.0, width, height), rgba(BG));
    fill_rect(
        &mut painter,
        Rect::new(0.0, 0.0, PANEL_WIDTH, height),
        rgba(PANEL_BG),
    );
    stroke_line(
        &mut painter,
        Point::new(PANEL_WIDTH, 0.0),
        Point::new(PANEL_WIDTH, height),
        1.0,
        rgba(PANEL_STROKE),
    );

    let shadow_offset = Point::new(6.0, 6.0) - Point::ZERO;
    stroke_line(
        &mut painter,
        guide.vertex() + shadow_offset,
        guide.start_point() + shadow_offset,
        5.0,
        rgba(GUIDE_SHADOW),
    );
    stroke_line(
        &mut painter,
        guide.vertex() + shadow_offset,
        guide.end_point() + shadow_offset,
        5.0,
        rgba(GUIDE_SHADOW),
    );

    let arc_radius =
        (guide.start_ray_length().min(guide.end_ray_length()) * 0.36).clamp(42.0, 108.0);
    let arc_points = sampled_arc_points(guide, arc_radius, 32);
    stroke_polyline(&mut painter, &arc_points, 3.0, rgba(ARC_COLOR));

    stroke_line(
        &mut painter,
        guide.vertex(),
        guide.start_point(),
        3.0,
        rgba(START_RAY),
    );
    stroke_line(
        &mut painter,
        guide.vertex(),
        guide.end_point(),
        3.0,
        rgba(END_RAY),
    );

    let bisector_end = guide.bisector_point(
        (arc_radius + 68.0).min(guide.start_ray_length().max(guide.end_ray_length())),
    );
    stroke_line(
        &mut painter,
        guide.vertex(),
        bisector_end,
        1.5,
        rgba(BISECTOR),
    );

    draw_handle(
        &mut painter,
        guide.vertex(),
        rgba(VERTEX_COLOR),
        matches!(hover, Some(AngleGuideHit::VertexHandle)),
    );
    draw_handle(
        &mut painter,
        guide.start_point(),
        rgba(START_RAY),
        matches!(hover, Some(AngleGuideHit::StartHandle)),
    );
    draw_handle(
        &mut painter,
        guide.end_point(),
        rgba(END_RAY),
        matches!(hover, Some(AngleGuideHit::EndHandle)),
    );

    if matches!(hover, Some(AngleGuideHit::StartRay)) {
        stroke_line(
            &mut painter,
            guide.vertex(),
            guide.start_point(),
            7.0,
            rgba(START_RAY).multiply_alpha(0.35),
        );
    }
    if matches!(hover, Some(AngleGuideHit::EndRay)) {
        stroke_line(
            &mut painter,
            guide.vertex(),
            guide.end_point(),
            7.0,
            rgba(END_RAY).multiply_alpha(0.35),
        );
    }

    let angle_label = format!("{}°", format_degrees(guide.minor_angle_rad().to_degrees()));
    let angle_layout = text.layout_label(&angle_label, 20.0, Brush::Solid(rgba(ARC_COLOR)), None);
    let angle_anchor = guide.bisector_point(arc_radius + 30.0);
    text.draw_layout(
        &mut painter,
        &angle_layout,
        Affine::translate((
            angle_anchor.x - f64::from(angle_layout.width()) * 0.5,
            angle_anchor.y - 12.0,
        )),
    );

    let panel_rect = Rect::new(18.0, 18.0, PANEL_WIDTH - 18.0, height - 18.0);
    fill_rounded_rect(
        &mut painter,
        panel_rect,
        16.0,
        rgba(PANEL_BG).multiply_alpha(1.04),
    );
    stroke_rounded_rect(&mut painter, panel_rect, 16.0, 1.0, rgba(PANEL_STROKE));

    let title = text.layout_label("Angle Guide", 18.0, Brush::Solid(rgba(TEXT_BRIGHT)), None);
    text.draw_layout(&mut painter, &title, Affine::translate((34.0, 34.0)));

    let snap_color = if snap_enabled {
        rgba(SNAP_ON)
    } else {
        rgba(SNAP_OFF)
    };
    fill_rounded_rect(
        &mut painter,
        Rect::new(34.0, 68.0, 128.0, 94.0),
        12.0,
        snap_color.multiply_alpha(0.18),
    );
    let badge = text.layout_label(
        if snap_enabled {
            "Snap 15°"
        } else {
            "Free Angle"
        },
        12.0,
        Brush::Solid(snap_color),
        None,
    );
    text.draw_layout(&mut painter, &badge, Affine::translate((48.0, 75.0)));

    let info_lines = [
        String::from("Left-drag vertex: move guide"),
        String::from("Left-drag endpoints: rotate rays"),
        String::from("Left-drag ray body: move guide"),
        String::from("S: toggle 15° snapping"),
        String::from("R: reset"),
        format!(
            "Minor angle  {}°",
            format_degrees(guide.minor_angle_rad().to_degrees())
        ),
        format!("Start ray  {} px", guide.start_ray_length().round()),
        format!("End ray  {} px", guide.end_ray_length().round()),
    ];
    let mut y = 112.0;
    for line in info_lines {
        let layout = text.layout_label(&line, 12.0, Brush::Solid(rgba(TEXT_DIM)), None);
        text.draw_layout(&mut painter, &layout, Affine::translate((34.0, y)));
        y += 22.0;
    }

    let hint = text.layout_label(
        "This example does not use understory_axis.\nIt exists to prove understory_guide can carry non-axis geometry cleanly.",
        12.0,
        Brush::Solid(rgba(TEXT_DIM)),
        Some((PANEL_WIDTH - 68.0) as f32),
    );
    text.draw_layout(
        &mut painter,
        &hint,
        Affine::translate((34.0, panel_rect.y1 - 86.0)),
    );

    scene
}

fn sampled_arc_points(guide: AngleGuide2D, radius: f64, segments: usize) -> Vec<Point> {
    let segments = segments.max(2);
    (0..=segments)
        .map(|index| guide.point_on_minor_arc(index as f64 / segments as f64, radius))
        .collect()
}

fn draw_handle<S: PaintSink + ?Sized>(
    painter: &mut Painter<'_, S>,
    center: Point,
    fill: Color,
    hovered: bool,
) {
    let fill = if hovered { rgba(HANDLE_HOVER) } else { fill };
    painter
        .fill(Circle::new(center, HANDLE_RADIUS), &Brush::Solid(fill))
        .draw();
    painter
        .stroke(
            Circle::new(center, HANDLE_RADIUS),
            &Stroke::new(2.0),
            &Brush::Solid(rgba(PANEL_BG)),
        )
        .draw();
}

fn fill_rect<S: PaintSink + ?Sized>(painter: &mut Painter<'_, S>, rect: Rect, color: Color) {
    painter.fill_rect(rect, &Brush::Solid(color));
}

fn fill_rounded_rect<S: PaintSink + ?Sized>(
    painter: &mut Painter<'_, S>,
    rect: Rect,
    radius: f64,
    color: Color,
) {
    painter
        .fill(RoundedRect::from_rect(rect, radius), &Brush::Solid(color))
        .draw();
}

fn stroke_rounded_rect<S: PaintSink + ?Sized>(
    painter: &mut Painter<'_, S>,
    rect: Rect,
    radius: f64,
    width: f64,
    color: Color,
) {
    painter
        .stroke(
            RoundedRect::from_rect(rect, radius),
            &Stroke::new(width),
            &Brush::Solid(color),
        )
        .draw();
}

fn stroke_line<S: PaintSink + ?Sized>(
    painter: &mut Painter<'_, S>,
    start: Point,
    end: Point,
    width: f64,
    color: Color,
) {
    painter
        .stroke(
            Line::new(start, end),
            &Stroke::new(width),
            &Brush::Solid(color),
        )
        .draw();
}

fn stroke_polyline<S: PaintSink + ?Sized>(
    painter: &mut Painter<'_, S>,
    points: &[Point],
    width: f64,
    color: Color,
) {
    for segment in points.windows(2) {
        stroke_line(painter, segment[0], segment[1], width, color);
    }
}

fn rgba(rgba: u32) -> Color {
    let [r, g, b, a] = rgba.to_be_bytes();
    Color::from_rgba8(r, g, b, a)
}

fn format_degrees(value: f64) -> String {
    format!("{value:.1}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn init_gpu(window: Arc<Window>, size: PhysicalSize<u32>) -> Result<GpuState, String> {
    let instance = wgpu::Instance::default();
    let surface = instance
        .create_surface(window)
        .map_err(|error| format!("surface {error:?}"))?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
    }))
    .map_err(|error| format!("adapter {error:?}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("angle guide demo device"),
        required_features: wgpu::Features::empty(),
        ..Default::default()
    }))
    .map_err(|error| format!("device {error:?}"))?;

    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .into_iter()
        .find(|format| {
            matches!(
                *format,
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
            )
        })
        .ok_or_else(|| String::from("surface missing RGBA8/BGRA8 support"))?;
    let alpha_mode = caps
        .alpha_modes
        .first()
        .copied()
        .unwrap_or(wgpu::CompositeAlphaMode::Auto);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        desired_maximum_frame_latency: 2,
        alpha_mode,
        view_formats: vec![],
    };
    surface.configure(&device, &config);
    let (target_texture, target_view) =
        create_render_target(&device, size.width.max(1), size.height.max(1));
    let blitter = wgpu::util::TextureBlitter::new(&device, format);

    Ok(GpuState {
        _instance: instance,
        surface,
        device: device.clone(),
        queue: queue.clone(),
        config,
        blitter,
        target_texture,
        target_view,
        renderer: VelloHybridTargetRenderer::new(device, queue),
    })
}

fn create_render_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("angle guide demo render target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = AngleGuideApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
