// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal `winit` demo for Overstory rendered through `imaging_vello_hybrid`.

use std::{process::ExitCode, sync::Arc};

use imaging_vello_hybrid::VelloHybridRenderer;
use imaging_vello_hybrid::wgpu::{
    self, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendState,
    Color, ColorTargetState, ColorWrites, CommandEncoderDescriptor, Device, Extent3d, FilterMode,
    FragmentState, LoadOp, MultisampleState, Operations, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Surface,
    SurfaceConfiguration, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
    VertexState,
};
use kurbo::Size;
use overstory::{
    BACKGROUND_PROPERTY, BORDER_PART, BORDER_PROPERTY, BORDER_WIDTH_PROPERTY, BUTTON_PART,
    CONTENT_PRESENTER_PART, CONTENT_PROPERTY, CORNER_RADIUS_PROPERTY, ControlTemplate,
    FOREGROUND_PROPERTY, HOVERED, PADDING_PROPERTY, PRESSED, TemplateBinding, TemplateNode, Ui,
    built_in, compose,
};
use peniko::{Brush, Color as PaintColor};
use ui_events_winit::{WindowEventReducer, WindowEventTranslation};
use understory_style::{ClassId, StyleBuilder, StyleCascadeBuilder, StyleOrigin};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

const WINDOW_WIDTH: f64 = 680.0;
const WINDOW_HEIGHT: f64 = 400.0;
const PRIMARY_BUTTON: ClassId = ClassId(1);
const SURFACE_BUTTON: ClassId = ClassId(2);
const ALERT_BUTTON: ClassId = ClassId(3);

const BLIT_SHADER: &str = r#"
@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
    );

    var out: VertexOut;
    out.position = vec4<f32>(positions[index], 0.0, 1.0);
    out.uv = uvs[index];
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(scene_texture, scene_sampler, in.uv);
}
"#;

#[derive(Debug)]
struct App {
    state: Option<RunState>,
}

#[derive(Debug)]
struct RunState {
    window: Arc<Window>,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    device: Device,
    queue: Queue,
    renderer: VelloHybridRenderer,
    blit: BlitState,
    event_reducer: WindowEventReducer,
    ui: Ui,
}

#[derive(Debug)]
struct BlitState {
    pipeline: RenderPipeline,
    layout: BindGroupLayout,
    sampler: Sampler,
    offscreen: Option<OffscreenTarget>,
}

#[derive(Debug)]
struct OffscreenTarget {
    #[expect(
        dead_code,
        reason = "the texture must be retained for the view and bind group to stay valid"
    )]
    texture: Texture,
    view: TextureView,
    bind_group: BindGroup,
    width: u32,
    height: u32,
}

impl App {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Overstory demo")
                        .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
                )
                .expect("failed to create window"),
        );

        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no suitable GPU adapter found");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("overstory_demo"),
            ..Default::default()
        }))
        .expect("failed to create device");

        let mut surface_config = surface
            .get_default_config(&adapter, 1, 1)
            .expect("surface not compatible with adapter");
        let initial_size = window.inner_size();
        surface_config.width = initial_size.width.max(1);
        surface_config.height = initial_size.height.max(1);
        surface.configure(&device, &surface_config);

        let blit = BlitState::new(&device, surface_config.format);
        let ui = build_ui();
        let state = RunState {
            window,
            surface,
            surface_config,
            renderer: VelloHybridRenderer::new(device.clone(), queue.clone()),
            device,
            queue,
            blit,
            event_reducer: WindowEventReducer::default(),
            ui,
        };
        state.window.request_redraw();
        self.state = Some(state);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        if let Some(translation) = state
            .event_reducer
            .reduce(state.window.scale_factor(), &event)
        {
            state.handle_translation(translation);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                state.resize(size);
                state.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                state.render();
            }
            _ => {}
        }
    }
}

impl RunState {
    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.surface_config.width = size.width.max(1);
        self.surface_config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
        self.blit.offscreen = None;
    }

    fn handle_translation(&mut self, translation: WindowEventTranslation) {
        let changed = match translation {
            WindowEventTranslation::Pointer(event) => {
                self.ui.pointer_event(self.viewport_size(), &event)
            }
            WindowEventTranslation::Keyboard(event) => self.ui.keyboard_event(&event),
        };
        if changed {
            self.window.request_redraw();
        }
    }

    fn viewport_size(&self) -> Size {
        let scale = self.window.scale_factor().max(1.0);
        Size::new(
            f64::from(self.surface_config.width) / scale,
            f64::from(self.surface_config.height) / scale,
        )
    }

    fn render(&mut self) {
        let width = self.surface_config.width.max(1);
        let height = self.surface_config.height.max(1);
        let scene = self
            .ui
            .paint_scene_with_scale(self.viewport_size(), self.window.scale_factor());
        let native = self
            .renderer
            .encode_scene(scene, checked_u16(width), checked_u16(height))
            .expect("failed to encode overstory scene");
        let target = self.blit.target(&self.device, width, height);
        self.renderer
            .render_to_texture_view(&native, &target.view, width, height)
            .expect("failed to render overstory scene");

        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.surface_config);
                return;
            }
            Err(error) => {
                eprintln!("surface error: {error}");
                return;
            }
        };
        let output_view = output
            .texture
            .create_view(&TextureViewDescriptor::default());
        self.blit_to_surface(&output_view);
        output.present();
        self.ui.clear_input_frame();
    }

    fn blit_to_surface(&self, output: &TextureView) {
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("overstory_demo blit"),
            });
        let Some(target) = self.blit.offscreen.as_ref() else {
            return;
        };
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("overstory_demo blit pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blit.pipeline);
            pass.set_bind_group(0, &target.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }
}

impl BlitState {
    fn new(device: &Device, output_format: TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("overstory_demo scene texture layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("overstory_demo scene sampler"),
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            ..Default::default()
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("overstory_demo blit shader"),
            source: ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("overstory_demo blit pipeline layout"),
            bind_group_layouts: &[&layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("overstory_demo blit pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: output_format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            layout,
            sampler,
            offscreen: None,
        }
    }

    fn target(&mut self, device: &Device, width: u32, height: u32) -> &OffscreenTarget {
        let needs_new = self
            .offscreen
            .as_ref()
            .is_none_or(|target| target.width != width || target.height != height);
        if needs_new {
            self.offscreen = Some(OffscreenTarget::new(
                device,
                &self.layout,
                &self.sampler,
                width,
                height,
            ));
        }
        self.offscreen
            .as_ref()
            .expect("offscreen target should exist")
    }
}

impl OffscreenTarget {
    fn new(
        device: &Device,
        layout: &BindGroupLayout,
        sampler: &Sampler,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("overstory_demo offscreen scene"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("overstory_demo scene texture binding"),
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            bind_group,
            width,
            height,
        }
    }
}

fn build_ui() -> Ui {
    let mut ui = Ui::new();
    let props = ui.properties();
    ui.set_local(
        ui.root(),
        props.background,
        Some(Brush::from(PaintColor::from_rgb8(0x10, 0x11, 0x14))),
    );
    ui.set_local(
        ui.root(),
        props.padding,
        kurbo::Insets::new(58.0, 42.0, 0.0, 0.0),
    );
    ui.set_local(ui.root(), props.spacing, 14.0);
    ui.set_local(
        ui.root(),
        props.foreground,
        Some(Brush::from(PaintColor::from_rgb8(0xe9, 0xec, 0xf1))),
    );

    let panel = ui.append_spec(
        ui.root(),
        compose::panel()
            .set(props.padding, kurbo::Insets::new(22.0, 20.0, 22.0, 22.0))
            .set(props.spacing, 12.0)
            .set(props.corner_radius, 18.0)
            .style(built_in::panel_style(props)),
    );

    ui.append_spec(
        panel,
        compose::text_block("Open widgets")
            .set(props.padding, kurbo::Insets::uniform(0.0))
            .set(props.font_size, 24.0)
            .set(
                props.foreground,
                Some(Brush::from(PaintColor::from_rgb8(0xff, 0xff, 0xff))),
            ),
    );

    ui.append_spec(
        panel,
        compose::text_block(
            "Button and TextBlock now own their own measurement and presentation. The runtime keeps identity, properties, style, invalidation, and layout scheduling.",
        )
        .set(props.padding, kurbo::Insets::uniform(0.0))
        .set(props.font_size, 14.0)
        .set(props.min_width, 360.0)
        .set(
            props.foreground,
            Some(Brush::from(PaintColor::from_rgb8(0xb5, 0xbe, 0xca))),
        ),
    );

    let row = ui.append_spec(
        panel,
        compose::row()
            .set(props.spacing, 10.0)
            .set(props.padding, kurbo::Insets::uniform(0.0))
            .style(built_in::row_style(props)),
    );

    for (label, checked) in [("Sync", true), ("Draft", false)] {
        ui.append_spec(
            row,
            compose::toggle(label)
                .checked(checked)
                .set(props.padding, kurbo::Insets::new(0.0, 3.0, 0.0, 3.0))
                .set(props.min_width, 112.0)
                .style(built_in::toggle_style(props)),
        );
    }

    let button_style = demo_button_style(&ui);
    for (label, class, template, radius, padding, min_width) in [
        (
            "Launch workspace",
            PRIMARY_BUTTON,
            overstory::button_template(),
            14.0,
            kurbo::Insets::new(30.0, 15.0, 30.0, 17.0),
            244.0,
        ),
        (
            "Inspect layout",
            SURFACE_BUTTON,
            ring_button_template(),
            10.0,
            kurbo::Insets::new(28.0, 14.0, 28.0, 16.0),
            220.0,
        ),
        (
            "Resolve alerts",
            ALERT_BUTTON,
            framed_button_template(),
            5.0,
            kurbo::Insets::new(24.0, 12.0, 24.0, 14.0),
            196.0,
        ),
    ] {
        ui.append_spec(
            panel,
            compose::button(label)
                .class(class)
                .set(props.padding, padding)
                .set(props.min_width, min_width)
                .set(props.corner_radius, radius)
                .template(template)
                .style(button_style.clone()),
        );
    }

    ui
}

fn demo_button_style(ui: &Ui) -> understory_style::StyleCascade {
    let props = ui.properties();
    let base = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x2f, 0x36, 0x42))),
        )
        .set(
            props.foreground,
            Some(Brush::from(PaintColor::from_rgb8(0xf5, 0xf7, 0xfa))),
        )
        .set(
            props.border,
            Some(Brush::from(PaintColor::from_rgb8(0x52, 0x61, 0x73))),
        )
        .set(props.border_width, 1.0)
        .build();
    let hover = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x39, 0x43, 0x51))),
        )
        .build();
    let pressed = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x22, 0x28, 0x31))),
        )
        .build();

    let primary = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x2f, 0x6f, 0xed))),
        )
        .set(
            props.foreground,
            Some(Brush::from(PaintColor::from_rgb8(0xff, 0xff, 0xff))),
        )
        .set(
            props.border,
            Some(Brush::from(PaintColor::from_rgb8(0x86, 0xaa, 0xff))),
        )
        .build();
    let primary_hover = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x3f, 0x7f, 0xf3))),
        )
        .build();
    let primary_pressed = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x24, 0x55, 0xb8))),
        )
        .build();

    let surface = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0xf7, 0xf1, 0xe6))),
        )
        .set(
            props.foreground,
            Some(Brush::from(PaintColor::from_rgb8(0x20, 0x24, 0x28))),
        )
        .set(
            props.border,
            Some(Brush::from(PaintColor::from_rgb8(0xd6, 0xb6, 0x65))),
        )
        .build();
    let surface_hover = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0xff, 0xf8, 0xdf))),
        )
        .build();
    let surface_pressed = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0xe8, 0xd4, 0x96))),
        )
        .build();

    let alert = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x3d, 0x1f, 0x2d))),
        )
        .set(
            props.foreground,
            Some(Brush::from(PaintColor::from_rgb8(0xff, 0xef, 0xf5))),
        )
        .set(
            props.border,
            Some(Brush::from(PaintColor::from_rgb8(0xe4, 0x57, 0x7d))),
        )
        .set(props.border_width, 2.0)
        .build();
    let alert_hover = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x4e, 0x27, 0x39))),
        )
        .build();
    let alert_pressed = StyleBuilder::new()
        .set(
            props.background,
            Some(Brush::from(PaintColor::from_rgb8(0x2c, 0x17, 0x22))),
        )
        .build();

    StyleCascadeBuilder::new()
        .push_style(StyleOrigin::Base, base)
        .push_rules(
            StyleOrigin::Sheet,
            [
                (overstory::style::button_hovered(), hover),
                (overstory::style::button_pressed(), pressed),
                (
                    overstory::style::button().with_class(PRIMARY_BUTTON),
                    primary,
                ),
                (
                    overstory::style::button_when(HOVERED).with_class(PRIMARY_BUTTON),
                    primary_hover,
                ),
                (
                    overstory::style::button_when(PRESSED).with_class(PRIMARY_BUTTON),
                    primary_pressed,
                ),
                (
                    overstory::style::button().with_class(SURFACE_BUTTON),
                    surface,
                ),
                (
                    overstory::style::button_when(HOVERED).with_class(SURFACE_BUTTON),
                    surface_hover,
                ),
                (
                    overstory::style::button_when(PRESSED).with_class(SURFACE_BUTTON),
                    surface_pressed,
                ),
                (overstory::style::button().with_class(ALERT_BUTTON), alert),
                (
                    overstory::style::button_when(HOVERED).with_class(ALERT_BUTTON),
                    alert_hover,
                ),
                (
                    overstory::style::button_when(PRESSED).with_class(ALERT_BUTTON),
                    alert_pressed,
                ),
            ],
        )
        .build()
}

fn ring_button_template() -> ControlTemplate {
    ControlTemplate::new(TemplateNode::new(
        BUTTON_PART,
        [],
        [TemplateNode::new(
            BORDER_PART,
            [
                TemplateBinding::pass(BACKGROUND_PROPERTY),
                TemplateBinding::pass(BORDER_PROPERTY),
                TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
                TemplateBinding::pass(PADDING_PROPERTY),
                TemplateBinding::pass(CORNER_RADIUS_PROPERTY),
            ],
            [TemplateNode::new(
                BORDER_PART,
                [
                    TemplateBinding::pass(BORDER_PROPERTY),
                    TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
                    TemplateBinding::pass(CORNER_RADIUS_PROPERTY),
                ],
                [TemplateNode::new(
                    CONTENT_PRESENTER_PART,
                    [
                        TemplateBinding::pass(CONTENT_PROPERTY),
                        TemplateBinding::pass(FOREGROUND_PROPERTY),
                    ],
                    [],
                )],
            )
            .with_inset(3.0)],
        )],
    ))
}

fn framed_button_template() -> ControlTemplate {
    ControlTemplate::new(TemplateNode::new(
        BUTTON_PART,
        [],
        [TemplateNode::new(
            BORDER_PART,
            [
                TemplateBinding::pass(BACKGROUND_PROPERTY),
                TemplateBinding::pass(BORDER_PROPERTY),
                TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
                TemplateBinding::pass(PADDING_PROPERTY),
            ],
            [TemplateNode::new(
                BORDER_PART,
                [
                    TemplateBinding::pass(BORDER_PROPERTY),
                    TemplateBinding::pass(BORDER_WIDTH_PROPERTY),
                ],
                [TemplateNode::new(
                    CONTENT_PRESENTER_PART,
                    [
                        TemplateBinding::pass(CONTENT_PROPERTY),
                        TemplateBinding::pass(FOREGROUND_PROPERTY),
                    ],
                    [],
                )],
            )
            .with_inset(4.0)],
        )],
    ))
}

fn checked_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn main() -> ExitCode {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("overstory_demo: failed to create event loop: {error}");
            return event_loop_error_exit_code(&error);
        }
    };
    let mut app = App::new();
    match event_loop.run_app(&mut app) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("overstory_demo: event loop failed: {error}");
            // If the display connection is already broken, platform teardown can
            // run through invalid native state. The process is exiting anyway.
            std::mem::forget(app);
            event_loop_error_exit_code(&error)
        }
    }
}

fn event_loop_error_exit_code(error: &EventLoopError) -> ExitCode {
    match error {
        EventLoopError::ExitFailure(0) => ExitCode::SUCCESS,
        EventLoopError::ExitFailure(code) => {
            ExitCode::from(u8::try_from(*code).unwrap_or(1).max(1))
        }
        EventLoopError::NotSupported(_)
        | EventLoopError::Os(_)
        | EventLoopError::RecreationAttempt => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_loop_exit_failure_maps_to_process_failure() {
        assert_eq!(
            event_loop_error_exit_code(&EventLoopError::ExitFailure(1)),
            ExitCode::FAILURE
        );
    }
}
