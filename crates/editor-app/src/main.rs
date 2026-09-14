use editor_core::{DocumentSnapshot, Geometry};
use editor_service::{AnalysisResult, ApplicationService, Capabilities};
use eframe::egui::{self, Color32, Rect, Stroke, Vec2};

const SAMPLE: &[u8] = include_bytes!("../../../fixtures/synthetic/s0_polarity.gbr");

struct S0App {
    capabilities: Capabilities,
    snapshot: DocumentSnapshot,
    analysis: AnalysisResult,
    zoom: f32,
    origin: Vec2,
    adapter_info: String,
    target_format: egui_wgpu::wgpu::TextureFormat,
}

impl S0App {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let mut service = ApplicationService::new();
        service
            .open_demo_s0("s0-demo", SAMPLE)
            .map_err(|error| error.to_string())?;
        let capabilities = service.capabilities();
        let snapshot = service
            .snapshot("s0-demo")
            .map_err(|error| error.to_string())?;
        let analysis = service
            .analyze_s0("s0-demo")
            .map_err(|error| error.to_string())?;
        let adapter_info = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| {
                let info = state.adapter.get_info();
                format!(
                    "{} / {:?} / {:?}",
                    info.name, info.backend, info.device_type
                )
            })
            .unwrap_or_else(|| "wgpu adapter unavailable".into());
        let target_format = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| state.target_format)
            .unwrap_or(egui_wgpu::wgpu::TextureFormat::Bgra8UnormSrgb);
        Ok(Self {
            capabilities,
            snapshot,
            analysis,
            zoom: 18.0,
            origin: Vec2::ZERO,
            adapter_info,
            target_format,
        })
    }
}

impl eframe::App for S0App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("s0_status").show(ctx, |ui| {
            ui.heading("Gerber editor - S0 validation");
            ui.label("S0 read-only technology demo");
            ui.label("No editing or export capability");
            ui.label(format!("wgpu adapter: {}", self.adapter_info));
        });
        egui::SidePanel::left("s0_capabilities")
            .resizable(false)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("S0 capabilities");
                ui.label(format!(
                    "API v{} · revision {}",
                    self.capabilities.api_version, self.snapshot.revision
                ));
                let object_total: usize = self
                    .snapshot
                    .layers
                    .iter()
                    .map(|layer| layer.objects.len())
                    .sum();
                ui.label(format!("objects (snapshot total): {}", object_total));
                ui.separator();
                ui.label("Supported: FS / MO / C aperture / LPD-LPC / D03");
                ui.label("Rejected: D01/D02, Region, AM/AB/SR, export");
                ui.separator();
                ui.label("Coverage samples (mm)");
                for sample in &self.analysis.samples {
                    ui.label(format!(
                        "({:.1}, {:.1}) -> {}",
                        sample.point.x_mm,
                        sample.point.y_mm,
                        if sample.covered { "Dark" } else { "Clear" }
                    ));
                }
                ui.add(egui::Slider::new(&mut self.zoom, 8.0..=50.0).text("zoom"));
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, egui::Sense::drag());
            self.origin += canvas_pan_delta(&response);
            let rect = response.rect;
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, Color32::from_gray(90)),
                egui::StrokeKind::Inside,
            );
            match gpu_objects(&self.snapshot) {
                Ok((objects, object_count)) => {
                    let callback = egui_wgpu::Callback::new_paint_callback(
                        rect,
                        ClipCallback {
                            zoom: self.zoom,
                            pan: self.origin,
                            rect,
                            format: self.target_format,
                            object_count,
                            objects,
                        },
                    );
                    painter.add(egui::Shape::Callback(callback));
                }
                Err(error) => {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        error,
                        egui::TextStyle::Body.resolve(ui.style()),
                        Color32::RED,
                    );
                }
            }
            painter.text(
                rect.left_top() + Vec2::new(12.0, 12.0),
                egui::Align2::LEFT_TOP,
                "Drag to pan · Slider controls zoom · Clip is canvas-local",
                egui::TextStyle::Small.resolve(ui.style()),
                Color32::LIGHT_GRAY,
            );
        });
    }
}

fn canvas_pan_delta(response: &egui::Response) -> Vec2 {
    if response.dragged() || response.drag_stopped() {
        // drag_delta() becomes zero on release, even if that frame also moved.
        response.ctx.input(|input| input.pointer.delta())
    } else {
        Vec2::ZERO
    }
}

struct ClipCallback {
    zoom: f32,
    pan: Vec2,
    rect: Rect,
    format: egui_wgpu::wgpu::TextureFormat,
    object_count: u32,
    objects: [[f32; 4]; 48],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CanvasUniforms {
    canvas_px: [f32; 2],
    viewport_min_px: [f32; 2],
    pixels_per_point: f32,
    zoom: f32,
    pan: [f32; 2],
    object_count: u32,
    _padding: u32,
    _padding2: [u32; 2],
    objects: [[f32; 4]; 48],
}

// S0 has no floating origin yet. Refuse an unrepresentable preview while
// preserving the f64 document. This is not an import/manufacturing size limit.
fn checked_gpu_mm(value: f64, feature: f64) -> Result<f32, String> {
    let converted = value as f32;
    let tolerance = editor_core::EPSILON_MM.min(feature / 16.0);
    if !value.is_finite()
        || !feature.is_finite()
        || feature < f64::from(f32::MIN_POSITIVE).sqrt() * 16.0
        || !converted.is_finite()
        || converted.abs() > 1e18
        || (value != 0.0 && !converted.is_normal())
        || (f64::from(converted) - value).abs() > tolerance
        || f64::from(converted.abs()) * f64::from(f32::EPSILON) > feature / 16.0
    {
        return Err("S0 GPU precision cannot preserve geometry; preview refused".into());
    }
    Ok(converted)
}

fn gpu_objects(snapshot: &DocumentSnapshot) -> Result<([[f32; 4]; 48], u32), String> {
    let mut objects = [[0.0; 4]; 48];
    let mut count = 0_usize;
    for (layer_index, layer) in snapshot.layers.iter().enumerate() {
        if layer_index >= 4 {
            return Err("S0 GPU preview limit is 4 layers; preview refused".into());
        }
        for object in &layer.objects {
            if count + 2 >= objects.len() {
                return Err("S0 GPU preview limit is 16 objects; preview refused".into());
            }
            match object.geometry {
                Geometry::CircleFlash { center, aperture } => {
                    editor_core::CircleAperture::new(
                        aperture.diameter_mm,
                        aperture.hole_diameter_mm,
                    )
                    .map_err(|error| error.to_string())?;
                    let radius = aperture.diameter_mm / 2.0;
                    let hole = aperture.hole_diameter_mm.unwrap_or(0.0) / 2.0;
                    let feature = if hole > 0.0 {
                        hole.min(radius - hole)
                    } else {
                        radius
                    };
                    let x = checked_gpu_mm(center.x_mm, feature)?;
                    let y = checked_gpu_mm(center.y_mm, feature)?;
                    let radius = checked_gpu_mm(radius, feature)?;
                    let hole = checked_gpu_mm(hole, feature)?;
                    if aperture.hole_diameter_mm.is_some() && (hole <= 0.0 || hole >= radius) {
                        return Err(
                            "S0 GPU preview cannot preserve aperture hole; preview refused".into(),
                        );
                    }
                    objects[count] = [x, y, 0.0, layer_index as f32];
                    objects[count + 1] = [radius, hole, 0.0, 0.0];
                }
                Geometry::Line {
                    start,
                    end,
                    width_mm,
                } => {
                    if !width_mm.is_finite()
                        || width_mm <= 0.0
                        || !start.is_finite()
                        || !end.is_finite()
                    {
                        return Err("S0 GPU preview requires finite valid line geometry".into());
                    }
                    let length = start.distance_mm(end);
                    let feature = if length > editor_core::EPSILON_MM {
                        (width_mm / 2.0).min(length)
                    } else {
                        width_mm / 2.0
                    };
                    let start_x = checked_gpu_mm(start.x_mm, feature)?;
                    let start_y = checked_gpu_mm(start.y_mm, feature)?;
                    let end_x = checked_gpu_mm(end.x_mm, feature)?;
                    let end_y = checked_gpu_mm(end.y_mm, feature)?;
                    let radius = checked_gpu_mm(width_mm / 2.0, feature)?;
                    let dx = end_x - start_x;
                    let dy = end_y - start_y;
                    if ((dx * dx + dy * dy) <= 1e-12) != (length <= editor_core::EPSILON_MM) {
                        return Err(
                            "S0 GPU rounding changes line degeneracy; preview refused".into()
                        );
                    }
                    objects[count] = [start_x, start_y, 1.0, layer_index as f32];
                    objects[count + 1] = [end_x, end_y, radius, 0.0];
                }
            }
            objects[count + 2][0] = match object.exposure {
                editor_core::Exposure::Dark => 1.0,
                editor_core::Exposure::Clear => 0.0,
            };
            count += 3;
        }
    }
    Ok((objects, (count / 3) as u32))
}

struct CanvasGpu {
    pipeline: egui_wgpu::wgpu::RenderPipeline,
    uniform_buffer: egui_wgpu::wgpu::Buffer,
    bind_group: egui_wgpu::wgpu::BindGroup,
}

impl egui_wgpu::CallbackTrait for ClipCallback {
    fn prepare(
        &self,
        device: &egui_wgpu::wgpu::Device,
        queue: &egui_wgpu::wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut egui_wgpu::wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<egui_wgpu::wgpu::CommandBuffer> {
        use bytemuck::Zeroable;
        use egui_wgpu::wgpu::util::DeviceExt;
        if callback_resources.get::<CanvasGpu>().is_none() {
            let shader = device.create_shader_module(egui_wgpu::wgpu::ShaderModuleDescriptor {
                label: Some("s0-canvas-shader"),
                source: egui_wgpu::wgpu::ShaderSource::Wgsl(include_str!("canvas.wgsl").into()),
            });
            let uniform_buffer =
                device.create_buffer_init(&egui_wgpu::wgpu::util::BufferInitDescriptor {
                    label: Some("s0-canvas-uniform"),
                    contents: bytemuck::bytes_of(&CanvasUniforms::zeroed()),
                    usage: egui_wgpu::wgpu::BufferUsages::UNIFORM
                        | egui_wgpu::wgpu::BufferUsages::COPY_DST,
                });
            let bind_group_layout =
                device.create_bind_group_layout(&egui_wgpu::wgpu::BindGroupLayoutDescriptor {
                    label: Some("s0-canvas-bind-layout"),
                    entries: &[egui_wgpu::wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: egui_wgpu::wgpu::ShaderStages::FRAGMENT
                            | egui_wgpu::wgpu::ShaderStages::VERTEX,
                        ty: egui_wgpu::wgpu::BindingType::Buffer {
                            ty: egui_wgpu::wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
            let bind_group = device.create_bind_group(&egui_wgpu::wgpu::BindGroupDescriptor {
                label: Some("s0-canvas-bind-group"),
                layout: &bind_group_layout,
                entries: &[egui_wgpu::wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });
            let pipeline_layout =
                device.create_pipeline_layout(&egui_wgpu::wgpu::PipelineLayoutDescriptor {
                    label: Some("s0-canvas-pipeline-layout"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });
            let pipeline =
                device.create_render_pipeline(&egui_wgpu::wgpu::RenderPipelineDescriptor {
                    label: Some("s0-canvas-pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: egui_wgpu::wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(egui_wgpu::wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(egui_wgpu::wgpu::ColorTargetState {
                            format: self.format,
                            blend: None,
                            write_mask: egui_wgpu::wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview: None,
                    cache: None,
                });
            callback_resources.insert(CanvasGpu {
                pipeline,
                uniform_buffer,
                bind_group,
            });
        }
        let Some(gpu) = callback_resources.get::<CanvasGpu>() else {
            return Vec::new();
        };
        let uniform = CanvasUniforms {
            canvas_px: [
                self.rect.width() * _screen_descriptor.pixels_per_point,
                self.rect.height() * _screen_descriptor.pixels_per_point,
            ],
            viewport_min_px: [
                self.rect.min.x * _screen_descriptor.pixels_per_point,
                self.rect.min.y * _screen_descriptor.pixels_per_point,
            ],
            pixels_per_point: _screen_descriptor.pixels_per_point,
            zoom: self.zoom,
            pan: [self.pan.x, self.pan.y],
            object_count: self.object_count,
            _padding: 0,
            _padding2: [0; 2],
            objects: self.objects,
        };
        queue.write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut egui_wgpu::wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let viewport = info.viewport_in_pixels();
        let clip = info.clip_rect_in_pixels();
        let x = viewport.left_px.max(clip.left_px).max(0) as u32;
        let y = viewport.top_px.max(clip.top_px).max(0) as u32;
        let right = (viewport.left_px + viewport.width_px)
            .min(clip.left_px + clip.width_px)
            .max(0) as u32;
        let bottom = (viewport.top_px + viewport.height_px)
            .min(clip.top_px + clip.height_px)
            .max(0) as u32;
        let width = right.saturating_sub(x);
        let height = bottom.saturating_sub(y);
        if width == 0 || height == 0 {
            return;
        }
        render_pass.set_scissor_rect(x, y, width, height);
        if let Some(gpu) = callback_resources.get::<CanvasGpu>() {
            render_pass.set_pipeline(&gpu.pipeline);
            render_pass.set_bind_group(0, &gpu.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Gerber 编辑器 · S0 技术验证",
        native_options,
        Box::new(|cc| {
            S0App::new(cc)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })
        }),
    )
}

#[cfg(test)]
mod gpu_tests;

#[cfg(test)]
mod navigation_tests;
