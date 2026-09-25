//! Ignored release benchmark for selected-object Grip generation; fixture setup is untimed.
use editor_core::grip::grip_features;
use editor_core::{
    ApertureDefinition, ApertureShape, Exposure, LocalTransform, MmPoint, ObjectOrigin,
    RegionContour, RegionEdge, RegionRole, SemanticDocument, SemanticFormat, SemanticGeometry,
    SemanticLayer, SemanticObject, SourceMetadata,
};
use std::{hint::black_box, time::Instant};

fn object(id: usize, geometry: SemanticGeometry) -> SemanticObject {
    SemanticObject {
        object_id: format!("object-{id}"),
        geometry,
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Generated {
            operation_id: "benchmark".into(),
        },
    }
}

fn flash(x: f64) -> SemanticGeometry {
    SemanticGeometry::Flash {
        center: MmPoint::new(x, 0.0),
        aperture_id: "a10".into(),
        transform: LocalTransform::default(),
    }
}

fn region_2k_edges() -> SemanticGeometry {
    let points: Vec<_> = (0..2_000)
        .map(|n| {
            let angle = std::f64::consts::TAU * f64::from(n) / 2_000.0;
            MmPoint::new(100.0 * angle.cos(), 100.0 * angle.sin())
        })
        .collect();
    let edges = (0..points.len())
        .map(|n| RegionEdge::Line {
            start: points[n],
            end: points[(n + 1) % points.len()],
        })
        .collect();
    SemanticGeometry::Region {
        contours: vec![RegionContour {
            edges,
            role: RegionRole::Solid,
        }],
    }
}

fn document(count: usize, selected_region: bool) -> SemanticDocument {
    let mut objects = Vec::with_capacity(count);
    objects.push(object(
        0,
        if selected_region {
            region_2k_edges()
        } else {
            flash(0.0)
        },
    ));
    objects.extend((1..count).map(|n| object(n, flash(n as f64))));
    SemanticDocument {
        id: "grip-performance".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 3,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: vec![SemanticLayer {
            id: "l1".into(),
            objects,
        }],
        apertures: vec![ApertureDefinition {
            id: "a10".into(),
            source_dcode: 10,
            shape: ApertureShape::Circle {
                diameter_mm: 1.0,
                hole_diameter_mm: None,
            },
        }],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}

fn measure(count: usize, selected_region: bool, iterations: usize) -> (usize, u128) {
    let doc = document(count, selected_region);
    let selected = &doc.layers[0].objects[0];
    let aperture = if selected_region {
        None
    } else {
        Some(&doc.apertures[0].shape)
    };
    let expected = if selected_region { 2_000 } else { 1 };
    assert_eq!(grip_features(selected, aperture).unwrap().len(), expected);
    let start = Instant::now();
    for _ in 0..iterations {
        let features = grip_features(black_box(selected), black_box(aperture)).unwrap();
        black_box(features);
    }
    let total_ns = start.elapsed().as_nanos();
    (expected, total_ns)
}

#[test]
#[ignore = "run with --release and RCAM_GRIP_PERF_OUT=<JSON path>"]
fn selected_object_grip_generation_is_bounded_by_selected_geometry() {
    assert!(!cfg!(debug_assertions), "release build required");
    let output = std::env::var_os("RCAM_GRIP_PERF_OUT")
        .expect("set RCAM_GRIP_PERF_OUT to an explicit JSON path");
    let flash_iterations = 1_000;
    let region_iterations = 100;
    let (flash_1k_count, flash_1k_ns) = measure(1_000, false, flash_iterations);
    let (flash_100k_count, flash_100k_ns) = measure(100_000, false, flash_iterations);
    let (region_1k_count, region_1k_ns) = measure(1_000, true, region_iterations);
    let (region_100k_count, region_100k_ns) = measure(100_000, true, region_iterations);
    let parallelism = std::thread::available_parallelism().map_or(0, |n| n.get());
    let report = format!(
        "{{\"schema_version\":2,\"environment\":{{\"os\":\"{}\",\"arch\":\"{}\",\"profile\":\"release\",\"available_parallelism\":{},\"fixture_generation_timed\":false}},\"flash\":{{\"iterations\":{},\"objects_1k\":{{\"feature_count\":{},\"total_ns\":{}}},\"objects_100k\":{{\"feature_count\":{},\"total_ns\":{}}}}},\"region_2k_edges\":{{\"iterations\":{},\"objects_1k\":{{\"feature_count\":{},\"total_ns\":{}}},\"objects_100k\":{{\"feature_count\":{},\"total_ns\":{}}}}}}}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        parallelism,
        flash_iterations,
        flash_1k_count,
        flash_1k_ns,
        flash_100k_count,
        flash_100k_ns,
        region_iterations,
        region_1k_count,
        region_1k_ns,
        region_100k_count,
        region_100k_ns,
    );
    std::fs::write(output, report).unwrap();
}
