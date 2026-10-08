//! S2-A.2 exact hit-test through the real application/JSON/file boundary.
use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Run {
    service: ApplicationService,
    dir: PathBuf,
    id: String,
    layer: String,
    object: String,
    calls: Vec<Value>,
}
impl Run {
    fn new() -> Self {
        let base = std::env::var_os("RCAM_S2A2_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!(
            "hit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("source.gbr"),
            "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\nD10*\nX2000000Y3000000D03*\nM02*\n",
        )
        .unwrap();
        let service = ApplicationService::with_file_access(FileAccessPolicy::new(
            dir.clone(),
            [dir.clone()],
            [dir.clone()],
        ));
        let mut r = Self {
            service,
            dir,
            id: String::new(),
            layer: String::new(),
            object: String::new(),
            calls: vec![],
        };
        let info = r.ok("document.open", None, json!({"path":"source.gbr"}));
        r.id = info["document_id"].as_str().unwrap().into();
        r.layer = info["layer_ids"][0].as_str().unwrap().into();
        r.object = r.objects()[0]["object"]["object_id"]
            .as_str()
            .unwrap()
            .into();
        r
    }
    fn call(&mut self, op: &str, rev: Option<&str>, params: Value) -> Value {
        let mut request = json!({"api_version":1,"request_id":format!("hit-{}",self.calls.len()),"op":op,"params":params});
        if op != "document.open" && op != "system.capabilities" {
            request["document_id"] = json!(self.id);
        }
        if let Some(rev) = rev {
            request["expected_revision"] = json!(rev);
        }
        let response = self.service.execute_json(&request.to_string());
        assert_eq!(response["request_id"], request["request_id"]);
        self.calls
            .push(json!({"request":request,"response":response}));
        response
    }
    fn ok(&mut self, op: &str, rev: Option<&str>, params: Value) -> Value {
        let response = self.call(op, rev, params);
        assert_eq!(response["status"], "completed", "{response}");
        response["result"].clone()
    }
    fn info(&mut self) -> Value {
        self.ok("document.get", None, json!({}))
    }
    fn objects(&mut self) -> Value {
        self.ok("objects.query", None, json!({"layer_id":self.layer}))["objects"].clone()
    }
    fn update(&mut self, mut patch: Value) -> Value {
        let info = self.info();
        patch["layer_id"] = json!(self.layer);
        patch["expected_workspace_revision"] = info["workspace_revision"].clone();
        self.ok("layer.update", info["revision"].as_str(), patch)
    }
    fn hit(&mut self, x: f64, y: f64, tolerance: f64) -> Value {
        self.ok(
            "objects.hit_test",
            None,
            json!({"layer_id":self.layer,"point":{"x_mm":x,"y_mm":y},"tolerance_mm":tolerance}),
        )["object_ids"]
            .clone()
    }
    fn open_fixture(&mut self, path: &str) {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
        std::fs::copy(fixtures.join(path), self.dir.join("fixture.gbr")).unwrap();
        let info = self.ok("document.open", None, json!({"path":"fixture.gbr"}));
        self.id = info["document_id"].as_str().unwrap().into();
        self.layer = info["layer_ids"][0].as_str().unwrap().into();
        self.object = self.objects()[0]["object"]["object_id"]
            .as_str()
            .unwrap()
            .into();
    }
    fn move_one(&mut self) {
        let rev = self.info()["revision"].as_str().unwrap().to_string();
        self.ok(
            "objects.move",
            Some(&rev),
            json!({"layer_id":self.layer,"object_ids":[self.object],"dx_mm":5,"dy_mm":-1}),
        );
    }
    fn export(&mut self, path: &str) -> Vec<u8> {
        let rev = self.info()["revision"].as_str().unwrap().to_string();
        self.ok("gerber.export_layer", Some(&rev), json!({"layer_id":self.layer,"path":path,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}}));
        std::fs::read(self.dir.join(path)).unwrap()
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        if std::env::var_os("RCAM_S2A2_EVIDENCE").is_some() {
            std::fs::write(
                self.dir.join("requests.json"),
                serde_json::to_vec_pretty(&self.calls).unwrap(),
            )
            .unwrap();
        } else {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

#[test]
fn hit_test_after_move_undo_redo_export_reopen() {
    let mut r = Run::new();
    let original = r.object.clone();
    let source = std::fs::read(r.dir.join("source.gbr")).unwrap();
    assert_eq!(r.hit(2., 3., 0.), json!([original]));
    r.move_one();
    assert_eq!(r.hit(2., 3., 0.), json!([]));
    assert_eq!(r.hit(7., 2., 0.), json!([original]));
    r.ok("history.undo", Some("1"), json!({}));
    assert_eq!(r.hit(2., 3., 0.), json!([original]));
    assert_eq!(r.hit(7., 2., 0.), json!([]));
    r.update(json!({"locked":true}));
    r.ok("history.redo", Some("2"), json!({}));
    assert_eq!(r.hit(7., 2., 0.), json!([original]));
    let result = r.ok(
        "objects.hit_test",
        None,
        json!({"layer_id":r.layer,"point":{"x_mm":7,"y_mm":2},"tolerance_mm":0}),
    );
    assert_eq!(result["revision"], "3");
    r.export("saved.gbr");
    let info = r.ok("document.open", None, json!({"path":"saved.gbr"}));
    r.id = info["document_id"].as_str().unwrap().into();
    r.layer = info["layer_ids"][0].as_str().unwrap().into();
    assert_eq!(r.hit(7., 2., 0.).as_array().unwrap().len(), 1);
    assert_eq!(r.hit(2., 3., 0.), json!([]));
    assert_eq!(std::fs::read(r.dir.join("source.gbr")).unwrap(), source);
}
#[test]
fn hit_test_after_duplicate_delete_and_restored_id_order() {
    let mut r = Run::new();
    let original = r.object.clone();
    r.ok(
        "objects.duplicate",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":[original],"dx_mm":2,"dy_mm":0}),
    );
    let objects = r.objects();
    let duplicate = objects[1]["object"]["object_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(original, duplicate);
    assert_eq!(r.hit(3., 3., 0.), json!([original, duplicate]));
    assert_eq!(r.hit(4., 3., 0.), json!([duplicate]));
    r.ok(
        "objects.delete",
        Some("1"),
        json!({"layer_id":r.layer,"object_ids":[original]}),
    );
    assert_eq!(r.hit(3., 3., 0.), json!([duplicate]));
    r.ok("history.undo", Some("2"), json!({}));
    assert_eq!(r.hit(3., 3., 0.), json!([original, duplicate]));
}
#[test]
fn hit_test_after_rotate_mirror_tracks_real_geometry() {
    let mut r = Run::new();
    let id = r.object.clone();
    r.ok(
        "objects.rotate",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":[id],"angle_deg":90,"pivot_mm":{"x_mm":0,"y_mm":0}}),
    );
    assert_eq!(r.hit(2., 3., 0.), json!([]));
    assert_eq!(r.hit(-3., 2., 0.), json!([id]));
    r.ok(
        "objects.mirror",
        Some("1"),
        json!({"layer_id":r.layer,"object_ids":[id],"axis":{"kind":"vertical","coordinate_mm":0}}),
    );
    assert_eq!(r.hit(-3., 2., 0.), json!([]));
    assert_eq!(r.hit(3., 2., 0.), json!([id]));
}
#[test]
fn hit_test_workspace_and_queries_do_not_change_history_or_revisions() {
    let mut r = Run::new();
    let before = r.info();
    let hit = r.hit(2., 3., 0.);
    assert_eq!(r.info(), before);
    r.move_one();
    r.ok("history.undo", Some("1"), json!({}));
    r.update(json!({"visible":false,"locked":true,"display_name":"隐藏 #层"}));
    let before = r.info();
    assert_eq!(r.hit(2., 3., 0.), hit);
    assert_eq!(r.info(), before);
    assert_eq!(before["redo_entries"], 1);
    let response = r.call(
        "objects.move",
        Some("2"),
        json!({"layer_id":r.layer,"object_ids":[r.object],"dx_mm":1,"dy_mm":0}),
    );
    assert_eq!(response["error"]["code"], "LAYER_LOCKED");
    assert_eq!(r.info(), before);
}
#[test]
fn hit_test_invalid_params_are_atomic_and_nested_fields_strict() {
    let mut r = Run::new();
    let before = r.info();
    for params in [
        json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0},"tolerance_mm":-1}),
        json!({"layer_id":r.layer,"point":{"x_mm":1e9+1.,"y_mm":0},"tolerance_mm":0}),
        json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0},"tolerance_mm":1e9+1.}),
        json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0,"extra":1},"tolerance_mm":0}),
        json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0},"tolerance_mm":0,"extra":1}),
        json!({"layer_id":"","point":{"x_mm":0,"y_mm":0},"tolerance_mm":0}),
        json!({"layer_id":r.layer,"point_mm":{"x_mm":0,"y_mm":0},"tolerance_mm":0}),
        json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0}}),
    ] {
        let response = r.call("objects.hit_test", None, params);
        assert_eq!(response["error"]["code"], "INVALID_ARGUMENT", "{response}");
        assert_eq!(r.info(), before);
    }
    let params = json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0},"tolerance_mm":0});
    assert_eq!(
        r.call("objects.hit_test", Some("0"), params)["error"]["code"],
        "INVALID_ARGUMENT"
    );
    let missing = r.call(
        "objects.hit_test",
        None,
        json!({"layer_id":"absent","point":{"x_mm":0,"y_mm":0},"tolerance_mm":0}),
    );
    assert_eq!(missing["error"]["code"], "NOT_FOUND");
    assert_eq!(missing["error"]["details"]["entity"], "layer");
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let params = editor_service::HitTestParams {
            layer_id: r.layer.clone(),
            point: editor_service::HitTestPoint {
                x_mm: value,
                y_mm: 0.,
            },
            tolerance_mm: 0.,
            selectable_only: false,
        };
        assert_eq!(
            r.service.objects_hit_test(&r.id, params).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
    }
    for invalid in ["NaN", "Infinity", "1e999"] {
        let raw = format!(
            r#"{{"api_version":1,"request_id":"nonfinite","document_id":"{}","op":"objects.hit_test","params":{{"layer_id":"{}","point":{{"x_mm":{invalid},"y_mm":0}},"tolerance_mm":0}}}}"#,
            r.id, r.layer
        );
        let response = r.service.execute_json(&raw);
        assert_eq!(response["error"]["code"], "INVALID_ARGUMENT");
        r.calls.push(json!({"raw_request":raw,"response":response}));
    }
    assert_eq!(r.info(), before);
}
#[test]
fn hit_test_capability_empty_layer_and_closed_document() {
    let mut r = Run::new();
    let caps = r.ok("system.capabilities", None, json!({}));
    assert!(
        caps["supported_operations"]
            .as_array()
            .unwrap()
            .contains(&json!("objects.hit_test"))
    );
    assert_eq!(caps["resource_limits"]["max_hit_test_work"], 2_000_000);
    assert!(
        caps["resource_limits"]
            .as_object()
            .unwrap()
            .contains_key("max_select_rect_work")
    );
    assert!(caps["resource_limits"]["max_select_rect_work"].is_null());
    r.ok(
        "objects.delete",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":[r.object]}),
    );
    assert_eq!(r.hit(2., 3., 100.), json!([]));
    r.ok("document.close", Some("1"), json!({"discard_changes":true}));
    let response = r.call(
        "objects.hit_test",
        None,
        json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0},"tolerance_mm":0}),
    );
    assert_eq!(response["error"]["code"], "NOT_FOUND");
    assert_eq!(response["error"]["details"]["entity"], "document");
}
#[test]
fn hit_test_real_parser_region_cutin_arc_deviation_and_rectangular_sweep() {
    let mut r = Run::new();
    for (fixture, hits, misses) in [
        (
            "s1a/region_cutin.gbr",
            vec![(0.5, 0.5), (3.5, 3.5)],
            vec![(2., 2.), (5., 5.)],
        ),
        (
            "s1a1/g75_large_deviation.gbr",
            vec![(1.25, 0.), (0., 1.75)],
            vec![(1. / 2_f64.sqrt(), 1. / 2_f64.sqrt()), (-1.5, 0.)],
        ),
        (
            "s0c/rectangular_draw.gbr",
            vec![(-0.9, -0.4), (3.9, 2.4)],
            vec![(-0.9, 2.4), (5., 0.)],
        ),
        (
            "s0c/am_1.gbr",
            vec![(0., 0.), (0.9, 0.)],
            vec![(0.9, 0.9), (2., 0.)],
        ),
    ] {
        r.open_fixture(fixture);
        let before = r.info();
        for (x, y) in hits {
            assert!(
                !r.hit(x, y, 0.).as_array().unwrap().is_empty(),
                "{fixture} ({x},{y})"
            );
        }
        for (x, y) in misses {
            assert_eq!(r.hit(x, y, 0.), json!([]), "{fixture} ({x},{y})");
        }
        assert_eq!(r.info(), before);
    }
}
#[test]
fn hit_test_macro_clear_and_global_clear_are_distinct_parser_semantics() {
    let mut r = Run::new();
    std::fs::write(r.dir.join("macro.gbr"),"%FSLAX26Y26*%\n%MOMM*%\n%AMRING*1,1,10,0,0*1,0,6,0,0*%\n%ADD10RING*%\n%ADD11C,10*%\nD10*\nX0Y0D03*\n%LPC*%\nD11*\nX0Y0D03*\nM02*\n").unwrap();
    let info = r.ok("document.open", None, json!({"path":"macro.gbr"}));
    r.id = info["document_id"].as_str().unwrap().into();
    r.layer = info["layer_ids"][0].as_str().unwrap().into();
    let objects = r.objects();
    let ring = objects[0]["object"]["object_id"].clone();
    let clear = objects[1]["object"]["object_id"].clone();
    assert_eq!(r.hit(0., 0., 0.1), json!([clear]));
    assert_eq!(r.hit(4., 0., 0.), json!([ring, clear]));
    assert_eq!(r.hit(2.8, 0., 0.21), json!([ring, clear]));
}
#[test]
fn hit_test_rectangular_rotation_swap_and_arc_mirror_direction() {
    let mut r = Run::new();
    r.open_fixture("s0c/rectangular_draw.gbr");
    let ids: Vec<_> = r
        .objects()
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"]["object_id"].clone())
        .collect();
    r.ok(
        "objects.rotate",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":ids,"angle_deg":90,"pivot_mm":{"x_mm":0,"y_mm":0}}),
    );
    assert!(!r.hit(0.4, 3.9, 0.).as_array().unwrap().is_empty());
    assert_eq!(r.hit(3.9, 0.4, 0.), json!([]));
    r.open_fixture("s1a1/g75_exact.gbr");
    let id = r.object.clone();
    r.ok(
        "objects.mirror",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":[id],"axis":{"kind":"vertical","coordinate_mm":0}}),
    );
    assert_eq!(r.hit(-0.7, 0.7, 0.), json!([id]));
    assert_eq!(r.hit(0.7, 0.7, 0.), json!([]));
}
#[test]
fn hit_test_result_json_preserves_large_revision_and_stable_ids() {
    let r = editor_service::HitTestResult {
        document_id: "文档 #1".into(),
        revision: "9007199254740993".into(),
        layer_id: "layer".into(),
        object_ids: vec!["z".into(), "a".into()],
    };
    assert_eq!(
        serde_json::from_str::<editor_service::HitTestResult>(&serde_json::to_string(&r).unwrap())
            .unwrap(),
        r
    );
}

#[test]
fn hit_test_resource_limit_returns_no_partial_ids_and_no_side_effects() {
    let mut r = Run::new();
    let mut source = String::from("%FSLAX26Y26*%\n%MOMM*%\n%AMLARGE*");
    for _ in 0..1500 {
        source.push_str("1,1,2,0,0*");
    }
    source.push_str("%\n%ADD10C,2*%\n%ADD11LARGE*%\nD10*\nX0Y0D03*\nD11*\nX0Y0D03*\nM02*\n");
    std::fs::write(r.dir.join("budget.gbr"), source).unwrap();
    let info = r.ok("document.open", None, json!({"path":"budget.gbr"}));
    r.id = info["document_id"].as_str().unwrap().into();
    r.layer = info["layer_ids"][0].as_str().unwrap().into();
    let before = r.info();
    let objects = r.objects();
    let response = r.call(
        "objects.hit_test",
        None,
        json!({"layer_id":r.layer,"point":{"x_mm":0,"y_mm":0},"tolerance_mm":0}),
    );
    assert_eq!(response["error"]["code"], "RESOURCE_LIMIT");
    assert!(response["result"].is_null());
    assert_eq!(r.info(), before);
    assert_eq!(r.objects(), objects);
}

#[test]
fn internal_scored_query_preserves_legacy_policy_errors_and_wire_schema() {
    use editor_service::{HitTestParams, HitTestPoint};
    let mut r = Run::new();
    let query = |layer: &str, x, y, tolerance| HitTestParams {
        layer_id: layer.into(),
        point: HitTestPoint { x_mm: x, y_mm: y },
        tolerance_mm: tolerance,
        selectable_only: true,
    };
    for (x, y, tol) in [(2., 3., 0.), (3.1, 3., 0.2), (9., 9., 0.1)] {
        let params = query(&r.layer, x, y, tol);
        let old = r.service.objects_hit_test(&r.id, params.clone()).unwrap();
        let new = r
            .service
            .objects_hit_test_scored_with_cancel(&r.id, params, None)
            .unwrap();
        assert_eq!(
            old.object_ids,
            new.hits
                .iter()
                .map(|h| h.object_id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            (&old.document_id, &old.revision, &old.layer_id),
            (&new.document_id, &new.revision, &new.layer_id)
        );
        let wire = serde_json::to_value(&old).unwrap();
        assert!(wire.get("hits").is_none());
        assert!(wire.get("distance_mm").is_none());
    }
    for (x, y, tol) in [
        (f64::NAN, 3., 0.),
        (2., f64::INFINITY, 0.),
        (2., 3., -1.),
        (2., 3., f64::INFINITY),
    ] {
        let params = query(&r.layer, x, y, tol);
        assert_eq!(
            r.service
                .objects_hit_test(&r.id, params.clone())
                .unwrap_err(),
            r.service
                .objects_hit_test_scored_with_cancel(&r.id, params, None)
                .unwrap_err()
        );
    }
    for layer in ["", "missing"] {
        let params = query(layer, 2., 3., 0.);
        assert_eq!(
            r.service
                .objects_hit_test(&r.id, params.clone())
                .unwrap_err(),
            r.service
                .objects_hit_test_scored_with_cancel(&r.id, params, None)
                .unwrap_err()
        );
    }
    r.update(json!({"locked":true}));
    assert_eq!(
        r.service
            .objects_hit_test_scored_with_cancel(&r.id, query(&r.layer, 2., 3., 0.), None)
            .unwrap()
            .hits
            .len(),
        1,
        "locked geometry remains inspectable in both queries"
    );
    r.update(json!({"visible":false}));
    assert!(
        r.service
            .objects_hit_test_scored_with_cancel(&r.id, query(&r.layer, 2., 3., 0.), None)
            .unwrap()
            .hits
            .is_empty()
    );
    let cancel = editor_service::task::CancellationToken::default();
    cancel.cancel();
    assert_eq!(
        r.service
            .objects_hit_test_scored_with_cancel(&r.id, query(&r.layer, 2., 3., 0.), Some(&cancel))
            .unwrap_err()
            .code,
        "CANCELLED"
    );
}
