// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Windowed axis ruler demo using `understory_axis`, `parley`, and `imaging`.
//!
//! Run:
//! - `cargo run -p understory_examples --example axis_ruler_demo`

use std::error::Error;
use std::ops::Range;
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
use understory_axis::{
    AxisMajorStepLadder, AxisMapping1D, AxisRuler1D, AxisRulerOptions, AxisScale1D,
    AxisScaleOptions, AxisSubdivisionPolicy, AxisTickKind,
};
use understory_guide::{AxisGuide2D, AxisGuideOptions, GuideHit, LineGuide2D};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const WINDOW_WIDTH: u32 = 1240;
const WINDOW_HEIGHT: u32 = 860;
const PANEL_WIDTH: f64 = 280.0;
const HANDLE_RADIUS: f64 = 11.0;
const BODY_HIT_RADIUS: f64 = 14.0;
const MIN_RULER_LENGTH: f64 = 180.0;
const DEFAULT_RULER_LENGTH: f64 = 720.0;
const RIGHT_DRAG_PAN_GAIN: f64 = 1.0;

const BG: u32 = 0x0E141BFF;
const PANEL_BG: u32 = 0x141C27F2;
const PANEL_STROKE: u32 = 0x314154FF;
const RULER_BASE: u32 = 0xDBEAFEFF;
const RULER_SECONDARY: u32 = 0x93C5FDFF;
const RULER_MINOR: u32 = 0x64748BFF;
const LABEL_TEXT: u32 = 0xE2E8F0FF;
const DIM_TEXT: u32 = 0x94A3B8FF;
const HANDLE_FILL: u32 = 0xF8FAFCFF;
const HANDLE_HOVER: u32 = 0xF59E0BFF;
const BASELINE_HOVER: u32 = 0x22D3EEFF;
const DOMAIN_GUIDE: u32 = 0x38BDF8FF;
const SHADOW: u32 = 0x02061766;
const LOG_BADGE: u32 = 0xA78BFAFF;
const LINEAR_BADGE: u32 = 0x34D399FF;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum AxisMode {
    Linear,
    Log,
}

impl AxisMode {
    fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Log => "Log",
        }
    }
}

#[derive(Clone, Debug)]
struct AxisDomainView {
    mode: AxisMode,
    linear_bounds: Range<f64>,
    linear_visible: Range<f64>,
    log_bounds: Range<f64>,
    log_visible: Range<f64>,
    log_base: f64,
}

impl AxisDomainView {
    fn new() -> Self {
        Self {
            mode: AxisMode::Linear,
            linear_bounds: 0.0..10_000.0,
            linear_visible: 0.0..2_000.0,
            log_bounds: 1.0..10_000.0,
            log_visible: 1.0..1_000.0,
            log_base: 10.0,
        }
    }

    fn mode(&self) -> AxisMode {
        self.mode
    }

    fn visible_domain(&self) -> Range<f64> {
        match self.mode {
            AxisMode::Linear => self.linear_visible.clone(),
            AxisMode::Log => self.log_visible.clone(),
        }
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            AxisMode::Linear => AxisMode::Log,
            AxisMode::Log => AxisMode::Linear,
        };
    }

    fn reset_visible(&mut self) {
        match self.mode {
            AxisMode::Linear => self.linear_visible = 0.0..2_000.0,
            AxisMode::Log => self.log_visible = 1.0..1_000.0,
        }
    }

    fn fit_bounds(&mut self) {
        match self.mode {
            AxisMode::Linear => self.linear_visible = self.linear_bounds.clone(),
            AxisMode::Log => self.log_visible = self.log_bounds.clone(),
        }
    }

    fn mapping(&self, view_span: Range<f64>) -> AxisMapping1D {
        match self.mode {
            AxisMode::Linear => AxisMapping1D::linear(view_span, self.linear_visible.clone()),
            AxisMode::Log => AxisMapping1D::log(view_span, self.log_visible.clone(), self.log_base),
        }
    }

    fn scale(&self, view_span: Range<f64>) -> AxisScale1D {
        AxisScale1D::from_mapping(&self.mapping(view_span), self.axis_options())
    }

    fn ruler(&self, view_span: Range<f64>) -> AxisRuler1D {
        let mapping = self.mapping(view_span);
        let scale = AxisScale1D::from_mapping(&mapping, self.axis_options());
        AxisRuler1D::from_mapping(
            &mapping,
            &scale,
            AxisRulerOptions {
                major_mark_extent: 22.0,
                medium_mark_extent: 16.0,
                minor_mark_extent: 10.0,
            },
        )
    }

    fn label_step(&self, view_span: Range<f64>) -> Option<f64> {
        self.scale(view_span).label_step()
    }

    fn axis_options(&self) -> AxisScaleOptions {
        AxisScaleOptions {
            target_major_spacing_px: 96.0,
            min_major_step: 0.0,
            medium_label_min_spacing_px: 220.0,
            major_step_ladder: AxisMajorStepLadder::Decimal125,
            subdivision_policy: AxisSubdivisionPolicy::Auto,
        }
    }

    fn zoom_about_view(&mut self, anchor_view: f64, factor: f64, view_span: Range<f64>) {
        if factor <= 0.0 {
            return;
        }

        let view_len = (view_span.end - view_span.start).max(f64::MIN_POSITIVE);
        let ratio = ((anchor_view - view_span.start) / view_len).clamp(0.0, 1.0);
        match self.mode {
            AxisMode::Linear => {
                let visible = self.linear_visible.clone();
                let bounds = self.linear_bounds.clone();
                let old_span = visible.end - visible.start;
                let new_span = (old_span / factor).clamp(1e-3, bounds.end - bounds.start);
                let anchor = visible.start + ratio * old_span;
                let start = anchor - ratio * new_span;
                self.linear_visible = clamp_linear_range(start..(start + new_span), bounds);
            }
            AxisMode::Log => {
                let visible = self.log_visible.clone();
                let bounds = self.log_bounds.clone();
                let old_span = log_range_span(&visible, self.log_base);
                let bounds_span = log_range_span(&bounds, self.log_base);
                let new_span = (old_span / factor).clamp(0.05, bounds_span);
                let anchor = view_ratio_to_log_value(&visible, ratio, self.log_base);
                let anchor_log = log_in_base(anchor, self.log_base);
                let start_log = anchor_log - ratio * new_span;
                self.log_visible = clamp_log_range(
                    log_range(start_log, new_span, self.log_base),
                    bounds,
                    self.log_base,
                );
            }
        }
    }

    fn pan_by_view(&mut self, delta_view: f64, view_span: Range<f64>) {
        let view_len = (view_span.end - view_span.start).max(f64::MIN_POSITIVE);
        match self.mode {
            AxisMode::Linear => {
                let visible = self.linear_visible.clone();
                let bounds = self.linear_bounds.clone();
                let domain_span = visible.end - visible.start;
                let delta_domain = -(delta_view / view_len) * domain_span;
                self.linear_visible = clamp_linear_range(
                    (visible.start + delta_domain)..(visible.end + delta_domain),
                    bounds,
                );
            }
            AxisMode::Log => {
                let visible = self.log_visible.clone();
                let bounds = self.log_bounds.clone();
                let log_span = log_range_span(&visible, self.log_base);
                let delta_log = -(delta_view / view_len) * log_span;
                let start_log = log_in_base(visible.start, self.log_base) + delta_log;
                self.log_visible = clamp_log_range(
                    log_range(start_log, log_span, self.log_base),
                    bounds,
                    self.log_base,
                );
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum Interaction {
    Idle,
    DragBody { last_point: Point },
    DragStartHandle { fixed_end: Point },
    DragEndHandle { fixed_start: Point },
    PanDomain { last_axis_view: f64 },
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

struct AxisRulerApp {
    render_state: RenderState,
    renderer_error: Option<String>,
    gpu: Option<GpuState>,
    text: DemoText,
    domain: AxisDomainView,
    pose: LineGuide2D,
    hover: Option<GuideHit>,
    interaction: Interaction,
    cursor_position: Option<Point>,
}

impl AxisRulerApp {
    fn new() -> Self {
        Self {
            render_state: RenderState::Suspended,
            renderer_error: None,
            gpu: None,
            text: DemoText::new(),
            domain: AxisDomainView::new(),
            pose: LineGuide2D::new(Point::new(760.0, 440.0), -0.34, DEFAULT_RULER_LENGTH),
            hover: None,
            interaction: Interaction::Idle,
            cursor_position: None,
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

    fn reset_pose(&mut self, size: PhysicalSize<u32>) {
        let center = Point::new(
            PANEL_WIDTH + (f64::from(size.width) - PANEL_WIDTH) * 0.54,
            f64::from(size.height) * 0.56,
        );
        let available_width = (f64::from(size.width) - PANEL_WIDTH - 120.0).max(MIN_RULER_LENGTH);
        self.pose = LineGuide2D::new(center, -0.34, available_width.min(DEFAULT_RULER_LENGTH));
    }

    fn axis_view_span(&self) -> Range<f64> {
        0.0..self.pose.length()
    }

    fn axis_mapping(&self) -> AxisMapping1D {
        self.domain.mapping(self.axis_view_span())
    }

    fn current_value_under_cursor(&self) -> Option<f64> {
        let cursor = self.cursor_position?;
        self.pose
            .hit_test(cursor, BODY_HIT_RADIUS, HANDLE_RADIUS + 4.0)?;
        let scalar = self
            .pose
            .project_view_position(cursor)
            .clamp(0.0, self.pose.length());
        Some(self.axis_mapping().view_to_domain(scalar))
    }

    fn sync_window_title(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let visible = self.domain.visible_domain();
        let hover = match self.hover {
            Some(GuideHit::Body) => "body",
            Some(GuideHit::StartHandle) => "start handle",
            Some(GuideHit::EndHandle) => "end handle",
            None => "none",
        };
        let cursor = self
            .current_value_under_cursor()
            .map(|value| format_value(value, self.domain.label_step(self.axis_view_span())))
            .unwrap_or_else(|| String::from("none"));
        let error = self
            .renderer_error
            .as_deref()
            .map(|message| format!(" | renderer: {message}"))
            .unwrap_or_default();
        window.set_title(&format!(
            "axis ruler demo | mode: {} | domain: {} -> {} | hover: {} | cursor: {}{}",
            self.domain.mode().label(),
            format_value(visible.start, self.domain.label_step(self.axis_view_span())),
            format_value(visible.end, self.domain.label_step(self.axis_view_span())),
            hover,
            cursor,
            error
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
            &self.domain,
            self.pose,
            self.hover,
            self.cursor_position,
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
                            label: Some("axis ruler demo surface blit"),
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

    fn hit_test(&self, point: Point) -> Option<GuideHit> {
        self.pose
            .hit_test(point, BODY_HIT_RADIUS, HANDLE_RADIUS + 4.0)
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        let point = Point::new(position.x, position.y);
        self.cursor_position = Some(point);
        self.hover = self.hit_test(point);

        match self.interaction {
            Interaction::Idle => {}
            Interaction::DragBody { last_point } => {
                let delta = point - last_point;
                self.pose = LineGuide2D::new(
                    self.pose.center() + delta,
                    self.pose.angle_rad(),
                    self.pose.length(),
                );
                self.interaction = Interaction::DragBody { last_point: point };
            }
            Interaction::DragStartHandle { fixed_end } => {
                self.update_pose_from_endpoints(point, fixed_end);
                self.hover = Some(GuideHit::StartHandle);
            }
            Interaction::DragEndHandle { fixed_start } => {
                self.update_pose_from_endpoints(fixed_start, point);
                self.hover = Some(GuideHit::EndHandle);
            }
            Interaction::PanDomain { last_axis_view } => {
                let axis_view = self.pose.project_view_position(point);
                self.domain.pan_by_view(
                    (axis_view - last_axis_view) * RIGHT_DRAG_PAN_GAIN,
                    self.axis_view_span(),
                );
                self.interaction = Interaction::PanDomain {
                    last_axis_view: axis_view,
                };
                self.hover = Some(GuideHit::Body);
            }
        }
        self.sync_window_title();
        self.request_redraw();
    }

    fn update_pose_from_endpoints(&mut self, start: Point, end: Point) {
        let Some(pose) = LineGuide2D::from_endpoints(start, end) else {
            return;
        };
        if pose.length() < MIN_RULER_LENGTH {
            return;
        }
        self.pose = pose;
    }

    fn on_mouse_input(&mut self, button: MouseButton, state: ElementState) {
        let Some(cursor) = self.cursor_position else {
            return;
        };

        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => match self.hit_test(cursor) {
                Some(GuideHit::StartHandle) => {
                    self.interaction = Interaction::DragStartHandle {
                        fixed_end: self.pose.end(),
                    };
                }
                Some(GuideHit::EndHandle) => {
                    self.interaction = Interaction::DragEndHandle {
                        fixed_start: self.pose.start(),
                    };
                }
                Some(GuideHit::Body) => {
                    self.interaction = Interaction::DragBody { last_point: cursor };
                }
                None => {}
            },
            (MouseButton::Left, ElementState::Released) => {
                self.interaction = Interaction::Idle;
            }
            (MouseButton::Right, ElementState::Pressed) => {
                if matches!(self.hit_test(cursor), Some(GuideHit::Body)) {
                    self.interaction = Interaction::PanDomain {
                        last_axis_view: self.pose.project_view_position(cursor),
                    };
                }
            }
            (MouseButton::Right, ElementState::Released) => {
                if matches!(self.interaction, Interaction::PanDomain { .. }) {
                    self.interaction = Interaction::Idle;
                }
            }
            _ => {}
        }
        self.sync_window_title();
        self.request_redraw();
    }

    fn on_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(cursor) = self.cursor_position else {
            return;
        };
        if self
            .pose
            .hit_test(cursor, BODY_HIT_RADIUS, HANDLE_RADIUS + 4.0)
            .is_none()
        {
            return;
        }
        let scalar = self
            .pose
            .project_view_position(cursor)
            .clamp(0.0, self.pose.length());
        let dy = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y) * 36.0,
            MouseScrollDelta::PixelDelta(delta) => delta.y,
        };
        let factor = (1.0 + dy * 0.0015).clamp(0.5, 1.5);
        self.domain
            .zoom_about_view(scalar, factor, self.axis_view_span());
        self.sync_window_title();
        self.request_redraw();
    }

    fn on_keyboard(&mut self, key: &Key) {
        match key {
            Key::Named(NamedKey::Space) => self.domain.fit_bounds(),
            Key::Character(ch) if ch.eq_ignore_ascii_case("l") => self.domain.toggle_mode(),
            Key::Character(ch) if ch.eq_ignore_ascii_case("r") => {
                if let Some(window) = self.window() {
                    self.reset_pose(window.inner_size());
                }
                self.domain.reset_visible();
            }
            _ => return,
        }
        self.sync_window_title();
        self.request_redraw();
    }
}

impl ApplicationHandler for AxisRulerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if matches!(self.render_state, RenderState::Active { .. }) {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("axis ruler demo")
                        .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
                        .with_resizable(true),
                )
                .expect("create axis ruler demo window"),
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
        self.reset_pose(size);
        self.resize(size);
        self.domain.reset_visible();
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
            WindowEvent::MouseWheel { delta, .. } => self.on_mouse_wheel(delta),
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

fn build_scene(
    size: PhysicalSize<u32>,
    domain: &AxisDomainView,
    pose: LineGuide2D,
    hover: Option<GuideHit>,
    cursor_position: Option<Point>,
    text: &mut DemoText,
) -> Scene {
    let mut scene = Scene::new();
    let mut painter = Painter::new(&mut scene);
    let width = f64::from(size.width);
    let height = f64::from(size.height);
    let background = Rect::new(0.0, 0.0, width, height);
    fill_rect(&mut painter, background, rgba(BG));

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

    let shadow_offset = pose.normal() * 7.0;
    stroke_line(
        &mut painter,
        pose.start() + shadow_offset,
        pose.end() + shadow_offset,
        5.0,
        rgba(SHADOW),
    );

    let mapping = domain.mapping(0.0..pose.length());
    let ruler = domain.ruler(0.0..pose.length());
    let axis_guide = AxisGuide2D::from_ruler(&ruler, pose, AxisGuideOptions::default());
    let scale = domain.scale(0.0..pose.length());
    let label_step = domain.label_step(0.0..pose.length());
    let label_angle = axis_guide.label_angle_rad();

    for mark in axis_guide.marks() {
        let (mark_color, mark_width) = match mark.kind {
            AxisTickKind::Major => (rgba(RULER_BASE), 2.0),
            AxisTickKind::Medium => (rgba(RULER_SECONDARY), 1.5),
            AxisTickKind::Minor => (rgba(RULER_MINOR), 1.0),
        };
        stroke_line(
            &mut painter,
            mark.baseline_point,
            mark.tip_point,
            mark_width,
            mark_color,
        );

        if mark.labeled {
            let label = format_value(mark.value, label_step);
            let layout = text.layout_label(&label, 12.0, Brush::Solid(mark_color), None);
            let transform = Affine::translate((mark.label_anchor.x, mark.label_anchor.y))
                * Affine::rotate(label_angle)
                * Affine::translate((-f64::from(layout.width()) * 0.5, 0.0));
            text.draw_layout(&mut painter, &layout, transform);
        }
    }

    stroke_line(
        &mut painter,
        pose.start(),
        pose.end(),
        3.5,
        rgba(RULER_BASE),
    );

    if let Some(cursor) = cursor_position.filter(|point| {
        pose.hit_test(*point, BODY_HIT_RADIUS, HANDLE_RADIUS + 4.0)
            .is_some()
    }) {
        let scalar = pose.project_view_position(cursor).clamp(0.0, pose.length());
        let projected = pose.nearest_point_on_baseline(cursor);
        let normal = pose.normal();
        stroke_line(
            &mut painter,
            projected - normal * 48.0,
            projected + normal * 28.0,
            2.0,
            rgba(DOMAIN_GUIDE),
        );

        let value = mapping.view_to_domain(scalar);
        let readout = format_value(value, label_step);
        let layout = text.layout_label(&readout, 12.0, Brush::Solid(rgba(LABEL_TEXT)), None);
        let label_anchor = projected - normal * 64.0;
        let transform = Affine::translate((label_anchor.x, label_anchor.y))
            * Affine::rotate(label_angle)
            * Affine::translate((-f64::from(layout.width()) * 0.5, -16.0));
        text.draw_layout(&mut painter, &layout, transform);
    }

    draw_handle(
        &mut painter,
        pose.start(),
        matches!(hover, Some(GuideHit::StartHandle)),
    );
    draw_handle(
        &mut painter,
        pose.end(),
        matches!(hover, Some(GuideHit::EndHandle)),
    );
    if matches!(hover, Some(GuideHit::Body)) {
        stroke_line(
            &mut painter,
            pose.start(),
            pose.end(),
            6.0,
            rgba(BASELINE_HOVER).multiply_alpha(0.35),
        );
    }

    let panel_rect = Rect::new(18.0, 18.0, PANEL_WIDTH - 18.0, height - 18.0);
    fill_rounded_rect(
        &mut painter,
        panel_rect,
        16.0,
        rgba(PANEL_BG).multiply_alpha(1.04),
    );
    stroke_rounded_rect(&mut painter, panel_rect, 16.0, 1.0, rgba(PANEL_STROKE));

    let title = text.layout_label("Axis Ruler", 18.0, Brush::Solid(rgba(LABEL_TEXT)), None);
    text.draw_layout(&mut painter, &title, Affine::translate((34.0, 34.0)));

    let badge_color = match domain.mode() {
        AxisMode::Linear => rgba(LINEAR_BADGE),
        AxisMode::Log => rgba(LOG_BADGE),
    };
    fill_rounded_rect(
        &mut painter,
        Rect::new(34.0, 68.0, 114.0, 92.0),
        12.0,
        badge_color.multiply_alpha(0.18),
    );
    let badge = text.layout_label(domain.mode().label(), 12.0, Brush::Solid(badge_color), None);
    text.draw_layout(&mut painter, &badge, Affine::translate((48.0, 74.0)));

    let visible = domain.visible_domain();
    let info_lines = [
        String::from("Left-drag body: move ruler"),
        String::from("Left-drag endpoints: rotate / stretch"),
        String::from("Right-drag baseline: pan domain"),
        String::from("Wheel over baseline: zoom domain"),
        String::from("L: toggle linear/log"),
        String::from("Space: fit bounds    R: reset"),
        format!("Visible  {}", format_range(&visible, label_step)),
        format!(
            "Ticks  major {}  label {}",
            format_optional_number(scale.major_step()),
            format_optional_number(scale.label_step())
        ),
    ];
    let mut y = 108.0;
    for line in info_lines {
        let layout = text.layout_label(&line, 12.0, Brush::Solid(rgba(DIM_TEXT)), None);
        text.draw_layout(&mut painter, &layout, Affine::translate((34.0, y)));
        y += 22.0;
    }

    let hint = text.layout_label(
        "This demo uses understory_guide above understory_axis.\nDomain navigation stays local; both crates remain headless.",
        12.0,
        Brush::Solid(rgba(DIM_TEXT)),
        Some((PANEL_WIDTH - 68.0) as f32),
    );
    text.draw_layout(
        &mut painter,
        &hint,
        Affine::translate((34.0, panel_rect.y1 - 84.0)),
    );

    scene
}

fn draw_handle<S: PaintSink + ?Sized>(painter: &mut Painter<'_, S>, center: Point, hovered: bool) {
    let fill = if hovered {
        rgba(HANDLE_HOVER)
    } else {
        rgba(HANDLE_FILL)
    };
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

fn rgba(rgba: u32) -> Color {
    let [r, g, b, a] = rgba.to_be_bytes();
    Color::from_rgba8(r, g, b, a)
}

fn clamp_linear_range(range: Range<f64>, bounds: Range<f64>) -> Range<f64> {
    let bounds_span = bounds.end - bounds.start;
    let span = (range.end - range.start)
        .max(1e-9)
        .min(bounds_span.max(1e-9));
    let mut start = range.start;
    let mut end = start + span;
    if start < bounds.start {
        start = bounds.start;
        end = start + span;
    }
    if end > bounds.end {
        end = bounds.end;
        start = end - span;
    }
    start.max(bounds.start)..end.min(bounds.end)
}

fn clamp_log_range(range: Range<f64>, bounds: Range<f64>, base: f64) -> Range<f64> {
    let bounds_log_start = log_in_base(bounds.start, base);
    let bounds_log_end = log_in_base(bounds.end, base);
    let span = (log_in_base(range.end.max(range.start), base)
        - log_in_base(range.start.max(f64::MIN_POSITIVE), base))
    .max(0.05)
    .min(bounds_log_end - bounds_log_start);
    let mut start_log = log_in_base(range.start.max(bounds.start), base);
    let mut end_log = start_log + span;
    if start_log < bounds_log_start {
        start_log = bounds_log_start;
        end_log = start_log + span;
    }
    if end_log > bounds_log_end {
        end_log = bounds_log_end;
        start_log = end_log - span;
    }
    log_range(start_log, end_log - start_log, base)
}

fn log_range(start_log: f64, span_log: f64, base: f64) -> Range<f64> {
    let start = base.powf(start_log);
    let end = base.powf(start_log + span_log);
    start..end
}

fn log_range_span(range: &Range<f64>, base: f64) -> f64 {
    log_in_base(range.end, base) - log_in_base(range.start, base)
}

fn log_in_base(value: f64, base: f64) -> f64 {
    value.ln() / base.ln()
}

fn view_ratio_to_log_value(range: &Range<f64>, ratio: f64, base: f64) -> f64 {
    let start = log_in_base(range.start, base);
    let span = log_range_span(range, base);
    base.powf(start + ratio * span)
}

fn precision_for_step(step: f64) -> usize {
    if step >= 100.0 {
        0
    } else if step >= 10.0 {
        1
    } else if step >= 1.0 {
        2
    } else if step >= 0.1 {
        3
    } else {
        4
    }
}

fn format_value(value: f64, step: Option<f64>) -> String {
    let abs = value.abs();
    if abs >= 10_000.0 || (abs > 0.0 && abs < 0.001) {
        return format!("{value:.2e}");
    }
    let precision = step.map(|step| precision_for_step(step.abs())).unwrap_or(2);
    format!("{value:.precision$}", precision = precision)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn format_range(range: &Range<f64>, step: Option<f64>) -> String {
    format!(
        "{} -> {}",
        format_value(range.start, step),
        format_value(range.end, step)
    )
}

fn format_optional_number(value: Option<f64>) -> String {
    value
        .map(|value| format_value(value, None))
        .unwrap_or_else(|| String::from("n/a"))
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
        label: Some("axis ruler demo device"),
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
        label: Some("axis ruler demo render target"),
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
    let mut app = AxisRulerApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
