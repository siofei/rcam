use crate::{display::Scene, gpu::Uniforms, state::Model};
use editor_core::*;
use editor_service::{LayerInfo, RenderSnapshot};
use egui_wgpu::wgpu::{self, util::DeviceExt};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
fn block_on<T>(f: impl Future<Output = T>) -> T {
    struct WakeThread(std::thread::Thread);
    impl Wake for WakeThread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let w = Waker::from(Arc::new(WakeThread(std::thread::current())));
    let mut cx = Context::from_waker(&w);
    let mut f = std::pin::pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park_timeout(Duration::from_secs(1)),
        }
    }
}
fn fixture(name: &str) -> (RenderSnapshot, Vec<LayerInfo>) {
    let mut m = Model::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic")
        .join(name);
    m.open(&path).unwrap();
    let s = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    (s, m.view.layers)
}
#[test]
fn renderer_refuses_partial_unsupported_document() {
    let (mut s, l) = fixture("s1a/standard_hole_over_line.gbr");
    s.layers[0].objects.push(SemanticObject {
        object_id: "broken".into(),
        geometry: SemanticGeometry::Flash {
            center: MmPoint::new(0., 0.),
            aperture_id: "absent".into(),
            transform: Default::default(),
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Imported { command_index: 100 },
    });
    assert!(
        Scene::build(&s, &l, MmPoint::new(0., 0.), 100., 1)
            .err()
            .expect("expected refusal")
            .contains("NOT_FOUND")
    );
}
#[test]
fn renderer_builds_all_existing_semantic_variants() {
    for name in [
        "s2a3/gui_primitives.gbr",
        "s1a/standard_hole_over_line.gbr",
        "s1a/macro_hole_over_line.gbr",
        "s1a/macro_rotated_circle.gbr",
        "s1a/macro_rotated_rectangle.gbr",
        "s1a/region_cutin.gbr",
        "s1a/nested_region_union.gbr",
        "s0c/rectangular_draw.gbr",
        "s1a1/g75_exact.gbr",
        "s1a1/g75_small_deviation.gbr",
        "s1a1/g75_region_deviation_safe.gbr",
    ] {
        let (s, l) = fixture(name);
        let scene = Scene::build(&s, &l, MmPoint::new(0., 0.), 100., 1).unwrap();
        assert_eq!(
            scene.ids.len(),
            s.layers.iter().map(|l| l.objects.len()).sum::<usize>()
        );
        assert!(!scene.primitives.is_empty());
    }
}
#[test]
fn region_refines_with_zoom_without_changing_semantics() {
    let (s, l) = fixture("s1a/region_arc.gbr");
    let before = s.clone();
    let a = Scene::build(&s, &l, MmPoint::new(0., 0.), 10., 1).unwrap();
    let b = Scene::build(&s, &l, MmPoint::new(0., 0.), 1000., 2).unwrap();
    assert!(b.points.len() > a.points.len());
    assert_eq!(s, before);
}
#[test]
fn renderer_refuses_numeric_precision_and_resource_overflow() {
    let (s, l) = fixture("s1a/region_arc.gbr");
    assert!(Scene::build(&s, &l, MmPoint::new(1e12, 1e12), 1000., 1).is_err());
    assert!(Scene::build(&s, &l, MmPoint::new(0., 0.), 1e30, 1).is_err());
}

#[test]
#[ignore = "requires native Metal hardware; retain raw output"]
fn native_metal_semantic_renderer() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("Metal adapter");
    println!("S2-A.3 GPU {:?}", adapter.get_info());
    assert_ne!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let source = format!(
        "{}\n{}",
        include_str!("editor.wgsl"),
        r#"
@group(0) @binding(4) var<storage,read> probes:array<vec2<f32>>;
@group(0) @binding(5) var<storage,read_write> results:array<u32>;
@compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3<u32>) {
    results[id.x]=select(0u,1u,length(sample_scene(probes[id.x])-vec3(0.055,0.072,0.085))>0.01);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("semantic-native-probes"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    for name in [
        "s2a3/gui_primitives.gbr",
        "s1a/standard_hole_over_line.gbr",
        "s1a/macro_hole_over_line.gbr",
        "s1a/ordered_local_hole.gbr",
        "s1a/macro_rotated_circle.gbr",
        "s1a/macro_rotated_rectangle.gbr",
        "s1a/region_cutin.gbr",
        "s1a/nested_region_union.gbr",
        "s0c/rectangular_draw.gbr",
        "s1a1/g75_exact.gbr",
        "s1a1/g75_small_deviation.gbr",
        "s1a1/g75_full.gbr",
        "s1a1/g74_zero.gbr",
        "s1a1/g75_region_deviation_safe.gbr",
    ] {
        let (s, l) = fixture(name);
        let scene = Scene::build(&s, &l, MmPoint::new(0., 0.), 1000., 1).unwrap();
        // Create the actual render pipeline too: compute probes alone do not validate fragment IO.
        let _render = crate::gpu::Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm, &scene);
        let doc = SemanticDocument {
            id: s.document_id.clone(),
            unit: "mm".into(),
            format: SemanticFormat {
                integer: 6,
                decimal: 6,
                leading_zero_omission: true,
                absolute: true,
            },
            layers: s.layers,
            apertures: s.apertures,
            source: Default::default(),
        };
        let b = doc.manufacturing_bounds(None).unwrap().unwrap();
        let mut probes = Vec::<[f32; 2]>::new();
        let mut expected = Vec::<bool>::new();
        // Offset grid avoids deliberately ambiguous manufacturing/pixel boundaries.
        for y in 0..29 {
            for x in 0..31 {
                let p = MmPoint::new(
                    b.min_x_mm + (b.max_x_mm - b.min_x_mm) * (f64::from(x) + 0.371) / 31.,
                    b.min_y_mm + (b.max_y_mm - b.min_y_mm) * (f64::from(y) + 0.419) / 29.,
                );
                let p = MmPoint::new(f64::from(p.x_mm as f32), f64::from(p.y_mm as f32));
                probes.push([p.x_mm as f32, p.y_mm as f32]);
                expected.push(
                    doc.layers
                        .iter()
                        .any(|l| doc.layer_coverage_at(&l.id, p) == Some(true)),
                );
            }
        }
        // Independent known truth for a standard or macro hole over a preexisting line.
        if name.ends_with("hole_over_line.gbr") {
            for (x, y, hit) in [
                (0., 0., true),
                (0., 0.5, false),
                (0., 1.5, true),
                (0., 2.5, false),
            ] {
                probes.push([x, y]);
                expected.push(hit);
            }
        }
        let uniform = Uniforms {
            view: [0.; 4],
            camera: [0.; 4],
            counts: [scene.objects.len() as u32, 0, 0, 0],
        };
        let buf = |bytes: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytes,
                usage,
            })
        };
        let objects = if scene.objects.is_empty() {
            vec![Default::default()]
        } else {
            scene.objects.clone()
        };
        let shapes = if scene.primitives.is_empty() {
            vec![Default::default()]
        } else {
            scene.primitives.clone()
        };
        let points = if scene.points.is_empty() {
            vec![[0f32; 2]]
        } else {
            scene.points.clone()
        };
        let buffers = [
            buf(bytemuck::bytes_of(&uniform), wgpu::BufferUsages::UNIFORM),
            buf(bytemuck::cast_slice(&objects), wgpu::BufferUsages::STORAGE),
            buf(bytemuck::cast_slice(&shapes), wgpu::BufferUsages::STORAGE),
            buf(bytemuck::cast_slice(&points), wgpu::BufferUsages::STORAGE),
            buf(bytemuck::cast_slice(&probes), wgpu::BufferUsages::STORAGE),
        ];
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (probes.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: out.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 5,
            resource: out.as_entire_binding(),
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(probes.len() as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&out, 0, &read, 0, out.size());
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        let mapped = read.slice(..).get_mapped_range();
        let actual: &[u32] = bytemuck::cast_slice(&mapped);
        for (i, e) in expected.iter().enumerate() {
            assert_eq!(actual[i] != 0, *e, "{name} point {:?}", probes[i]);
        }
        println!(
            "PASS {name} {} Metal/semantic coverage probes",
            probes.len()
        );
    }
}

#[test]
fn invalid_zero_length_rectangle_remains_rejected() {
    let mut m = Model::default();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s1a/zero_length_rectangular_sweep.gbr");
    assert_eq!(m.open(&p).unwrap_err().code, "VALIDATION_FAILED");
    assert!(m.view.info.is_none());
}

#[test]
fn public_gui_sample_has_real_standard_and_macro_geometry() {
    let (s, l) = fixture("s2a3/gui_primitives.gbr");
    assert!(
        s.apertures
            .iter()
            .any(|a| matches!(a.shape, ApertureShape::Polygon { .. }))
    );
    assert!(
        s.apertures
            .iter()
            .any(|a| matches!(a.shape, ApertureShape::Obround { .. }))
    );
    assert!(Scene::build(&s, &l, MmPoint::new(30., 30.), 1000., 1).is_ok());
}
