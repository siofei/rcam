//! S4-B1 Gate I: 10 layers x 1000 objects, CPU/service path measured in release
//! on the native Mac (the GPU part has its own offscreen timing test).
//!
//! `cargo test --release -p editor-app --bin editor-app s4b1_release_performance \
//!  -- --ignored --nocapture` and set `RCAM_S4B1_PERF_OUT=<file.json>` to keep the
//! raw numbers. Structural assertions (candidate counts, no geometry copies, writer
//! bytes unchanged) hold in every build; timings are recorded, never asserted.
use crate::display::Scene;
use crate::selection::SelectionMode::Replace;
use crate::state::{Action, Model};
use editor_core::block::{
    BlockDefinition, BlockDefinitionId, BlockObject, BlockObjectGeometry, BlockTransform,
};
use editor_core::workspace::{Color, DisplayClass, LayerDisplayMode};
use editor_core::{
    ApertureDefinition, ApertureShape, Exposure, LocalTransform, Mirror, MmPoint, ObjectOrigin,
    SemanticGeometry, SemanticLayer, SemanticObject,
};
use editor_service::{ClassStyleUpdate, LayerInfo, LayerUpdateParams, RenderSnapshot};
use serde_json::{Value, json};
use std::time::Instant;

const LAYERS: usize = 10;
const OBJECTS: usize = 1000;

fn s4b2_block_snapshot() -> (RenderSnapshot, Vec<LayerInfo>) {
    let aperture = ApertureDefinition {
        id: "block-opening".into(),
        source_dcode: 10,
        shape: ApertureShape::Circle {
            diameter_mm: 0.1,
            hole_diameter_mm: None,
        },
    };
    let definition = BlockDefinition {
        id: BlockDefinitionId("perf-definition".into()),
        name: "400 openings".into(),
        local_origin: MmPoint::new(0., 0.),
        objects: (0..400)
            .map(|index| BlockObject {
                geometry: BlockObjectGeometry::Flash {
                    center: MmPoint::new((index % 20) as f64 * 0.2, (index / 20) as f64 * 0.2),
                    aperture_id: aperture.id.clone(),
                    transform: LocalTransform {
                        mirror: Mirror::None,
                        rotation_deg: 0.,
                        scale: 1.,
                    },
                },
                exposure: Exposure::Dark,
            })
            .collect(),
        revision: 1,
    };
    let layer = SemanticLayer {
        id: "block-layer".into(),
        objects: (0..100)
            .map(|index| SemanticObject {
                object_id: format!("instance-{index}"),
                geometry: SemanticGeometry::BlockInstance {
                    definition_id: definition.id.clone(),
                    transform: BlockTransform {
                        translation: MmPoint::new(
                            (index % 10) as f64 * 5.,
                            (index / 10) as f64 * 5.,
                        ),
                        rotation_deg: 0.,
                        mirror: false,
                    },
                },
                exposure: Exposure::Dark,
                origin: ObjectOrigin::Generated {
                    operation_id: "s4b2-performance".into(),
                },
            })
            .collect(),
    };
    let snapshot = RenderSnapshot {
        document_id: "s4b2-performance".into(),
        revision: "1".into(),
        workspace_revision: "1".into(),
        layers: vec![layer],
        apertures: vec![aperture],
        styles: vec![],
        block_definitions: vec![definition],
    };
    let layer_info = LayerInfo {
        layer_id: "block-layer".into(),
        display_name: "Block performance".into(),
        base_color: Color::rgb(0x2d, 0xc7, 0x9f),
        object_count: 100,
        ..Default::default()
    };
    (snapshot, vec![layer_info])
}

#[test]
#[ignore = "release/native S4-B2 400x100 display-cache and export-flatten evidence"]
fn s4b2_block_release_performance() {
    let (snapshot, layers) = s4b2_block_snapshot();
    let definition = &snapshot.block_definitions[0];
    let transform = match &snapshot.layers[0].objects[0].geometry {
        SemanticGeometry::BlockInstance { transform, .. } => transform,
        _ => unreachable!(),
    };
    let mut cache = crate::block_display::BlockDisplayCache::default();
    let started = Instant::now();
    let cached = cache.resolve(definition, transform).unwrap();
    let cache_build_ms = ms(started);
    assert_eq!(cached.len(), 400);
    assert_eq!(cache.stats(), (1, 400));

    let started = Instant::now();
    let scene = Scene::build_cached(
        &snapshot,
        &layers,
        MmPoint::new(0., 0.),
        100.,
        1,
        None,
        &mut cache,
    )
    .unwrap();
    let instance_display_prepare_ms = ms(started);
    assert_eq!(scene.objects.len(), 40_000);
    assert_eq!(snapshot.layers[0].objects.len(), 100);
    assert_eq!(snapshot.block_definitions[0].objects.len(), 400);
    assert_eq!(cache.stats(), (1, 400));

    const BASE: &[u8] = b"%FSLAX36Y36*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\nX0Y0D03*\nM02*\n";
    let mut document = gerber_io::parse_s1(BASE, "s4b2-performance")
        .unwrap()
        .document;
    document.layers = snapshot.layers.clone();
    document.apertures = snapshot.apertures.clone();
    document.block_definitions = snapshot.block_definitions.clone();
    let started = Instant::now();
    let flattened = gerber_io::flatten_block_instances_for_export(&document).unwrap();
    let export_flatten_ms = ms(started);
    assert_eq!(flattened.layers[0].objects.len(), 40_000);
    assert!(flattened.block_definitions.is_empty());
    assert_eq!(document.layers[0].objects.len(), 100);
    assert_eq!(document.block_definitions[0].objects.len(), 400);

    let display_memory_estimate_bytes = std::mem::size_of_val(scene.objects.as_slice())
        + std::mem::size_of_val(scene.primitives.as_slice())
        + std::mem::size_of_val(scene.points.as_slice());
    let report = json!({
        "schema": "rcam-s4b2-block-performance/1",
        "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "definition_openings": 400,
        "snapshot_sha256": editor_core::hash::sha256_hex(&serde_json::to_vec(&snapshot).unwrap()),
        "project_instances": 100,
        "project_top_level_objects": snapshot.layers[0].objects.len(),
        "cache_entries": cache.stats().0,
        "cache_resolved_objects": cache.stats().1,
        "display_objects": scene.objects.len(),
        "display_primitives": scene.primitives.len(),
        "display_memory_estimate_bytes": display_memory_estimate_bytes,
        "display_cache_build_ms": cache_build_ms,
        "instance_display_prepare_ms": instance_display_prepare_ms,
        "export_flatten_ms": export_flatten_ms,
        "working_project_remained_shared": true,
    });
    let text = serde_json::to_string_pretty(&report).unwrap();
    println!("S4B2_BLOCK_PERF_JSON_BEGIN\n{text}\nS4B2_BLOCK_PERF_JSON_END");
    if let Ok(out) = std::env::var("RCAM_S4B2_BLOCK_PERF_OUT") {
        std::fs::write(out, text).unwrap();
    }
}

/// Deterministic layer: 70 % regions, 10 % circle flashes, 10 % rectangle flashes,
/// 10 % strokes. Each layer is shifted a little so the layers overlap like a real stack.
fn layer_gerber(layer: usize) -> String {
    let mut s = String::from(
        "%FSLAX36Y36*%\n%MOMM*%\n%ADD10C,0.8*%\n%ADD11R,1.2X0.8*%\n%ADD12C,0.2*%\nG01*\n",
    );
    let q = |v: f64| (v * 1e6).round() as i64;
    for k in 0..OBJECTS {
        let x = (k % 40) as f64 * 2.5 + layer as f64 * 0.4;
        let y = (k / 40) as f64 * 2.5 + layer as f64 * 0.4;
        match k % 10 {
            0..=6 => s.push_str(&format!(
                "G36*\nX{}Y{}D02*\nX{}Y{}D01*\nX{}Y{}D01*\nX{}Y{}D01*\nG37*\n",
                q(x),
                q(y),
                q(x + 1.),
                q(y),
                q(x + 1.),
                q(y + 1.),
                q(x),
                q(y)
            )),
            7 => s.push_str(&format!("D10*\nX{}Y{}D03*\n", q(x), q(y))),
            8 => s.push_str(&format!("D11*\nX{}Y{}D03*\n", q(x), q(y))),
            _ => s.push_str(&format!(
                "D12*\nX{}Y{}D02*\nX{}Y{}D01*\n",
                q(x),
                q(y),
                q(x + 1.5),
                q(y)
            )),
        }
    }
    s.push_str("M02*\n");
    s
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// median / min / max in milliseconds over `reps` runs.
fn timed(reps: usize, mut f: impl FnMut()) -> Value {
    let mut v: Vec<f64> = (0..reps)
        .map(|_| {
            let t = Instant::now();
            f();
            ms(t)
        })
        .collect();
    v.sort_by(f64::total_cmp);
    json!({"median_ms": v[v.len() / 2], "min_ms": v[0], "max_ms": v[v.len() - 1], "runs": reps})
}

fn rss_kb() -> u64 {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

fn patch(m: &Model, id: &str) -> LayerUpdateParams {
    LayerUpdateParams {
        layer_id: id.into(),
        expected_workspace_revision: m.view.info.as_ref().unwrap().workspace_revision.clone(),
        ..Default::default()
    }
}

fn run(m: &mut Model, action: Action) {
    m.run(action);
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
}

/// Everything that depends on what is visible/styled right now.
fn measure(m: &mut Model, label: &str, dir: &std::path::Path) -> (Value, Vec<u8>) {
    let doc = m.view.info.as_ref().unwrap().document_id.clone();
    let layer0 = m.view.layers[0].layer_id.clone();
    let snapshot_stats = timed(5, || {
        m.service.render_snapshot(&doc).unwrap();
    });
    let snapshot = m.service.render_snapshot(&doc).unwrap();
    let layers = m.view.layers.clone();
    let scene_stats = timed(5, || {
        Scene::build(&snapshot, &layers, MmPoint::new(0., 0.), 1000., 1).unwrap();
    });
    let scene = Scene::build(&snapshot, &layers, MmPoint::new(0., 0.), 1000., 1).unwrap();
    let index_stats = timed(5, || {
        crate::render_index::RenderIndex::build(&scene.objects, &[], [0.; 2]).unwrap();
    });
    let index = crate::render_index::RenderIndex::build(&scene.objects, &[], [0.; 2]).unwrap();
    let visible = scene.objects.iter().filter(|o| o.meta[3] != 0).count();
    let fit_stats = timed(20, || {
        m.service.visible_bounds(&doc).unwrap();
    });
    // Points spread over the sheet; every call walks all layers top to bottom.
    let mut n = 0u32;
    let hit_stats = timed(50, || {
        n += 1;
        let p = MmPoint::new(f64::from(n % 40) * 2.5 + 0.3, f64::from(n % 25) * 2.5 + 0.3);
        m.run(Action::Select(p, 0.05, Replace));
    });
    let path = dir.join(format!("{label}.gbr"));
    let _ = std::fs::remove_file(&path);
    let export_stats = timed(1, || {
        m.run(Action::Save(path.clone(), layer0.clone(), None));
    });
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let bytes = std::fs::read(&path).unwrap();
    let scene_bytes = std::mem::size_of_val(scene.objects.as_slice())
        + std::mem::size_of_val(scene.primitives.as_slice())
        + std::mem::size_of_val(scene.points.as_slice())
        + std::mem::size_of_val(index.data.as_slice());
    (
        json!({
            "scenario": label,
            "layers_visible": m.view.layers.iter().filter(|l| l.effective_visible).count(),
            "manufacturing_objects": snapshot.layers.iter().map(|l| l.objects.len()).sum::<usize>(),
            "display_objects_total": scene.objects.len(),
            "display_objects_visible": visible,
            "render_index_object_refs": index.data.len(),
            "render_index_max_candidates": index.max_candidates,
            "display_primitives": scene.primitives.len(),
            "display_points": scene.points.len(),
            "display_memory_estimate_bytes": scene_bytes,
            "snapshot": snapshot_stats,
            "display_scene_and_index_build": scene_stats,
            "render_index_build": index_stats,
            "fit_visible": fit_stats,
            "hit_test_select": hit_stats,
            "export_one_layer": export_stats,
            "export_bytes_len": bytes.len(),
            "process_rss_kb": rss_kb(),
        }),
        bytes,
    )
}

#[test]
#[ignore = "release/native performance capture: run with --release --ignored --nocapture"]
fn s4b1_release_performance() {
    let dir = std::env::temp_dir().join(format!("rcam-s4b1-perf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let files: Vec<_> = (0..LAYERS)
        .map(|i| {
            let p = dir.join(format!("L{i:02}.gbr"));
            std::fs::write(&p, layer_gerber(i)).unwrap();
            p
        })
        .collect();
    let mut m = Model::default();
    run(&mut m, Action::NewWorkspace);
    let rss_before = rss_kb();
    let t = Instant::now();
    run(&mut m, Action::ImportGerbers(files));
    let import_ms = ms(t);
    let rss_after_import = rss_kb();
    assert_eq!(m.view.layers.len(), LAYERS);
    let ids: Vec<String> = m.view.layers.iter().map(|l| l.layer_id.clone()).collect();
    let manufacturing_revision = m.view.info.as_ref().unwrap().revision.clone();

    // A: 10 visible, Filled.
    let (a, bytes_a) = measure(&mut m, "A_10_visible_filled", &dir);

    // B: one visible layer, nine hidden (one workspace revision per toggle here; the
    // GUI's "hide all" is one revision, measured separately below).
    let toggle = timed(1, || {
        run(&mut m, Action::SetAllLayersVisible(false));
    });
    let show_one = timed(1, || {
        let mut p = patch(&m, &ids[0]);
        p.visible = Some(true);
        run(&mut m, Action::Layer(p));
    });
    let (b, bytes_b) = measure(&mut m, "B_1_visible_9_hidden", &dir);
    run(&mut m, Action::SetAllLayersVisible(true));

    // C: hide the high-share RegionFreeform class on every layer.
    let hide_class = timed(1, || {
        for id in &ids {
            let mut p = patch(&m, id);
            p.classes = vec![ClassStyleUpdate {
                class: Some(DisplayClass::RegionFreeform),
                visible: Some(false),
                ..Default::default()
            }];
            run(&mut m, Action::Layer(p));
        }
    });
    let (c, bytes_c) = measure(&mut m, "C_regions_class_hidden", &dir);
    for id in &ids {
        let mut p = patch(&m, id);
        p.classes = vec![ClassStyleUpdate {
            class: Some(DisplayClass::RegionFreeform),
            visible: Some(true),
            ..Default::default()
        }];
        run(&mut m, Action::Layer(p));
    }

    // D: ZeroWidth on every layer (display-only; no manufacturing copy).
    for id in &ids {
        let mut p = patch(&m, id);
        p.display_mode = Some(LayerDisplayMode::ZeroWidth);
        run(&mut m, Action::Layer(p));
    }
    let (d, bytes_d) = measure(&mut m, "D_10_visible_zerowidth", &dir);

    // Structural guarantees.
    let num = |v: &Value, k: &str| v[k].as_u64().unwrap();
    assert_eq!(
        num(&a, "display_objects_visible"),
        (LAYERS * OBJECTS) as u64
    );
    assert_eq!(num(&b, "display_objects_visible"), OBJECTS as u64);
    assert!(
        num(&b, "render_index_object_refs") * 5 < num(&a, "render_index_object_refs"),
        "B: index work must really drop"
    );
    assert!(
        num(&c, "display_objects_visible") * 2 < num(&a, "display_objects_visible"),
        "C: hidden class must remove render/hit candidates"
    );
    assert!(num(&c, "render_index_object_refs") < num(&a, "render_index_object_refs"));
    assert!(
        num(&d, "display_primitives") <= num(&a, "display_primitives")
            && num(&d, "display_points") <= num(&a, "display_points"),
        "D: ZeroWidth must not create extra geometry copies"
    );
    assert!(
        num(&d, "display_memory_estimate_bytes")
            <= num(&a, "display_memory_estimate_bytes") * 12 / 10,
        "D: no abnormal memory growth"
    );
    assert_eq!(bytes_a, bytes_b, "view state never changes writer bytes");
    assert_eq!(bytes_a, bytes_c);
    assert_eq!(bytes_a, bytes_d);
    assert_eq!(
        m.view.info.as_ref().unwrap().revision,
        manufacturing_revision,
        "no manufacturing revision moved during the whole run"
    );

    let report = json!({
        "schema": "rcam-s4b1-release-performance/1",
        "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "layers": LAYERS,
        "objects_per_layer": OBJECTS,
        "object_mix": "70% regions, 10% circle flashes, 10% rectangle flashes, 10% strokes",
        "batch_import_ms": import_ms,
        "rss_kb_before_import": rss_before,
        "rss_kb_after_import": rss_after_import,
        "toggle_visibility": {
            "hide_all_layers": toggle,
            "show_one_layer": show_one,
            "hide_class_on_all_layers": hide_class,
        },
        "scenarios": [a, b, c, d],
        "writer_bytes_identical_across_scenarios": true,
    });
    let text = serde_json::to_string_pretty(&report).unwrap();
    println!("S4B1_PERF_JSON_BEGIN\n{text}\nS4B1_PERF_JSON_END");
    if let Ok(out) = std::env::var("RCAM_S4B1_PERF_OUT") {
        std::fs::write(out, text).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
