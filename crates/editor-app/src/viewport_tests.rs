use crate::{
    camera::Camera,
    display::{Object, Primitive, Scene},
    gpu,
    render_index::RenderIndex,
};
use editor_core::MmPoint;
use eframe::egui::{Pos2, Rect, vec2};
use std::sync::Arc;
fn scene(objects: Vec<Object>) -> Scene {
    let index = Arc::new(RenderIndex::build(&objects, &[], [0.; 2]).unwrap());
    Scene {
        serial: 1,
        anchor: MmPoint::new(0., 0.),
        ids: (0..objects.len()).map(|i| i.to_string()).collect(),
        objects,
        primitives: vec![Primitive {
            meta: [0, 1, 0, 0],
            a: [0.; 4],
            b: [0.25, 0., 0., 0.],
        }],
        points: vec![],
        ppm: 100.,
        index,
    }
}
fn o(b: [f32; 4]) -> Object {
    Object {
        meta: [0, 1, 1, 1],
        bounds: b,
        ..Default::default()
    }
}
fn rect() -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(1600., 900.))
}
#[test]
fn viewport_physical_margin_large_object_order_and_empty() {
    let s = scene(vec![
        o([-100., 0., 100., 1.]),
        o([0., 0., 1., 1.]),
        Object {
            meta: [0, 1, 0, 2],
            bounds: [0., 0., 1., 1.],
            ..Default::default()
        },
    ]);
    let camera = Camera {
        center: MmPoint::new(0., 0.),
        scale: 100.,
    };
    let bounds = gpu::viewport_bounds(&s, camera, rect(), 2.).unwrap();
    assert_eq!(bounds, [-8.01, -4.51, 8.01, 4.51]);
    let view = s.index.viewport(bounds);
    assert_eq!(view.ordered_candidate_ids, vec![0, 1, 2]);
    assert!(view.cell_range.is_some());
    assert!(
        s.index
            .viewport([200., 200., 300., 300.])
            .ordered_candidate_ids
            .is_empty()
    );
    assert_eq!(s.objects[2].meta[2], 0);
}
#[test]
fn viewport_dense_offscreen_budget_and_hundred_thousand_visit_counter() {
    let mut objects = vec![o([0., 0., 1., 1.]); 100];
    // 99900 distributed objects, separated from the sparse viewport, fit the existing bounded index.
    objects.extend((0..99900).map(|i| {
        let x = 1000. + (i % 1000) as f32;
        let y = (i / 1000) as f32;
        o([x, y, x + 0.5, y + 0.5])
    }));
    let s = scene(objects);
    let flags = vec![0; s.objects.len()];
    let camera = Camera {
        center: MmPoint::new(0., 0.),
        scale: 10.,
    };
    let prepared = gpu::prepare_measured(
        &s,
        camera,
        Rect::from_min_size(Pos2::ZERO, vec2(128., 128.)),
        1.,
        &flags,
        MmPoint::new(0., 0.),
    )
    .unwrap();
    assert_eq!(prepared.stats.object_visits, 100);
    assert_eq!(prepared.stats.candidate_count, 100);
    assert!(Arc::ptr_eq(&s.index, &prepared.index));
    println!(
        "VIEWPORT_COUNTER {}",
        serde_json::json!({"scene_total":100000,"candidate_count":prepared.stats.candidate_count,"object_visits":prepared.stats.object_visits,"cell_references_visited":prepared.stats.cell_references_visited})
    );
    let mut objects = vec![o([0., 0., 1., 1.])];
    objects.extend(vec![o([1000., 1000., 1001., 1001.]); 1000]);
    let s = scene(objects);
    let flags = vec![0; s.objects.len()];
    let sparse =
        gpu::prepare_measured(&s, camera, rect(), 1., &flags, MmPoint::new(0., 0.)).unwrap();
    assert_eq!(sparse.stats.max_candidates_in_view, 1);
    assert_eq!(sparse.stats.object_visits, 1);
    let dense = Camera {
        center: MmPoint::new(1000., 1000.),
        ..camera
    };
    assert!(
        gpu::prepare_measured(&s, dense, rect(), 1., &flags, MmPoint::new(0., 0.))
            .err()
            .unwrap()
            .contains("candidate_sample_work")
    );
    assert!(Arc::ptr_eq(&s.index, &sparse.index));
    println!(
        "VIEWPORT_DENSE {}",
        serde_json::json!({"global_max":s.index.max_candidates,"sparse_max":sparse.stats.max_candidates_in_view,"sparse_work":sparse.stats.estimated_work,"dense_status":"RESOURCE_LIMIT"})
    );
}
#[test]
#[ignore = "release CPU benchmark; retain raw data"]
fn preview_index_benchmark() {
    assert!(!cfg!(debug_assertions));
    use crate::state::Model;
    for fixture in [
        "s2b3_1/P1K_CIRCLES.gbr",
        "s2b3_2/P10K_CIRCLES.gbr",
        "s2b3_2/P100K_CIRCLES.gbr",
    ] {
        let mut model = Model::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic")
            .join(fixture);
        if let Err(error) = model.open(&path) {
            println!(
                "PREVIEW_BENCH {}",
                serde_json::json!({"fixture":fixture,"status":"BLOCKED","error":error})
            );
            continue;
        }
        let s = model.view.scene.as_ref().unwrap();
        let flags = vec![1; s.objects.len()];
        // Small fixed viewport isolates preview index costs from full-view material work.
        let camera = Camera {
            center: MmPoint::new(5., 5.),
            scale: 100.,
        };
        let mut samples = vec![];
        for i in 0..60 {
            let delta = MmPoint::new(0.1 + f64::from(i) / 100., 0.2);
            let independent = std::time::Instant::now();
            let _index =
                RenderIndex::build(&s.objects, &flags, [delta.x_mm as f32, delta.y_mm as f32])
                    .unwrap();
            let index_ms = independent.elapsed().as_secs_f64() * 1000.;
            let start = std::time::Instant::now();
            match gpu::prepare_measured(s,camera,rect(),1.,&flags,delta) {
                Ok(p)=>samples.push(serde_json::json!({"preview_index_ms":p.stats.preview_index_ms,"cpu_prepare_ms":p.stats.cpu_prepare_ms,"object_visits":p.stats.object_visits,"prepare_status":"PASS"})),
                Err(e)=>samples.push(serde_json::json!({"preview_index_ms":index_ms,"cpu_prepare_ms":start.elapsed().as_secs_f64()*1000.,"prepare_status":"BLOCKED","error":e,"index_timing":"independent same-input build; prepare failed after its own index build"})),
            }
        }
        if !samples.is_empty() {
            println!(
                "PREVIEW_BENCH {}",
                serde_json::json!({"fixture":fixture,"status":"MEASURED","total_objects":s.objects.len(),"samples":samples})
            );
        }
    }
}

#[test]
fn f64_viewport_culls_remote_objects_before_rebasing_gpu_coordinates() {
    use crate::{state::Model, world_index::WorldIndex};
    use editor_core::BoundsMm;
    let mut model = Model::default();
    model
        .open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/s1a/standard_hole_over_line.gbr"),
        )
        .unwrap();
    let mut snapshot = model
        .service
        .render_snapshot(&model.view.info.as_ref().unwrap().document_id)
        .unwrap();
    let mut local = snapshot.layers[0].objects[0].clone();
    local.geometry = editor_core::SemanticGeometry::Line {
        start: MmPoint::new(1e8, 0.),
        end: MmPoint::new(1e8 + 0.001, 0.),
        width_mm: 0.0001,
    };
    local.object_id = "local".into();
    let mut remote = local.clone();
    remote.object_id = "remote".into();
    remote.geometry = editor_core::SemanticGeometry::Line {
        start: MmPoint::new(-1e12, 0.),
        end: MmPoint::new(-1e12 + 1., 0.),
        width_mm: 0.1,
    };
    snapshot.layers[0].objects = vec![remote, local];
    let before = snapshot.clone();
    let index = WorldIndex::build(&snapshot).unwrap();
    let view = BoundsMm {
        min_x_mm: 1e8 - 0.01,
        min_y_mm: -0.01,
        max_x_mm: 1e8 + 0.01,
        max_y_mm: 0.01,
    };
    let culled = index.query(&snapshot, &model.view.layers, view);
    assert_eq!(culled.layers[0].objects.len(), 1);
    assert_eq!(culled.layers[0].objects[0].object_id, "local");
    let origin = MmPoint::new(1e8, 0.);
    let scene = Scene::build(&culled, &model.view.layers, origin, 100_000., 1).unwrap();
    assert_eq!(scene.anchor, origin);
    assert!(Scene::build(&snapshot, &model.view.layers, origin, 100_000., 1).is_err());
    assert!(scene.scalar(0.001).is_ok());
    assert_eq!(snapshot, before);
}

#[test]
fn display_precision_failure_preserves_last_good_scene_and_manufacturing() {
    use crate::state::{Action, Model};
    let mut model = Model::default();
    model
        .open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/s1a/standard_hole_over_line.gbr"),
        )
        .unwrap();
    let scene = model.view.scene.clone().unwrap();
    let info = model.view.info.clone().unwrap();
    let snapshot = model.service.render_snapshot(&info.document_id).unwrap();
    model.run(Action::Rebuild(1e20));
    assert!(model.view.display_transient.is_some());
    assert!(Arc::ptr_eq(&scene, model.view.scene.as_ref().unwrap()));
    assert_eq!(model.view.info.as_ref().unwrap(), &info);
    assert_eq!(
        model.service.render_snapshot(&info.document_id).unwrap(),
        snapshot
    );
}
