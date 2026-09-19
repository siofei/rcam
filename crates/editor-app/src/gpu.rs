use crate::{
    camera::Camera,
    display::{Object, Primitive, Scene},
};
use eframe::egui;
use egui_wgpu::wgpu::{self, util::DeviceExt};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub view: [f32; 4],
    pub camera: [f32; 4],
    pub counts: [u32; 4],
    pub preview: [f32; 4],
    pub grid: [f32; 4],
}
pub fn uniforms(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[&str],
) -> Result<Uniforms, String> {
    uniforms_preview(
        scene,
        camera,
        rect,
        ppp,
        selected,
        editor_core::MmPoint::new(0., 0.),
    )
}
pub fn uniforms_preview(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[&str],
    delta: editor_core::MmPoint,
) -> Result<Uniforms, String> {
    prepare(scene, camera, rect, ppp, selected, delta).map(|v| v.0)
}
pub fn prepare(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[&str],
    delta: editor_core::MmPoint,
) -> Result<(Uniforms, Arc<crate::render_index::RenderIndex>), String> {
    let preview = [scene.scalar(delta.x_mm)?, scene.scalar(delta.y_mm)?, 0., 0.];
    let selected_flags = selection_flags(scene, selected);
    let width = rect.width() * ppp;
    let height = rect.height() * ppp;
    let pixels = f64::from(width) * f64::from(height);
    let ppm = camera.scale * f64::from(ppp);
    let cx = camera.center.x_mm - scene.anchor.x_mm;
    let cy = camera.center.y_mm - scene.anchor.y_mm;
    let index = if delta.x_mm == 0. && delta.y_mm == 0. {
        scene.index.clone()
    } else {
        Arc::new(crate::render_index::RenderIndex::build(
            &scene.objects,
            &selected_flags,
            [preview[0], preview[1]],
        )?)
    };
    let mut work = pixels * index.max_candidates as f64 * 20.;
    for (index, object) in scene.objects.iter().enumerate() {
        if object.meta[3] == 0 {
            continue;
        }
        let mut b = object.bounds;
        if selected_flags[index] != 0 {
            for i in 0..4 {
                b[i] += preview[i % 2];
            }
            for v in b {
                scene.scalar(f64::from(v))?;
            }
        }
        let left =
            ((f64::from(b[0]) - cx) * ppm + f64::from(width) / 2.).clamp(0., f64::from(width));
        let right =
            ((f64::from(b[2]) - cx) * ppm + f64::from(width) / 2.).clamp(0., f64::from(width));
        let bottom =
            ((f64::from(b[1]) - cy) * ppm + f64::from(height) / 2.).clamp(0., f64::from(height));
        let top =
            ((f64::from(b[3]) - cy) * ppm + f64::from(height) / 2.).clamp(0., f64::from(height));
        let cost: usize = scene.primitives[object.meta[0] as usize..object.meta[1] as usize]
            .iter()
            .map(|p| {
                if p.meta[0] == 1 {
                    p.meta[3] as usize
                } else {
                    1
                }
            })
            .sum();
        work += ((right - left).max(0.) + 4.) * ((top - bottom).max(0.) + 4.) * cost as f64 * 20.;
    }
    // Four coverage samples plus four selected-edge samples, conservatively bounded.
    if !work.is_finite() || work > 2_000_000_000. {
        return Err(format!(
            "RESOURCE_LIMIT: resource=candidate_sample_work limit=2000000000 actual={work}"
        ));
    }
    Ok((
        Uniforms {
            grid: index.grid,
            preview,
            view: [rect.left() * ppp, rect.top() * ppp, width, height],
            camera: [
                scene.scalar(camera.center.x_mm - scene.anchor.x_mm)?,
                scene.scalar(camera.center.y_mm - scene.anchor.y_mm)?,
                (camera.scale * f64::from(ppp)) as f32,
                0.,
            ],
            counts: [scene.objects.len() as u32, index.cols, index.rows, 0],
        },
        index,
    ))
}
pub fn selection_flags(scene: &Scene, selected: &[&str]) -> Vec<u32> {
    let ids: std::collections::HashSet<_> = selected.iter().copied().collect();
    scene
        .ids
        .iter()
        .map(|id| u32::from(ids.contains(id.as_str())))
        .collect()
}
pub struct Resources {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub selected: wgpu::Buffer,
    pub bind: wgpu::BindGroup,
    pub serial: u64,
    pub index: Arc<crate::render_index::RenderIndex>,
    pub bins: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    objects: wgpu::Buffer,
    shapes: wgpu::Buffer,
    points: wgpu::Buffer,
}
impl Resources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, scene: &Scene) -> Self {
        let storage = |name, bytes: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(name),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let dummy_o = [Object::default()];
        let dummy_p = [Primitive::default()];
        let dummy_v = [[0f32; 2]];
        let objects = storage(
            "semantic-objects",
            bytemuck::cast_slice(if scene.objects.is_empty() {
                &dummy_o
            } else {
                &scene.objects
            }),
        );
        let shapes = storage(
            "local-material",
            bytemuck::cast_slice(if scene.primitives.is_empty() {
                &dummy_p
            } else {
                &scene.primitives
            }),
        );
        let points = storage(
            "display-contours",
            bytemuck::cast_slice(if scene.points.is_empty() {
                &dummy_v
            } else {
                &scene.points
            }),
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let selected = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("selected-object-flags"),
            size: (scene.objects.len().max(1) * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bins = storage("world-bins", bytemuck::cast_slice(&scene.index.data));
        let entries: Vec<_> = (0..6)
            .map(|i| wgpu::BindGroupLayoutEntry {
                binding: i,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if i == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("editor-display"),
            entries: &entries,
        });
        let entries: Vec<_> = [&uniform, &objects, &shapes, &points, &selected, &bins]
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor-display"),
            layout: &layout,
            entries: &entries,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("editor-display"),
            source: wgpu::ShaderSource::Wgsl(include_str!("editor.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("editor-display"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        Self {
            pipeline,
            uniform,
            selected,
            bind,
            serial: scene.serial,
            index: scene.index.clone(),
            bins,
            layout,
            objects,
            shapes,
            points,
        }
    }
}
pub struct Callback {
    pub scene: Arc<Scene>,
    pub index: Arc<crate::render_index::RenderIndex>,
    pub uniforms: Uniforms,
    pub selected: Vec<u32>,
    pub format: wgpu::TextureFormat,
}
impl egui_wgpu::CallbackTrait for Callback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &egui_wgpu::ScreenDescriptor,
        _: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if resources
            .get::<Resources>()
            .is_none_or(|r| r.serial != self.scene.serial)
        {
            resources.insert(Resources::new(device, self.format, &self.scene));
        }
        if let Some(r) = resources.get_mut::<Resources>() {
            if !Arc::ptr_eq(&r.index, &self.index) {
                r.bins = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview-world-bins"),
                    contents: bytemuck::cast_slice(&self.index.data),
                    usage: wgpu::BufferUsages::STORAGE,
                });
                let entries: Vec<_> = [
                    &r.uniform,
                    &r.objects,
                    &r.shapes,
                    &r.points,
                    &r.selected,
                    &r.bins,
                ]
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect();
                r.bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("preview-display"),
                    layout: &r.layout,
                    entries: &entries,
                });
                r.index = self.index.clone();
            }
            queue.write_buffer(&r.uniform, 0, bytemuck::bytes_of(&self.uniforms));
            if !self.selected.is_empty() {
                queue.write_buffer(&r.selected, 0, bytemuck::cast_slice(&self.selected));
            }
        }
        vec![]
    }
    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let v = info.viewport_in_pixels();
        let c = info.clip_rect_in_pixels();
        let x = v.left_px.max(c.left_px).max(0) as u32;
        let y = v.top_px.max(c.top_px).max(0) as u32;
        let w = (v.left_px + v.width_px).min(c.left_px + c.width_px).max(0) as u32;
        let h = (v.top_px + v.height_px).min(c.top_px + c.height_px).max(0) as u32;
        if w <= x || h <= y {
            return;
        }
        pass.set_scissor_rect(x, y, w - x, h - y);
        if let Some(r) = resources.get::<Resources>() {
            pass.set_pipeline(&r.pipeline);
            pass.set_bind_group(0, &r.bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
