use super::*;
use editor_core::{CircleAperture, Document, DrawObject, Exposure, Layer, MmPoint};
use egui_wgpu::wgpu::{self, util::DeviceExt};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park_timeout(Duration::from_secs(1)),
        }
    }
}

fn scene(geometry: Geometry) -> DocumentSnapshot {
    let mut document = Document::new("gpu-test");
    let mut layer = Layer::new("one", "one");
    layer.objects.push(DrawObject {
        object_id: "one".into(),
        geometry,
        exposure: Exposure::Dark,
    });
    document.layers.push(layer);
    document.snapshot(0)
}

#[test]
fn checked_gpu_conversion_refuses_overflow_underflow_and_lost_holes() {
    for (outer, hole) in [
        (1e100, None),
        (f64::from(1e30_f32) * 2.0, None),
        (1e-25, Some(1e-26)),
        (1e-100, None),
        (1.0, Some(1e-100)),
        (1.0, Some(1.0 - 1e-10)),
    ] {
        let snapshot = scene(Geometry::CircleFlash {
            center: MmPoint::new(0.0, 0.0),
            aperture: CircleAperture::new(outer, hole).unwrap(),
        });
        assert!(
            gpu_objects(&snapshot).is_err(),
            "outer={outer}, hole={hole:?}"
        );
    }
    let mut valid = scene(Geometry::CircleFlash {
        center: MmPoint::new(0.0, 0.0),
        aperture: CircleAperture::new(1e-5, Some(1e-7)).unwrap(),
    });
    let (values, _) = gpu_objects(&valid).unwrap();
    assert!(values.iter().flatten().all(|x| x.is_finite()));
    assert!(values[1][1] > 0.0);
    valid.layers[0].objects[0].geometry = Geometry::Line {
        start: MmPoint::new(f64::MAX, 0.0),
        end: MmPoint::new(0.0, 0.0),
        width_mm: 1.0,
    };
    assert!(gpu_objects(&valid).is_err());
}

#[test]
fn short_line_projection_counterexample() {
    let length = 1e-4_f32;
    let old_amount = (length * length / (length * length).max(1e-6)).clamp(0.0, 1.0);
    assert!(
        (length - old_amount * length) > 1e-5,
        "old formula misses endpoint"
    );
    let corrected_amount = (length * length / (length * length)).clamp(0.0, 1.0);
    assert!((length - corrected_amount * length).abs() < 1e-10);
}

#[test]
#[ignore = "requires native Metal or DX12 hardware; run explicitly and retain raw output"]
fn native_gpu_coverage_regressions() {
    #[cfg(target_os = "macos")]
    let backends = wgpu::Backends::METAL;
    #[cfg(target_os = "windows")]
    let backends = wgpu::Backends::DX12;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("native GPU required");
    let info = adapter.get_info();
    println!(
        "Native GPU: {} / {:?} / {:?}",
        info.name, info.backend, info.device_type
    );
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu);
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    // The production coverage functions are executed directly on the native GPU.
    let source = format!(
        "{}\n{}",
        include_str!("canvas.wgsl"),
        r#"
@group(0) @binding(1) var<storage, read> probes: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> results: array<u32>;
@compute @workgroup_size(1)
fn probe(@builtin(global_invocation_id) id: vec3<u32>) {
    results[id.x] = select(0u, 1u, scene_coverage(probes[id.x].xy));
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production-coverage-test"),
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
    let short = scene(Geometry::Line {
        start: MmPoint::new(0.0, 0.0),
        end: MmPoint::new(1e-4, 0.0),
        width_mm: 2e-5,
    });
    let zero = scene(Geometry::Line {
        start: MmPoint::new(0.0, 0.0),
        end: MmPoint::new(0.0, 0.0),
        width_mm: 2e-5,
    });
    let ring = scene(Geometry::CircleFlash {
        center: MmPoint::new(0.0, 0.0),
        aperture: CircleAperture::new(1e-5, Some(1e-7)).unwrap(),
    });
    let mut service = ApplicationService::new();
    service.open_demo_s0("demo", SAMPLE).unwrap();
    let demo = service.snapshot("demo").unwrap();
    for (name, snapshot, probes) in [
        (
            "short line",
            short,
            vec![(0.0, 0.0, true), (1e-4, 0.0, true), (1e-4, 1e-4, false)],
        ),
        (
            "zero length",
            zero,
            vec![(0.0, 0.0, true), (5e-6, 0.0, true), (1e-4, 0.0, false)],
        ),
        (
            "tiny hole",
            ring,
            vec![(0.0, 0.0, false), (2e-8, 0.0, false), (2e-7, 0.0, true)],
        ),
        (
            "ordered local and cross-layer exposure",
            demo,
            vec![
                (0.0, 0.0, true),
                (2.0, 0.0, false),
                (4.0, 0.0, true),
                (12.0, 0.0, true),
                (12.0, 1.5, false),
                (-12.0, 0.0, true),
            ],
        ),
    ] {
        let (objects, object_count) = gpu_objects(&snapshot).unwrap();
        let uniform = CanvasUniforms {
            objects,
            object_count,
            ..bytemuck::Zeroable::zeroed()
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let points: Vec<[f32; 4]> = probes
            .iter()
            .map(|&(x, y, _)| [x as f32, y as f32, 0.0, 0.0])
            .collect();
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&points),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (probes.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: output.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(probes.len() as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output.size());
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let results: &[u32] = bytemuck::cast_slice(&mapped);
        for (i, &(x, y, expected)) in probes.iter().enumerate() {
            let cpu = snapshot
                .layers
                .iter()
                .any(|layer| layer.coverage_at(MmPoint::new(x, y)));
            println!(
                "{name} ({x}, {y}): expected={expected} cpu={cpu} gpu={}",
                results[i]
            );
            assert_eq!(cpu, expected);
            assert_eq!(results[i] != 0, expected, "{name} probe {i}");
        }
    }
}
