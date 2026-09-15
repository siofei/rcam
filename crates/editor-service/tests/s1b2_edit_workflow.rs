//! S1-B1.1 / S1-B2a: real public JSON calls and independent manufacturing truth.
use editor_core::edit::{EditError, EditHistory};
use editor_core::{Exposure, MmPoint, ObjectOrigin, SemanticGeometry, SemanticObject};
use editor_service::{ApplicationService, FileAccessPolicy, ObjectInfo};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);
// F_POL: radii 5 Dark, 3 Clear, 1 Dark, all centered at (2,2).
const POL: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,10*%\n%ADD11C,6*%\n%ADD12C,2*%\nD10*\nX2000000Y2000000D03*\n%LPC*%\nD11*\nX2000000Y2000000D03*\n%LPD*%\nD12*\nX2000000Y2000000D03*\nM02*\n";
struct Run {
    service: ApplicationService,
    dir: PathBuf,
    id: String,
    layer: String,
    calls: Vec<Value>,
}
impl Run {
    fn new() -> Self {
        Self::source(POL.as_bytes())
    }
    fn source(source: &[u8]) -> Self {
        let base = std::env::var_os("RCAM_S1B2_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!(
            "s1b2-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.gbr"), source).unwrap();
        let service = ApplicationService::with_file_access(FileAccessPolicy::new(
            dir.clone(),
            [dir.clone()],
            [dir.clone()],
        ));
        let mut run = Self {
            service,
            dir,
            id: String::new(),
            layer: String::new(),
            calls: vec![],
        };
        let opened = run.ok("document.open", None, json!({"path":"source.gbr"}));
        run.id = opened["document_id"].as_str().unwrap().into();
        run.layer = opened["layer_ids"][0].as_str().unwrap().into();
        run
    }
    fn call(&mut self, op: &str, rev: Option<&str>, params: Value) -> Value {
        let mut req = json!({"api_version":1,"request_id":format!("b2-{}",self.calls.len()),"op":op,"params":params});
        if !matches!(op, "document.open" | "system.capabilities") {
            req["document_id"] = json!(self.id);
        }
        if let Some(rev) = rev {
            req["expected_revision"] = json!(rev);
        }
        let reply = self.service.execute_json(&req.to_string());
        assert_eq!(reply["request_id"], req["request_id"]);
        self.calls.push(json!({"request":req,"response":reply}));
        reply
    }
    fn ok(&mut self, op: &str, rev: Option<&str>, params: Value) -> Value {
        let r = self.call(op, rev, params);
        assert_eq!(r["status"], "completed", "{r}");
        r["result"].clone()
    }
    fn info(&mut self) -> Value {
        self.ok("document.get", None, json!({}))
    }
    fn objects(&mut self) -> Vec<SemanticObject> {
        let result = self.ok("objects.query", None, json!({"layer_id":self.layer}));
        let objects: Vec<ObjectInfo> = serde_json::from_value(result["objects"].clone()).unwrap();
        objects.into_iter().map(|o| o.object).collect()
    }
    fn ids(&mut self) -> Vec<String> {
        self.objects().into_iter().map(|o| o.object_id).collect()
    }
    fn duplicate(&mut self, ids: &[String], rev: &str, dx: f64, dy: f64) -> Value {
        self.ok(
            "objects.duplicate",
            Some(rev),
            json!({"layer_id":self.layer,"object_ids":ids,"dx_mm":dx,"dy_mm":dy}),
        )
    }
    fn delete(&mut self, ids: &[String], rev: &str) -> Value {
        self.ok(
            "objects.delete",
            Some(rev),
            json!({"layer_id":self.layer,"object_ids":ids}),
        )
    }
    fn history(&mut self, op: &str, rev: &str) {
        self.ok(op, Some(rev), json!({}));
    }
    fn export_params(&self, path: &str) -> Value {
        json!({"layer_id":self.layer,"path":path,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}})
    }
    fn roundtrip(&mut self, rev: &str) -> gerber_io::S1Scene {
        self.ok("document.validate", None, json!({}));
        let source = std::fs::read(self.dir.join("source.gbr")).unwrap();
        let source_hash = self.info()["source_sha256"].clone();
        self.ok(
            "gerber.export_layer",
            Some(rev),
            self.export_params("edited.gbr"),
        );
        let opened = self.ok("document.open", None, json!({"path":"edited.gbr"}));
        assert!(
            self.service
                .validate(opened["document_id"].as_str().unwrap())
                .unwrap()
                .valid
        );
        assert_eq!(std::fs::read(self.dir.join("source.gbr")).unwrap(), source);
        assert_eq!(self.info()["source_sha256"], source_hash);
        gerber_io::parse_s1(
            &std::fs::read(self.dir.join("edited.gbr")).unwrap(),
            "reopened",
        )
        .unwrap()
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        if std::env::var_os("RCAM_S1B2_EVIDENCE").is_some() {
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
fn zero_move_does_not_create_history() {
    let mut r = Run::new();
    let ids = r.ids();
    r.ok(
        "objects.move",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":ids,"dx_mm":5,"dy_mm":-3}),
    );
    r.history("history.undo", "1");
    let before = r.objects();
    let info = r.info();
    for dx in [0.0, -0.0, f64::MIN_POSITIVE] {
        assert_eq!(
            r.call(
                "objects.move",
                Some("2"),
                json!({"layer_id":r.layer,"object_ids":ids,"dx_mm":dx,"dy_mm":0})
            )["error"]["code"],
            "INVALID_ARGUMENT"
        );
        assert_eq!(r.objects(), before);
        assert_eq!(r.info(), info);
    }
    r.history("history.redo", "2");
}
#[test]
fn missing_object_has_object_not_found_details() {
    let mut r = Run::new();
    for op in [
        "objects.get",
        "objects.move",
        "objects.duplicate",
        "objects.delete",
    ] {
        let p = match op {
            "objects.get" => json!({"layer_id":r.layer,"object_id":"missing"}),
            "objects.delete" => json!({"layer_id":r.layer,"object_ids":["missing"]}),
            _ => json!({"layer_id":r.layer,"object_ids":["missing"],"dx_mm":1,"dy_mm":0}),
        };
        let rev = if op == "objects.get" { None } else { Some("0") };
        let failed = r.call(op, rev, p);
        assert_eq!(failed["error"]["code"], "NOT_FOUND");
        assert_eq!(
            failed["error"]["details"],
            json!({"entity":"object","id":"missing"})
        );
    }
}
#[test]
fn missing_layer_has_layer_not_found_details() {
    let mut r = Run::new();
    let ids = r.ids();
    for op in [
        "objects.query",
        "objects.get",
        "objects.move",
        "objects.duplicate",
        "objects.delete",
        "gerber.export_layer",
    ] {
        let p = match op {
            "objects.query" => json!({"layer_id":"missing"}),
            "objects.get" => json!({"layer_id":"missing","object_id":ids[0]}),
            "objects.delete" => json!({"layer_id":"missing","object_ids":ids}),
            "gerber.export_layer" => {
                let mut p = r.export_params("missing.gbr");
                p["layer_id"] = json!("missing");
                p
            }
            _ => json!({"layer_id":"missing","object_ids":ids,"dx_mm":1,"dy_mm":0}),
        };
        let rev = if matches!(op, "objects.query" | "objects.get") {
            None
        } else {
            Some("0")
        };
        assert_eq!(
            r.call(op, rev, p)["error"]["details"],
            json!({"entity":"layer","id":"missing"})
        );
    }
    let error = r.service.document_get("absent").unwrap_err();
    assert_eq!(error.details, json!({"entity":"document","id":"absent"}));
}
#[test]
fn export_reports_saved_target_without_claiming_source_was_modified() {
    let mut r = Run::new();
    let original = r.info();
    assert_eq!(original["last_saved_path"], Value::Null);
    let ids = r.ids();
    r.duplicate(&ids, "0", 12.0, 0.0);
    r.roundtrip("1");
    let info = r.info();
    assert_eq!(info["source_path"], original["source_path"]);
    assert_eq!(info["source_sha256"], original["source_sha256"]);
    assert!(
        info["last_saved_path"]
            .as_str()
            .unwrap()
            .ends_with("edited.gbr")
    );
    assert_eq!(info["dirty"], false);
    r.history("history.undo", "1");
    assert_eq!(r.info()["dirty"], true);
    let before = r.info();
    assert_ne!(
        r.call(
            "gerber.export_layer",
            Some("2"),
            r.export_params("source.gbr")
        )["status"],
        "completed"
    );
    assert_eq!(r.info(), before);
    r.history("history.redo", "2");
    assert_eq!(r.info()["dirty"], false);
}
#[test]
fn duplicate_flash_has_new_stable_id() {
    let mut r = Run::new();
    let before = r.objects();
    let result = r.duplicate(&[before[0].object_id.clone()], "0", 5.0, -3.0);
    let after = r.objects();
    assert_eq!(after[0], before[0]);
    assert_eq!(&after[2..], &before[1..]);
    assert_eq!(result["changed_object_ids"], json!([after[1].object_id]));
    assert_ne!(after[1].object_id, before[0].object_id);
    let SemanticGeometry::Flash {
        center,
        aperture_id,
        transform,
    } = &after[1].geometry
    else {
        panic!()
    };
    assert_eq!(*center, MmPoint::new(7.0, -1.0));
    let SemanticGeometry::Flash {
        aperture_id: old,
        transform: ot,
        ..
    } = &before[0].geometry
    else {
        panic!()
    };
    assert_eq!(aperture_id, old);
    assert_eq!(transform, ot);
    assert_eq!(after[1].exposure, before[0].exposure);
    assert!(matches!(after[1].origin, ObjectOrigin::Generated { .. }));
    assert!(matches!(after[0].origin, ObjectOrigin::Imported { .. }));
    r.ok("document.validate", None, json!({}));
}
#[test]
fn duplicate_preserves_exposure_order() {
    let mut r = Run::new();
    let original = r.objects();
    let mut ids = r.ids();
    ids.reverse();
    let result = r.duplicate(&ids, "0", 0.0, 0.0);
    let after = r.objects();
    assert_eq!(after.len(), 6);
    assert_eq!(result["undo_entries_added"], 1);
    for i in 0..3 {
        assert_eq!(after[2 * i], original[i]);
        assert_eq!(after[2 * i + 1].geometry, original[i].geometry);
        assert_eq!(after[2 * i + 1].exposure, original[i].exposure);
    }
}
fn circle_truth(scene: &gerber_io::S1Scene, operations: &[(f64, f64, f64, bool)]) {
    // Independent analytic oracle, no parser/writer/coverage implementation reuse.
    for x in -24..100 {
        for y in -25..32 {
            let p = MmPoint::new(x as f64 * 0.2 + 0.013, y as f64 * 0.2 + 0.017);
            let mut expected = false;
            for &(cx, cy, radius, dark) in operations {
                if (p.x_mm - cx).powi(2) + (p.y_mm - cy).powi(2) <= radius * radius {
                    expected = dark;
                }
            }
            assert_eq!(
                scene
                    .document
                    .layer_coverage_at(&scene.document.layers[0].id, p),
                Some(expected),
                "{p:?}"
            );
        }
    }
}
#[test]
fn duplicate_clear_object_preserves_composition() {
    let mut r = Run::new();
    let ids = r.ids();
    r.duplicate(&[ids[1].clone()], "0", 2.0, 0.0);
    let scene = r.roundtrip("1");
    circle_truth(
        &scene,
        &[
            (2., 2., 5., true),
            (2., 2., 3., false),
            (4., 2., 3., false),
            (2., 2., 1., true),
        ],
    );
}
#[test]
fn duplicate_undo_redo_restores_same_ids() {
    let mut r = Run::new();
    let before = r.objects();
    let ids = r.ids();
    r.duplicate(&ids, "0", 5.0, -3.0);
    let after = r.objects();
    r.history("history.undo", "1");
    assert_eq!(r.objects(), before);
    assert_eq!(r.info()["dirty"], false);
    r.history("history.redo", "2");
    assert_eq!(r.objects(), after);
    assert_eq!(r.info()["revision"], "3");
}
#[test]
fn delete_multi_object_is_atomic() {
    let mut r = Run::new();
    let original = r.objects();
    let ids = r.ids();
    let result = r.delete(&[ids[2].clone(), ids[0].clone()], "0");
    assert_eq!(result["undo_entries_added"], 1);
    assert_eq!(result["changed_object_ids"], json!([ids[0], ids[2]]));
    assert_eq!(r.objects(), vec![original[1].clone()]);
}
#[test]
fn delete_undo_restores_exact_indices_and_ids() {
    let mut r = Run::new();
    let ids = r.ids();
    r.duplicate(&ids, "0", 5., -3.);
    let original = r.objects();
    let ids = r.ids();
    r.delete(&[ids[0].clone(), ids[3].clone(), ids[5].clone()], "1");
    r.history("history.undo", "2");
    assert_eq!(r.objects(), original);
}
#[test]
fn delete_redo_removes_same_objects() {
    let mut r = Run::new();
    let ids = r.ids();
    r.delete(&ids, "0");
    assert!(r.objects().is_empty());
    r.history("history.undo", "1");
    assert_eq!(r.ids(), ids);
    r.history("history.redo", "2");
    assert!(r.objects().is_empty());
}
fn rejected(op: &str) {
    let mut r = Run::new();
    let ids = r.ids();
    r.duplicate(&ids, "0", 1., 0.);
    r.history("history.undo", "1");
    let before = r.objects();
    let info = r.info();
    for bad in [
        vec![],
        vec![ids[0].clone(), "missing".into()],
        vec![ids[0].clone(), ids[0].clone()],
        (0..10001).map(|i| format!("id-{i}")).collect(),
    ] {
        let mut p = json!({"layer_id":r.layer,"object_ids":bad});
        if op == "objects.duplicate" {
            p["dx_mm"] = json!(1.);
            p["dy_mm"] = json!(0.);
        }
        assert_eq!(r.call(op, Some("2"), p)["status"], "error");
        assert_eq!(r.objects(), before);
        assert_eq!(r.info(), info);
    }
    if op == "objects.duplicate" {
        for dx in [1e300, 1e9] {
            assert_eq!(
                r.call(
                    op,
                    Some("2"),
                    json!({"layer_id":r.layer,"object_ids":ids,"dx_mm":dx,"dy_mm":0})
                )["status"],
                "error"
            );
            assert_eq!(r.objects(), before);
            assert_eq!(r.info(), info);
        }
        let result = r.duplicate(&ids, "2", 1., 0.);
        let created: Vec<String> =
            serde_json::from_value(result["changed_object_ids"].clone()).unwrap();
        // One previous successful insertion consumed 0..2, failed attempts consumed none.
        assert!(created[0].ends_with("-3"));
        assert_eq!(r.info()["redo_entries"], 0);
    } else {
        r.delete(&ids, "2");
        assert_eq!(r.info()["redo_entries"], 0);
    }
}
#[test]
fn failed_duplicate_preserves_document_history_and_id_contract() {
    rejected("objects.duplicate");
}
#[test]
fn failed_delete_preserves_document_history() {
    rejected("objects.delete");
}
#[test]
fn query_cursor_conflicts_after_insert_delete() {
    let mut r = Run::new();
    let ids = r.ids();
    for (rev, op) in [("0", "objects.duplicate"), ("1", "objects.delete")] {
        let page = r.ok("objects.query", None, json!({"layer_id":r.layer,"limit":1}));
        if op == "objects.duplicate" {
            r.duplicate(&ids, rev, 12., 0.);
        } else {
            r.delete(&ids, rev);
        }
        assert_eq!(
            r.call(
                "objects.query",
                None,
                json!({"layer_id":r.layer,"limit":1,"cursor":page["next_cursor"]})
            )["error"]["code"],
            "REVISION_CONFLICT"
        );
        assert_eq!(
            r.call(
                "objects.delete",
                Some(rev),
                json!({"layer_id":r.layer,"object_ids":ids})
            )["error"]["code"],
            "REVISION_CONFLICT"
        );
    }
}
#[test]
fn duplicate_export_reopen_matches_document() {
    // Overlapping and disjoint copies exercise the declared interleaved ordering.
    for dx in [0., 2., 12.] {
        let mut r = Run::new();
        let ids = r.ids();
        r.duplicate(&ids, "0", dx, 0.);
        let scene = r.roundtrip("1");
        assert_eq!(scene.document.layers[0].objects.len(), 6);
        circle_truth(
            &scene,
            &[
                (2., 2., 5., true),
                (2. + dx, 2., 5., true),
                (2., 2., 3., false),
                (2. + dx, 2., 3., false),
                (2., 2., 1., true),
                (2. + dx, 2., 1., true),
            ],
        );
    }
}
#[test]
fn delete_export_reopen_matches_document() {
    let mut r = Run::new();
    let ids = r.ids();
    r.delete(&[ids[1].clone()], "0");
    let scene = r.roundtrip("1");
    assert_eq!(scene.document.layers[0].objects.len(), 2);
    circle_truth(&scene, &[(2., 2., 5., true), (2., 2., 1., true)]);
}
#[test]
fn deleted_and_undone_ids_are_never_reallocated() {
    let mut r = Run::new();
    let ids = r.ids();
    let a = r.duplicate(&ids, "0", 0., 0.);
    let created: Vec<String> = serde_json::from_value(a["changed_object_ids"].clone()).unwrap();
    r.delete(&created, "1");
    let b = r.duplicate(&ids, "2", 0., 0.);
    let new: Vec<String> = serde_json::from_value(b["changed_object_ids"].clone()).unwrap();
    assert!(new.iter().all(|id| !created.contains(id)));
    assert!(new[0].ends_with("-3"));
}
#[test]
fn core_locked_and_budget_edits_are_atomic() {
    let mut scene = gerber_io::parse_s1(POL.as_bytes(), "locked").unwrap();
    let layer = scene.document.layers[0].id.clone();
    let ids: Vec<_> = scene.document.layers[0]
        .objects
        .iter()
        .map(|o| o.object_id.clone())
        .collect();
    let mut h = EditHistory::default();
    scene.document.layers[0].locked = true;
    let before = scene.document.clone();
    assert!(matches!(
        h.duplicate_objects(&mut scene.document, &layer, &ids, 1., 0.),
        Err(EditError::LayerLocked(_))
    ));
    assert!(matches!(
        h.delete_objects(&mut scene.document, &layer, &ids),
        Err(EditError::LayerLocked(_))
    ));
    assert_eq!(scene.document, before);
    assert_eq!(h.undo_len(), 0);
    scene.document.layers[0].locked = false;
    for _ in 0..100 {
        h.move_objects(&mut scene.document, &layer, &ids, 1., 0.)
            .unwrap();
    }
    let before = scene.document.clone();
    assert_eq!(
        h.duplicate_objects(&mut scene.document, &layer, &ids, 1., 0.),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(
        h.delete_objects(&mut scene.document, &layer, &ids),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(scene.document, before);
    assert_eq!(h.undo_len(), 100);
}
#[test]
fn capabilities_and_strict_dtos_match_implemented_operations() {
    let mut r = Run::new();
    let ids = r.ids();
    let caps = r.ok("system.capabilities", None, json!({}));
    for op in [
        "objects.duplicate",
        "objects.delete",
        "objects.rotate",
        "objects.mirror",
    ] {
        assert!(
            caps["supported_operations"]
                .as_array()
                .unwrap()
                .contains(&json!(op))
        );
    }
    {
        let op = "text.create";
        assert!(
            !caps["supported_operations"]
                .as_array()
                .unwrap()
                .contains(&json!(op))
        );
    }
    for (op, p) in [
        (
            "objects.duplicate",
            json!({"layer_id":r.layer,"object_ids":ids,"dx_mm":1,"dy_mm":0,"extra":1}),
        ),
        (
            "objects.delete",
            json!({"layer_id":r.layer,"object_ids":ids,"dx_mm":1}),
        ),
    ] {
        assert_eq!(
            r.call(op, Some("0"), p)["error"]["code"],
            "INVALID_ARGUMENT"
        );
    }
    assert_eq!(r.info()["revision"], "0");
    assert_eq!(r.objects()[1].exposure, Exposure::Clear);
}

#[test]
fn late_duplicate_failure_preserves_all_objects_and_allocator() {
    let bytes =
        b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX0Y0D03*\nX2000000Y2000000D03*\nM02*\n";
    let mut r = Run::source(bytes);
    let ids = r.ids();
    let before = r.objects();
    let info = r.info();
    let failed = r.call(
        "objects.duplicate",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":ids,"dx_mm":1e9,"dy_mm":0}),
    );
    assert_eq!(failed["error"]["code"], "INVALID_ARGUMENT");
    assert_eq!(r.objects(), before);
    assert_eq!(r.info(), info);
    let ok = r.duplicate(&ids, "0", 5., -3.);
    assert!(
        ok["changed_object_ids"][0]
            .as_str()
            .unwrap()
            .ends_with("-0")
    );
}

#[test]
fn mixed_geometry_duplicate_delete_roundtrip_preserves_originals() {
    // Copies of true arcs, a Region with arcs, local macro transparency and
    // rectangular sweeps exercise shared translation and complete-object history.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
    for relative in [
        "s1a1/g75_full.gbr",
        "s1a1/g75_small_deviation.gbr",
        "s1a1/g74_zero.gbr",
        "s1a/region_arc.gbr",
        "s1a/region_cutin.gbr",
        "s1a/macro_hole_over_line.gbr",
    ] {
        let bytes = std::fs::read(root.join(relative)).unwrap();
        let mut r = Run::source(&bytes);
        let before = r.objects();
        let ids = r.ids();
        r.duplicate(&ids, "0", 5., -3.);
        let duplicated = r.objects();
        for (i, object) in before.iter().enumerate() {
            assert_eq!(duplicated[2 * i], *object);
        }
        r.delete(&ids, "1");
        let copies = r.objects();
        assert_eq!(copies.len(), before.len());
        r.history("history.undo", "2");
        assert_eq!(r.objects(), duplicated);
        r.history("history.redo", "3");
        assert_eq!(r.objects(), copies);
        let output = r.roundtrip("4");
        let original = gerber_io::parse_s1(&bytes, "original").unwrap();
        for x in -8..=24 {
            for y in -8..=24 {
                let p = MmPoint::new(x as f64 * 0.25 + 0.019, y as f64 * 0.25 + 0.011);
                assert_eq!(
                    original
                        .document
                        .layer_coverage_at(&original.document.layers[0].id, p),
                    output.document.layer_coverage_at(
                        &output.document.layers[0].id,
                        MmPoint::new(p.x_mm + 5., p.y_mm - 3.)
                    ),
                    "{relative} {p:?}"
                );
            }
        }
        for (old, new) in before.iter().zip(&copies) {
            if let (SemanticGeometry::Arc { path: a, .. }, SemanticGeometry::Arc { path: b, .. }) =
                (&old.geometry, &new.geometry)
            {
                assert_eq!(a.direction, b.direction);
                assert_eq!(a.full_circle, b.full_circle);
                assert_eq!(a.source, b.source);
                assert!((b.center.x_mm - a.center.x_mm - 5.).abs() < 1e-6);
                assert!((b.center.y_mm - a.center.y_mm + 3.).abs() < 1e-6);
            }
        }
    }
    let bytes = b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10R,2X1*%\nD10*\nX0Y0D02*\nG01X3000000Y0D01*\nM02*\n";
    let mut r = Run::source(bytes);
    let ids = r.ids();
    r.duplicate(&ids, "0", 5., -3.);
    r.delete(&ids, "1");
    let output = r.roundtrip("2");
    assert!(
        matches!(output.document.layers[0].objects[0].geometry,SemanticGeometry::RectangularSweep{start,end,width_mm:2.0,height_mm:1.0} if start==MmPoint::new(5.,-3.) && end==MmPoint::new(8.,-3.))
    );
    assert_eq!(
        output
            .document
            .layer_coverage_at(&output.document.layers[0].id, MmPoint::new(4.01, -3.49)),
        Some(true)
    );
}

#[test]
fn structural_byte_budget_rejects_without_losing_redo() {
    let mut scene = gerber_io::parse_s1(POL.as_bytes(), "budget").unwrap();
    let layer = scene.document.layers[0].id.clone();
    let template = scene.document.layers[0].objects[0].clone();
    // Valid imported IDs with large provenance strings make the bounded order
    // guards exceed 64 MiB; reject before cloning transaction geometry.
    scene.document.layers[0].objects = (0..10_000)
        .map(|i| {
            let mut o = template.clone();
            o.object_id = format!("{i}-{}", "x".repeat(1800));
            o
        })
        .collect();
    scene.document.validate().unwrap();
    let ids = vec![scene.document.layers[0].objects[0].object_id.clone()];
    let mut h = EditHistory::default();
    h.move_objects(&mut scene.document, &layer, &ids, 1., 0.)
        .unwrap();
    h.undo(&mut scene.document).unwrap();
    let before = scene.document.clone();
    assert_eq!(
        h.duplicate_objects(&mut scene.document, &layer, &ids, 1., 0.),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(
        h.delete_objects(&mut scene.document, &layer, &ids),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(scene.document, before);
    assert_eq!(h.undo_len(), 0);
    assert_eq!(h.redo_len(), 1);
}

#[test]
fn empty_layer_exports_and_reopens_after_delete_all() {
    let mut r = Run::new();
    let ids = r.ids();
    r.delete(&ids, "0");
    let scene = r.roundtrip("1");
    assert!(scene.document.layers[0].objects.is_empty());
    assert_eq!(r.info()["dirty"], false);
    r.history("history.undo", "1");
    assert_eq!(r.ids(), ids);
    assert_eq!(r.info()["dirty"], true);
}

#[test]
fn direct_duplicate_rejects_nonfinite_parameters_and_json_requires_revision() {
    let mut r = Run::new();
    let ids = r.ids();
    let before = r.info();
    for dx in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = r
            .service
            .objects_duplicate(
                &r.id,
                "0",
                editor_service::DuplicateParams {
                    layer_id: r.layer.clone(),
                    object_ids: ids.clone(),
                    dx_mm: dx,
                    dy_mm: 0.,
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "INVALID_ARGUMENT");
        assert_eq!(r.info(), before);
    }
    for (op, params) in [
        (
            "objects.duplicate",
            json!({"layer_id":r.layer,"object_ids":ids,"dx_mm":0,"dy_mm":0}),
        ),
        (
            "objects.delete",
            json!({"layer_id":r.layer,"object_ids":ids}),
        ),
    ] {
        assert_eq!(
            r.call(op, None, params)["error"]["code"],
            "INVALID_ARGUMENT"
        );
        assert_eq!(r.info(), before);
    }
}

#[test]
fn writer_obeys_current_order_not_import_provenance() {
    let mut scene = gerber_io::parse_s1(POL.as_bytes(), "order").unwrap();
    // Reordering is internal test setup, not an exposed reorder operation.
    scene.document.layers[0].objects.swap(0, 1);
    scene.document.validate().unwrap();
    let mut r = Run::new();
    let path = r.dir.join("order.gbr");
    gerber_io::export_s1_new_path(&scene.document, &path).unwrap();
    let output = gerber_io::parse_s1(&std::fs::read(path).unwrap(), "out").unwrap();
    circle_truth(
        &output,
        &[(2., 2., 3., false), (2., 2., 5., true), (2., 2., 1., true)],
    );
    r.ok("document.open", None, json!({"path":"order.gbr"}));
}

#[test]
fn history_guards_reject_changed_order_and_locked_replay() {
    let mut scene = gerber_io::parse_s1(POL.as_bytes(), "guard").unwrap();
    let layer = scene.document.layers[0].id.clone();
    let ids = vec![scene.document.layers[0].objects[0].object_id.clone()];
    let mut h = EditHistory::default();
    h.duplicate_objects(&mut scene.document, &layer, &ids, 0., 0.)
        .unwrap();
    scene.document.layers[0].objects.swap(0, 2);
    let before = scene.document.clone();
    assert_eq!(h.undo(&mut scene.document), Err(EditError::InvalidArgument));
    assert_eq!(scene.document, before);
    assert_eq!(h.undo_len(), 1);
    scene.document.layers[0].objects.swap(0, 2);
    scene.document.layers[0].locked = true;
    let before = scene.document.clone();
    assert!(matches!(
        h.undo(&mut scene.document),
        Err(EditError::LayerLocked(_))
    ));
    assert_eq!(scene.document, before);
}

#[test]
fn empty_image_support_still_rejects_malformed_and_unknown_input() {
    for bytes in [
        b"".as_slice(),
        b"M02*\n",
        b"%FSLAX26Y26*%\nM02*\n",
        b"%MOMM*%\nM02*\n",
        b"%FSLAX26Y26*%\n%MOMM*%\n",
        b"%FSLAX26Y26*%\n%MOMM*%\n%ZZUNKNOWN*%\nM02*\n",
        b"%FSLAX26Y26*%\n%MOMM*%\nD99*\nM02*\n",
    ] {
        assert!(
            gerber_io::parse_s1(bytes, "bad").is_err(),
            "accepted {bytes:?}"
        );
    }
}
