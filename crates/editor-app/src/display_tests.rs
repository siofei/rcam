use crate::{display::Scene, gpu::Uniforms, state::Model};
use editor_core::*;
use editor_service::{
    BlockTransformParams, CreateBlockDefinitionParams, CreateBlockInstanceParams, LayerInfo,
    PivotMm, QueryParams, RenderSnapshot,
};
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
/// A definition (Flash + Line + Arc + Region, via the `MIXED` fixture also
/// used by `block_core_workflow.rs`) with 2 instances — identity, and
/// rotated 90° + mirrored — so display coverage exercises every resolved
/// primitive kind at more than one orientation. Returns the snapshot, the
/// layer workspace, and the shared `object_id` both instances' resolved
/// primitives report through `scene.ids`.
fn block_fixture() -> (RenderSnapshot, Vec<LayerInfo>, Vec<String>) {
    const MIXED: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX2000000Y2000000D03*\nX0Y0D02*\nG01X2000000Y0D01*\nG75*\nX1000000Y0D02*\nG03X0Y1000000I-1000000J0D01*\nG36*\nX10000000Y10000000D02*\nG01X12000000Y10000000D01*\nX12000000Y12000000D01*\nX10000000Y12000000D01*\nX10000000Y10000000D01*\nG37*\nM02*\n";
    let dir = std::env::temp_dir().join(format!("rcam-b2-display-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mixed.gbr");
    std::fs::write(&path, MIXED).unwrap();
    let mut m = Model::default();
    m.open(&path).unwrap();
    let doc_id = m.view.info.as_ref().unwrap().document_id.clone();
    let layer_id = m.view.layers[0].layer_id.clone();
    // `blocks_*` is called on `m.service` directly (no Block Editor GUI
    // exists yet), which does not go through `Model`'s own refresh, so the
    // revision must be re-read from the service itself, not `m.view.info`.
    let rev = |m: &Model| m.service.document_get(&doc_id).unwrap().revision;
    let object_ids: Vec<String> = m
        .service
        .objects_query(
            &doc_id,
            QueryParams {
                layer_id: layer_id.clone(),
                geometry_type: None,
                region_mm: None,
                relation: None,
                limit: Some(1000),
                cursor: None,
            },
        )
        .unwrap()
        .objects
        .into_iter()
        .map(|o| o.object.object_id)
        .collect();
    assert_eq!(object_ids.len(), 4, "Flash + Line + Arc + Region");
    let revision = rev(&m);
    let create = m
        .service
        .blocks_create_definition_from_objects(
            &doc_id,
            &revision,
            CreateBlockDefinitionParams {
                layer_id: layer_id.clone(),
                object_ids,
                local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
                name: "mixed".into(),
            },
        )
        .unwrap();
    let revision = rev(&m);
    m.service
        .blocks_create_instance(
            &doc_id,
            &revision,
            CreateBlockInstanceParams {
                layer_id: layer_id.clone(),
                definition_id: create.definition_id.clone(),
                transform: BlockTransformParams {
                    translation_mm: PivotMm {
                        x_mm: 20.,
                        y_mm: 0.,
                    },
                    rotation_deg: 90.,
                    mirror: true,
                },
            },
        )
        .unwrap();
    let object_ids: Vec<String> = m
        .service
        .objects_query(
            &doc_id,
            QueryParams {
                layer_id: layer_id.clone(),
                geometry_type: None,
                region_mm: None,
                relation: None,
                limit: Some(1000),
                cursor: None,
            },
        )
        .unwrap()
        .objects
        .into_iter()
        .map(|o| o.object.object_id)
        .collect();
    assert_eq!(object_ids.len(), 2, "the original instance + the new one");
    let s = m.service.render_snapshot(&doc_id).unwrap();
    (s, m.view.layers.clone(), object_ids)
}
#[test]
fn block_instance_resolves_display_across_modes_color_and_selection() {
    let (snapshot, layers, instance_ids) = block_fixture();
    assert_eq!(snapshot.block_definitions.len(), 1, "one shared definition");

    for mode in [
        workspace::LayerDisplayMode::Filled,
        workspace::LayerDisplayMode::Outline,
        workspace::LayerDisplayMode::ZeroWidth,
    ] {
        let mut layers = layers.clone();
        layers[0].display_mode = mode;
        let scene = Scene::build(&snapshot, &layers, MmPoint::new(0., 0.), 100., 1)
            .unwrap_or_else(|e| panic!("BlockInstance must display under {mode:?}: {e}"));
        assert!(!scene.primitives.is_empty());
        // 2 instances x 4 resolved primitives (Flash + Line + Arc + Region) each.
        assert_eq!(scene.ids.len(), 8);
        for id in &instance_ids {
            assert_eq!(
                scene.ids.iter().filter(|s| *s == id).count(),
                4,
                "every resolved primitive of instance {id} must share its object_id \
                 (so gpu::selection_flags, which matches by id, flags all of them)"
            );
        }
        // A block's own class is never Stroke, so under ZeroWidth its
        // resolved content keeps its true width (outline, not centerline)
        // instead of mixing per-primitive-kind hairline treatment within
        // one shared category (S4-B2 Final Closeout §2.3).
        let expected_mode = match mode {
            workspace::LayerDisplayMode::Filled => crate::display::MODE_FILLED,
            _ => crate::display::MODE_EDGE,
        };
        for object in &scene.objects {
            assert_eq!(object.style[1], expected_mode);
        }
    }

    // Rotation + mirror actually move the second instance's geometry.
    let scene = Scene::build(&snapshot, &layers, MmPoint::new(0., 0.), 100., 1).unwrap();
    let first = &scene.objects[0..4];
    let second = &scene.objects[4..8];
    assert_ne!(
        first.iter().map(|o| o.bounds).collect::<Vec<_>>(),
        second.iter().map(|o| o.bounds).collect::<Vec<_>>(),
        "the translated + rotated + mirrored instance must render at a different place"
    );

    // Building twice must be idempotent: the per-build BlockDisplayCache
    // must not leak state or change results across calls.
    let again = Scene::build(&snapshot, &layers, MmPoint::new(0., 0.), 100., 1).unwrap();
    assert_eq!(scene.ids, again.ids);
    assert_eq!(
        scene.objects.iter().map(|o| o.bounds).collect::<Vec<_>>(),
        again.objects.iter().map(|o| o.bounds).collect::<Vec<_>>()
    );
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
    metal_probes(0);
}
#[test]
#[ignore = "requires native Metal hardware; retain raw output"]
fn native_metal_drag_preview() {
    metal_probes(1);
}
#[test]
#[ignore = "requires native Metal hardware; retain raw output"]
fn native_metal_multi_drag_preview() {
    metal_probes(usize::MAX);
}
fn metal_probes(selected_count: usize) {
    let preview = selected_count > 0;
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
        include_str!("reference.wgsl"),
        r#"
@group(0) @binding(5) var<storage,read> probes:array<vec2<f32>>;
@group(0) @binding(6) var<storage,read_write> results:array<u32>;
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
        let mut doc = SemanticDocument {
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
            block_definitions: Vec::new(),
        };
        if preview {
            let layer = doc.layers[0].id.clone();
            let ids: Vec<_> = doc.layers[0]
                .objects
                .iter()
                .take(selected_count)
                .map(|o| o.object_id.clone())
                .collect();
            editor_core::edit::EditHistory::default()
                .move_objects(&mut doc, &layer, &ids, 3., -2.)
                .unwrap();
        }
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
        if !preview && name.ends_with("hole_over_line.gbr") {
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
            grid: [0.; 4],
            world: [0.; 4],
            preview: if preview { [3., -2., 0., 0.] } else { [0.; 4] },
            view: [0.; 4],
            camera: [0.; 4],
            counts: [scene.objects.len() as u32, u32::from(preview), 0, 0],
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
        let selected: Vec<u32> = (0..objects.len())
            .map(|i| u32::from(i < selected_count))
            .collect();
        let buffers = [
            buf(bytemuck::bytes_of(&uniform), wgpu::BufferUsages::UNIFORM),
            buf(bytemuck::cast_slice(&objects), wgpu::BufferUsages::STORAGE),
            buf(bytemuck::cast_slice(&shapes), wgpu::BufferUsages::STORAGE),
            buf(bytemuck::cast_slice(&points), wgpu::BufferUsages::STORAGE),
            buf(bytemuck::cast_slice(&selected), wgpu::BufferUsages::STORAGE),
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
            binding: 6,
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

struct ParityRig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: Vec<wgpu::ComputePipeline>,
}

fn parity_rig() -> ParityRig {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = block_on(instance.request_adapter(&Default::default())).unwrap();
    println!("parity adapter {:?}", adapter.get_info());
    let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
    let sources = [include_str!("reference.wgsl"), include_str!("editor.wgsl")];
    let pipelines: Vec<_> = sources
        .iter()
        .map(|source| {
            let source = source.replace(
                "@fragment fn fs_main(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32>",
                "fn shade(pos:vec4<f32>)->vec4<f32>",
            );
            let source = format!(
                "{source}\n{}",
                r#"
@group(0) @binding(6) var<storage,read_write> rgba:array<vec4<f32>>;
@compute @workgroup_size(8,8) fn parity(@builtin(global_invocation_id) id:vec3<u32>) {
 if id.x>=128u || id.y>=128u {return;}
 rgba[id.y*128u+id.x]=shade(vec4<f32>(vec2<f32>(id.xy)+vec2(0.5),0.,1.));
}"#
            );
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("parity"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &shader,
                entry_point: Some("parity"),
                compilation_options: Default::default(),
                cache: None,
            })
        })
        .collect();
    ParityRig {
        device,
        queue,
        pipelines,
    }
}

/// Reference vs production shader on one scene: selection {0, 1, all} x drag
/// {none, moved} x three cameras, all compared as exact RGBA.
fn assert_parity(rig: &ParityRig, scene: &Scene, name: &str) {
    assert_parity_cameras(rig, scene, name, None);
}

/// With `Some(render_ppm)` the three cameras stay inside the range the application
/// guarantees for a scene built at that ppm (`render_ppm / LOD_MAX_ZOOM_OUT ..= render_ppm`);
/// the coarsest one is the worst case for hairline bounds.
fn assert_parity_cameras(rig: &ParityRig, scene: &Scene, name: &str, render_ppm: Option<f64>) {
    let ParityRig {
        device,
        queue,
        pipelines,
    } = rig;
    let _render = crate::gpu::Resources::new(device, wgpu::TextureFormat::Rgba8Unorm, scene);
    let b = scene.objects.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |mut b, o| {
            for i in 0..2 {
                b[i] = b[i].min(o.bounds[i]);
                b[i + 2] = b[i + 2].max(o.bounds[i + 2]);
            }
            b
        },
    );
    for selection in [0, 1, scene.ids.len()] {
        for delta in [MmPoint::new(0., 0.), MmPoint::new(3., -2.)] {
            for view_mode in 0..3 {
                let mut camera = crate::camera::Camera {
                    center: MmPoint::new(
                        f64::from((b[0] + b[2]) / 2.),
                        f64::from((b[1] + b[3]) / 2.),
                    ),
                    scale: 90. / f64::from((b[2] - b[0]).max(b[3] - b[1]).max(1.)),
                };
                if let Some(ppm) = render_ppm {
                    camera.scale = match view_mode {
                        0 => ppm / crate::display::LOD_MAX_ZOOM_OUT,
                        1 => ppm / 2.,
                        _ => ppm,
                    };
                    if view_mode == 2 {
                        camera.center.x_mm += f64::from(b[2] - b[0]) * 0.45 / 8.;
                    }
                } else {
                    if view_mode == 1 {
                        camera.scale *= 5.;
                        camera.center = MmPoint::new(5., 5.);
                    }
                    if view_mode == 2 {
                        if name.contains("SPARSE_DENSE") {
                            camera.scale = 8.;
                            camera.center = MmPoint::new(1000., 1000.);
                        } else {
                            camera.center.x_mm += f64::from(b[2] - b[0]) * 0.45;
                            camera.scale *= 2.;
                        }
                    }
                }
                let ids: Vec<_> = scene
                    .ids
                    .iter()
                    .take(selection)
                    .map(String::as_str)
                    .collect();
                let (uniform, index) = crate::gpu::prepare(
                    scene,
                    camera,
                    eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        eframe::egui::vec2(128., 128.),
                    ),
                    1.,
                    &crate::gpu::selection_flags(scene, &ids),
                    delta,
                )
                .unwrap();
                let flags = crate::gpu::selection_flags(scene, &ids);
                let buf = |data: &[u8], usage| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: if data.is_empty() { &[0; 16] } else { data },
                        usage,
                    })
                };
                let buffers = [
                    buf(bytemuck::bytes_of(&uniform), wgpu::BufferUsages::UNIFORM),
                    buf(
                        bytemuck::cast_slice(&scene.objects),
                        wgpu::BufferUsages::STORAGE,
                    ),
                    buf(
                        bytemuck::cast_slice(&scene.primitives),
                        wgpu::BufferUsages::STORAGE,
                    ),
                    buf(
                        bytemuck::cast_slice(&scene.points),
                        wgpu::BufferUsages::STORAGE,
                    ),
                    buf(bytemuck::cast_slice(&flags), wgpu::BufferUsages::STORAGE),
                    buf(
                        bytemuck::cast_slice(&index.data),
                        wgpu::BufferUsages::STORAGE,
                    ),
                ];
                let mut images = Vec::new();
                for (mode, pipeline) in pipelines.iter().enumerate() {
                    let output = device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: 128 * 128 * 16,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    });
                    let read = device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: output.size(),
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let mut entries: Vec<_> = buffers
                        .iter()
                        .take(if mode == 0 { 5 } else { 6 })
                        .enumerate()
                        .map(|(i, b)| wgpu::BindGroupEntry {
                            binding: i as u32,
                            resource: b.as_entire_binding(),
                        })
                        .collect();
                    entries.push(wgpu::BindGroupEntry {
                        binding: 6,
                        resource: output.as_entire_binding(),
                    });
                    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: None,
                        layout: &pipeline.get_bind_group_layout(0),
                        entries: &entries,
                    });
                    let mut encoder = device.create_command_encoder(&Default::default());
                    {
                        let mut pass = encoder.begin_compute_pass(&Default::default());
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, &group, &[]);
                        pass.dispatch_workgroups(16, 16, 1);
                    }
                    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, output.size());
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
                    rx.recv().unwrap().unwrap();
                    images.push(read.slice(..).get_mapped_range().to_vec());
                }
                let differences = images[0]
                    .chunks_exact(16)
                    .zip(images[1].chunks_exact(16))
                    .filter(|(a, b)| a != b)
                    .count();
                if differences > 0 {
                    for (i, (a, b)) in images[0]
                        .chunks_exact(16)
                        .zip(images[1].chunks_exact(16))
                        .enumerate()
                        .filter(|(_, (a, b))| a != b)
                        .take(8)
                    {
                        println!(
                            "DIFF {i} {:?} {:?}",
                            bytemuck::cast_slice::<u8, f32>(a),
                            bytemuck::cast_slice::<u8, f32>(b)
                        );
                    }
                }
                assert_eq!(
                    differences, 0,
                    "{name} selection={selection} delta={delta:?}"
                );
                println!(
                    "PASS exact RGBA parity {name} selection={selection} delta={delta:?} view={view_mode} pixels=16384"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires native Metal; exact RGBA parity including AA and selection"]
fn native_metal_reference_production_pixel_parity() {
    let rig = parity_rig();
    for name in [
        "s2b3_2/SPARSE_DENSE.gbr",
        "s2b3_1/P1K_CIRCLES.gbr",
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
        let (mut snapshot, mut layers) = fixture(name);
        if name == "s1a1/g75_region_deviation_safe.gbr" {
            // A 1 mm-wide Region with a radius-100 mm shallow arc. The render
            // envelope must follow its sweep, never the full circle.
            let a = MmPoint::new(-0.5, 0.);
            let b = MmPoint::new(0.5, 0.);
            let c = MmPoint::new(0.5, -1.);
            let d = MmPoint::new(-0.5, -1.);
            snapshot.layers[0].objects.push(SemanticObject {
                object_id: "shallow-arc-region".into(),
                exposure: Exposure::Dark,
                origin: ObjectOrigin::Imported { command_index: 0 },
                geometry: SemanticGeometry::Region {
                    contours: vec![RegionContour {
                        role: RegionRole::Solid,
                        edges: vec![
                            RegionEdge::Arc(ArcGeometry {
                                start: a,
                                end: b,
                                center: MmPoint::new(0., 100.),
                                direction: ArcDirection::CounterClockwise,
                                full_circle: false,
                                source: None,
                            }),
                            RegionEdge::Line { start: b, end: c },
                            RegionEdge::Line { start: c, end: d },
                            RegionEdge::Line { start: d, end: a },
                        ],
                    }],
                },
            });
        }
        // Add a separate layer with Clear over the first layer's Dark.
        let mut upper = snapshot.layers[0].clone();
        upper.id = "parity-upper".into();
        for object in &mut upper.objects {
            object.object_id = format!("upper-{}", object.object_id);
            object.exposure = Exposure::Clear;
        }
        snapshot.layers.push(upper);
        let mut info = layers[0].clone();
        info.layer_id = "parity-upper".into();
        layers.push(info);
        let scene = Scene::build(
            &snapshot,
            &layers,
            MmPoint::new(0., 0.),
            if name.contains("SPARSE_DENSE") {
                100.
            } else {
                1000.
            },
            1,
        )
        .unwrap();
        assert_parity(&rig, &scene, name);
    }
}

/// Builds the display scene of a real multi-layer workspace after `setup` applied
/// workspace/view changes through the service (the path the GUI takes).
fn workspace_scene(files: &[&str], setup: impl Fn(&mut Model, &[String]), ppm: f64) -> Scene {
    use crate::state::Action;
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
    m.run(Action::ImportGerbers(
        files.iter().map(|f| root.join(f)).collect(),
    ));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let ids: Vec<String> = m
        .view
        .import
        .as_ref()
        .expect("import result")
        .layers
        .iter()
        .map(|l| l.layer_id.clone())
        .collect();
    setup(&mut m, &ids);
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let snapshot = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    Scene::build(&snapshot, &m.view.layers, MmPoint::new(0., 0.), ppm, 1).unwrap()
}

fn layer_patch(m: &Model, id: &str) -> editor_service::LayerUpdateParams {
    editor_service::LayerUpdateParams {
        layer_id: id.into(),
        expected_workspace_revision: m.view.info.as_ref().unwrap().workspace_revision.clone(),
        ..Default::default()
    }
}

type Setup = Box<dyn Fn(&mut Model, &[String])>;

/// Fixed, representative S4-B1 view-style workspaces (not a Cartesian product).
fn view_style_cases() -> Vec<(&'static str, &'static [&'static str], Setup)> {
    use crate::state::Action;
    use editor_core::workspace::{ColorMode, DisplayClass, LayerDisplayMode as Mode};
    use editor_service::ClassStyleUpdate;
    fn mode(m: &mut Model, id: &str, display: Mode, color: ColorMode) {
        let mut p = layer_patch(m, id);
        p.display_mode = Some(display);
        p.color_mode = Some(color);
        m.run(Action::Layer(p));
    }
    fn class(m: &mut Model, id: &str, update: ClassStyleUpdate) {
        let mut p = layer_patch(m, id);
        p.classes = vec![update];
        m.run(Action::Layer(p));
    }
    const TWO: [&str; 2] = ["s2a3/gui_primitives.gbr", "s1a/region_cutin.gbr"];
    const THREE: [&str; 3] = [
        "s2a3/gui_primitives.gbr",
        "s1a/region_cutin.gbr",
        "s1a/ordered_local_hole.gbr",
    ];
    vec![
        ("layer-color-filled", &TWO, Box::new(|_, _| {})),
        (
            "layer-color-outline",
            &TWO,
            Box::new(|m, ids| mode(m, &ids[0], Mode::Outline, ColorMode::LayerColor)),
        ),
        (
            "layer-color-zerowidth",
            &TWO,
            Box::new(|m, ids| mode(m, &ids[0], Mode::ZeroWidth, ColorMode::LayerColor)),
        ),
        (
            "category-color-filled",
            &TWO,
            Box::new(|m, ids| {
                for id in ids {
                    mode(m, id, Mode::Filled, ColorMode::CategoryColor);
                }
            }),
        ),
        (
            "category-color-outline-and-zerowidth",
            &TWO,
            Box::new(|m, ids| {
                mode(m, &ids[0], Mode::Outline, ColorMode::CategoryColor);
                mode(m, &ids[1], Mode::ZeroWidth, ColorMode::CategoryColor);
            }),
        ),
        (
            "category-color-override",
            &TWO,
            Box::new(|m, ids| {
                mode(m, &ids[0], Mode::Filled, ColorMode::CategoryColor);
                for (c, hex) in [
                    (DisplayClass::Stroke, "#ff8800"),
                    (DisplayClass::RegionFreeform, "#00ffaa"),
                    (DisplayClass::FlashCircle, "#3366ff"),
                ] {
                    class(
                        m,
                        &ids[0],
                        ClassStyleUpdate {
                            class: Some(c),
                            color_override: Some(hex.into()),
                            ..Default::default()
                        },
                    );
                }
            }),
        ),
        (
            "layer-visible-off",
            &TWO,
            Box::new(|m, ids| {
                let mut p = layer_patch(m, &ids[1]);
                p.visible = Some(false);
                m.run(Action::Layer(p));
            }),
        ),
        (
            "class-visible-off-outline",
            &TWO,
            Box::new(|m, ids| {
                mode(m, &ids[0], Mode::Outline, ColorMode::CategoryColor);
                for c in [DisplayClass::Stroke, DisplayClass::FlashCircle] {
                    class(
                        m,
                        &ids[0],
                        ClassStyleUpdate {
                            class: Some(c),
                            visible: Some(false),
                            ..Default::default()
                        },
                    );
                }
            }),
        ),
        (
            "z-order-three-layers-mixed-modes",
            &THREE,
            Box::new(|m, ids| {
                mode(m, &ids[0], Mode::Filled, ColorMode::LayerColor);
                mode(m, &ids[1], Mode::Outline, ColorMode::CategoryColor);
                mode(m, &ids[2], Mode::ZeroWidth, ColorMode::LayerColor);
                let mut order = ids.to_vec();
                order.reverse();
                m.run(Action::ReorderLayers(order));
            }),
        ),
        (
            "solo-hides-the-others",
            &THREE,
            Box::new(|m, ids| m.run(Action::SetSoloLayer(Some(ids[1].clone())))),
        ),
    ]
}

/// S4-B1 view styles: Production and Reference must agree bit-for-bit on every
/// pixel of every representative workspace.
#[test]
#[ignore = "requires native Metal; exact RGBA parity of S4-B1 view styles"]
fn native_metal_s4b1_view_style_parity_matrix() {
    let cases = view_style_cases();
    let rig = parity_rig();
    for (label, files, setup) in &cases {
        // Two passes: the first only measures the extent, the second is built at the
        // ppm whose supported zoom range [ppm/4, ppm] contains all three cameras.
        let extent = workspace_scene(files, setup, 1000.)
            .objects
            .iter()
            .fold(0_f32, |m, o| {
                m.max(o.bounds[2] - o.bounds[0])
                    .max(o.bounds[3] - o.bounds[1])
            });
        let render_ppm = 4. * 90. / f64::from(extent.max(1.));
        let scene = workspace_scene(files, setup, render_ppm);
        assert!(!scene.objects.is_empty(), "{label}");
        assert_parity_cameras(&rig, &scene, &format!("s4b1/{label}"), Some(render_ppm));
        println!(
            "S4B1_VIEW_STYLE_CASE_OK {label} objects={}",
            scene.objects.len()
        );
    }
    println!(
        "S4B1_VIEW_STYLE_MATRIX cases={} all exact RGBA",
        cases.len()
    );
}

/// Guards the matrix against silently testing nothing: every style and the
/// hidden / z-order variations must really reach the display scene.
#[test]
fn s4b1_view_style_matrix_scenes_exercise_every_style() {
    use crate::display::{MODE_CENTERLINE, MODE_EDGE, MODE_FILLED};
    let mut modes = std::collections::HashSet::new();
    let mut hidden = 0;
    for (label, files, setup) in view_style_cases() {
        let scene = workspace_scene(files, setup, 1000.);
        assert!(!scene.objects.is_empty(), "{label}");
        let colors: std::collections::HashSet<_> =
            scene.objects.iter().map(|o| o.style[0]).collect();
        if label.starts_with("category-color") {
            assert!(colors.len() > 1, "{label}: category colours must differ");
        }
        if label == "layer-color-outline" {
            assert!(
                scene.objects.iter().any(|o| o.style[1] == MODE_EDGE),
                "{label}"
            );
        }
        for o in &scene.objects {
            modes.insert(o.style[1]);
            hidden += usize::from(o.meta[3] == 0);
        }
    }
    assert!(modes.contains(&MODE_FILLED));
    assert!(modes.contains(&MODE_EDGE));
    assert!(modes.contains(&MODE_CENTERLINE));
    assert!(
        hidden > 0,
        "hidden layer/class/solo objects are in the matrix"
    );
}

#[test]
fn thousand_circles_metrics_writer_navigation_preview_and_history() {
    use crate::{camera::Camera, gpu, state::Action};
    use eframe::egui::{Rect, pos2, vec2};
    let mut model = Model::default();
    model
        .open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/s2b3_1/P1K_CIRCLES.gbr"),
        )
        .unwrap();
    model.run(Action::SelectRect(
        model.view.bounds.unwrap(),
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert_eq!(model.view.selected.ordered.len(), 1000);
    let info = model.view.info.clone().unwrap();
    let layer = model.view.layers[0].layer_id.clone();
    let ids: Vec<_> = model
        .view
        .selected
        .ids()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let request=serde_json::json!({"api_version":1,"request_id":"renderer-invariance","op":"objects.metrics","document_id":info.document_id,"params":{"layer_id":layer,"object_ids":ids}}).to_string();
    let before = model.service.execute_json(&request);
    assert_eq!(before["result"]["summary"]["exact_count"], 1000);
    assert_eq!(before["result"]["summary"]["unsupported_count"], 0);
    assert!(
        (before["result"]["summary"]["object_area_sum_mm2"]
            .as_f64()
            .unwrap()
            - 1000. * std::f64::consts::PI / 16.)
            .abs()
            < 1e-8
    );
    assert!(
        (before["result"]["summary"]["object_perimeter_sum_mm"]
            .as_f64()
            .unwrap()
            - 500. * std::f64::consts::PI)
            .abs()
            < 1e-8
    );
    let dir = std::env::temp_dir().join(format!("rcam-render-invariance-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let export = |model: &mut Model, name: &str| {
        let path = dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        model.run(Action::Save(path.clone(), layer.clone(), None));
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        std::fs::read(path).unwrap()
    };
    let bytes = export(&mut model, "before.gbr");
    let rect = Rect::from_min_size(pos2(0., 0.), vec2(1600., 900.));
    let mut camera = Camera::default();
    camera.fit(model.view.bounds, rect);
    let index = model.view.scene.as_ref().unwrap().index.clone();
    let baseline = model.view.info.clone();
    for n in 0..60 {
        camera.pan(vec2(1., -1.));
        camera.zoom(if n % 2 == 0 { 1.1 } else { 1. / 1.1 }, rect.center(), rect);
        model.run(Action::Rebuild(camera.scale * 2.));
        let scene = model.view.scene.as_ref().unwrap();
        assert!(Arc::ptr_eq(&index, &scene.index));
        gpu::prepare(
            scene,
            camera,
            rect,
            1.,
            &gpu::selection_flags(scene, &model.view.selected.ids()),
            MmPoint::new(n as f64 / 10., -2.),
        )
        .unwrap();
        assert_eq!(model.view.info, baseline);
    }
    assert_eq!(model.service.execute_json(&request), before);
    assert_eq!(export(&mut model, "after.gbr"), bytes);
    let snapshot = model.service.render_snapshot(&info.document_id).unwrap();
    let mut drag = crate::drag::Drag::arm(&model.view, rect.center(), camera, rect, 1.).unwrap();
    drag.confirmed = true;
    drag.update(rect.center() + vec2(100., -50.));
    let delta = drag.delta;
    model.run(drag.release().unwrap());
    assert!(model.view.error.is_none());
    assert_eq!(
        model.view.info.as_ref().unwrap().undo_entries,
        info.undo_entries + 1
    );
    let moved = model.service.render_snapshot(&info.document_id).unwrap();
    for (old, new) in snapshot.layers[0]
        .objects
        .iter()
        .zip(&moved.layers[0].objects)
    {
        if let (
            SemanticGeometry::Flash { center: a, .. },
            SemanticGeometry::Flash { center: b, .. },
        ) = (&old.geometry, &new.geometry)
        {
            assert!((b.x_mm - a.x_mm - delta.x_mm).abs() < 1e-10);
            assert!((b.y_mm - a.y_mm - delta.y_mm).abs() < 1e-10);
        } else {
            panic!("expected circle");
        }
    }
    model.run(Action::History(false));
    assert_eq!(
        model
            .service
            .render_snapshot(&info.document_id)
            .unwrap()
            .layers,
        snapshot.layers
    );
    println!(
        "P1K 1600x900 candidate_max={} metrics={}",
        index.max_candidates, before
    );
}

#[test]
#[ignore = "native Metal release-only offscreen performance supplement, not GUI AT-075"]
fn native_metal_p1k_offscreen_timing() {
    assert!(!cfg!(debug_assertions), "run --release");
    use eframe::egui::{Rect, pos2, vec2};
    use egui_wgpu::CallbackTrait;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = block_on(instance.request_adapter(&Default::default())).unwrap();
    println!("P1K offscreen adapter {:?}", adapter.get_info());
    let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
    let (snapshot, layers) = fixture("s2b3_1/P1K_CIRCLES.gbr");
    let scene = Arc::new(Scene::build(&snapshot, &layers, MmPoint::new(0., 0.), 100., 1).unwrap());
    let rect = Rect::from_min_size(pos2(0., 0.), vec2(1600., 900.));
    let camera = crate::camera::Camera {
        center: MmPoint::new(20.5, 13.),
        scale: 30.,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("P1K 1600x900"),
        size: wgpu::Extent3d {
            width: 1600,
            height: 900,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut resources = egui_wgpu::CallbackResources::default();
    let selected: Vec<_> = scene.ids.iter().map(String::as_str).collect();
    for round in 0..3 {
        let start = std::time::Instant::now();
        let mut frames = Vec::new();
        while start.elapsed().as_secs_f64() < 10. {
            let frame = std::time::Instant::now();
            let delta = MmPoint::new((start.elapsed().as_secs_f64() * 2.).sin() * 2., 1.);
            let (uniforms, index) = crate::gpu::prepare(
                &scene,
                camera,
                rect,
                1.,
                &crate::gpu::selection_flags(&scene, &selected),
                delta,
            )
            .unwrap();
            let callback = crate::gpu::Callback {
                painted: None,
                scene: scene.clone(),
                index,
                uniforms,
                selected: Arc::new(vec![1; 1000]),
                format: wgpu::TextureFormat::Rgba8Unorm,
            };
            let mut encoder = device.create_command_encoder(&Default::default());
            callback.prepare(
                &device,
                &queue,
                &egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [1600, 900],
                    pixels_per_point: 1.,
                },
                &mut encoder,
                &mut resources,
            );
            let prepared = frame.elapsed().as_secs_f64() * 1000.;
            {
                let r = resources.get::<crate::gpu::Resources>().unwrap();
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&r.pipeline);
                pass.set_bind_group(0, &r.bind, &[]);
                pass.draw(0..3, 0..1);
            }
            let submission = queue.submit([encoder.finish()]);
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(Duration::from_secs(30)),
                })
                .unwrap();
            let ms = frame.elapsed().as_secs_f64() * 1000.;
            frames.push(ms);
            println!(
                "P1K_RAW round={round} frame={} cpu_prepare_ms={prepared:.6} cpu_plus_gpu_fence_ms={ms:.6}",
                frames.len()
            );
        }
        frames.sort_by(f64::total_cmp);
        let p95 = frames[(frames.len() as f64 * 0.95).ceil() as usize - 1];
        println!(
            "P1K_OFFSCREEN round={round} frames={} p95_ms={p95:.6} threshold_50ms_met={}",
            frames.len(),
            p95 <= 50.
        );
    }
}
