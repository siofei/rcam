//! S1-B1 real JSON service workflow, plus independent physical assertions.
use editor_core::{ArcDirection, MmPoint, RegionEdge, SemanticGeometry};
use editor_service::{ApplicationService, FileAccessPolicy, MoveParams, ObjectInfo};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);
const MIXED: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX2000000Y2000000D03*\nX0Y0D02*\nG01X2000000Y0D01*\nG75*\nX1000000Y0D02*\nG03X0Y1000000I-1000000J0D01*\nG36*\nX10000000Y10000000D02*\nG01X12000000Y10000000D01*\nX12000000Y12000000D01*\nX10000000Y12000000D01*\nX10000000Y10000000D01*\nG37*\nM02*\n";

struct Run {
    service: ApplicationService,
    dir: PathBuf,
    id: String,
    layer: String,
    source: Vec<u8>,
    original_hash: String,
    calls: Vec<Value>,
}

impl Run {
    fn new(name: &str, source: &[u8]) -> Self {
        let base = std::env::var_os("RCAM_S1B_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!(
            "{name}-{}-{}",
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
            source: source.to_vec(),
            original_hash: String::new(),
            calls: Vec::new(),
        };
        let opened = run.ok("document.open", None, json!({"path":"source.gbr"}));
        run.id = opened["document_id"].as_str().unwrap().into();
        run.layer = opened["layer_ids"][0].as_str().unwrap().into();
        run.original_hash = opened["source_sha256"].as_str().unwrap().into();
        run
    }
    fn call(&mut self, op: &str, revision: Option<&str>, params: Value) -> Value {
        let mut request = json!({"api_version":1,"request_id":format!("s1b-{}",self.calls.len()),"op":op,"params":params});
        if !matches!(op, "document.open" | "system.capabilities") {
            request["document_id"] = json!(self.id);
        }
        if let Some(revision) = revision {
            request["expected_revision"] = json!(revision);
        }
        let result = self.service.execute_json(&request.to_string());
        assert_eq!(result["request_id"], request["request_id"]);
        self.calls
            .push(json!({"request":request,"response":result}));
        result
    }
    fn ok(&mut self, op: &str, revision: Option<&str>, params: Value) -> Value {
        let result = self.call(op, revision, params);
        assert_eq!(result["status"], "completed", "{result}");
        result["result"].clone()
    }
    fn objects(&mut self) -> Vec<ObjectInfo> {
        let result = self.ok("objects.query", None, json!({"layer_id":self.layer}));
        serde_json::from_value(result["objects"].clone()).unwrap()
    }
    fn info(&mut self) -> Value {
        self.ok("document.get", None, json!({}))
    }
    fn move_ids(&mut self, ids: &[String], revision: &str) -> Value {
        self.ok(
            "objects.move",
            Some(revision),
            json!({"layer_id":self.layer,"object_ids":ids,"dx_mm":5.0,"dy_mm":-3.0}),
        )
    }
    fn move_all(&mut self, revision: &str) -> Value {
        let ids: Vec<_> = self
            .objects()
            .iter()
            .map(|x| x.object.object_id.clone())
            .collect();
        self.move_ids(&ids, revision)
    }
    fn history(&mut self, op: &str, revision: &str) -> Value {
        self.ok(op, Some(revision), json!({}))
    }
    fn export(&mut self, revision: &str, name: &str) -> Value {
        self.ok("gerber.export_layer", Some(revision), json!({"layer_id":self.layer,"path":name,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}}))
    }
    fn roundtrip(&mut self, revision: &str) -> gerber_io::S1Scene {
        self.ok("document.validate", None, json!({}));
        let exported = self.export(revision, "edited.gbr");
        assert_eq!(exported["exported_revision"], revision);
        let opened = self.ok("document.open", None, json!({"path":"edited.gbr"}));
        let reopened_id = opened["document_id"].as_str().unwrap();
        assert_ne!(reopened_id, self.id);
        assert!(self.service.validate(reopened_id).unwrap().valid);
        let scene = gerber_io::parse_s1(
            &std::fs::read(self.dir.join("edited.gbr")).unwrap(),
            "expected",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(self.dir.join("source.gbr")).unwrap(),
            self.source
        );
        let reopened_source = self.ok("document.open", None, json!({"path":"source.gbr"}));
        assert_eq!(reopened_source["source_sha256"], self.original_hash);
        scene
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        if std::env::var_os("RCAM_S1B_EVIDENCE").is_some() {
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
fn point(actual: MmPoint, x: f64, y: f64) {
    assert!(
        (actual.x_mm - x).abs() < 1e-6 && (actual.y_mm - y).abs() < 1e-6,
        "{actual:?} != ({x},{y})"
    );
}
fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic")
            .join(relative),
    )
    .unwrap()
}
fn assert_mixed_geometry(objects: &[ObjectInfo]) {
    match &objects[0].object.geometry {
        SemanticGeometry::Flash { center, .. } => point(*center, 7.0, -1.0),
        _ => panic!(),
    }
    match &objects[1].object.geometry {
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => {
            point(*start, 5.0, -3.0);
            point(*end, 7.0, -3.0);
            assert_eq!(*width_mm, 0.2);
        }
        _ => panic!(),
    }
    match &objects[2].object.geometry {
        SemanticGeometry::Arc { path, .. } => {
            point(path.start, 6.0, -3.0);
            point(path.end, 5.0, -2.0);
            point(path.center, 5.0, -3.0);
            assert_eq!(path.direction, ArcDirection::CounterClockwise);
        }
        _ => panic!(),
    }
    match &objects[3].object.geometry {
        SemanticGeometry::Region { contours } => {
            let expected = [
                (15.0, 7.0, 17.0, 7.0),
                (17.0, 7.0, 17.0, 9.0),
                (17.0, 9.0, 15.0, 9.0),
                (15.0, 9.0, 15.0, 7.0),
            ];
            assert_eq!(contours.len(), 1);
            assert_eq!(contours[0].edges.len(), 4);
            for (edge, (sx, sy, ex, ey)) in contours[0].edges.iter().zip(expected) {
                let RegionEdge::Line { start, end } = edge else {
                    panic!()
                };
                point(*start, sx, sy);
                point(*end, ex, ey);
            }
        }
        _ => panic!(),
    }
}

#[test]
fn move_flash_roundtrip() {
    let mut run = Run::new("flash", MIXED.as_bytes());
    let original = run.objects();
    let id = original[0].object.object_id.clone();
    run.move_ids(&[id], "0");
    let moved = run.objects();
    assert_eq!(&moved[1..], &original[1..]);
    let SemanticGeometry::Flash {
        center,
        aperture_id,
        transform,
    } = &moved[0].object.geometry
    else {
        panic!()
    };
    point(*center, 7.0, -1.0);
    let SemanticGeometry::Flash {
        aperture_id: old,
        transform: old_transform,
        ..
    } = &original[0].object.geometry
    else {
        panic!()
    };
    assert_eq!(aperture_id, old);
    assert_eq!(transform, old_transform);
    let scene = run.roundtrip("1");
    assert!(
        scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(7.0, -1.0))
            .unwrap()
    );
    assert!(
        !scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(2.0, 2.0))
            .unwrap()
    );
}
#[test]
fn move_line_roundtrip() {
    let mut run = Run::new("line", MIXED.as_bytes());
    let before = run.objects();
    run.move_ids(&[before[1].object.object_id.clone()], "0");
    let after = run.objects();
    assert_eq!(after[0], before[0]);
    assert_eq!(&after[2..], &before[2..]);
    let scene = run.roundtrip("1");
    let SemanticGeometry::Line {
        start,
        end,
        width_mm,
    } = &scene.document.layers[0].objects[1].geometry
    else {
        panic!()
    };
    point(*start, 5.0, -3.0);
    point(*end, 7.0, -3.0);
    assert_eq!(*width_mm, 0.2);
}
#[test]
fn move_arc_preserves_arc_semantics() {
    for name in [
        "g75_exact",
        "g75_small_deviation",
        "g75_full",
        "g75_near_full",
        "g74_ccw_quarter",
        "g74_cw_quarter",
        "g74_zero",
    ] {
        let mut run = Run::new(name, &fixture(&format!("s1a1/{name}.gbr")));
        let original = run.objects();
        run.move_all("0");
        let moved = run.objects();
        let SemanticGeometry::Arc {
            path: before,
            width_mm: bw,
        } = &original[0].object.geometry
        else {
            panic!()
        };
        let SemanticGeometry::Arc {
            path: after,
            width_mm: aw,
        } = &moved[0].object.geometry
        else {
            panic!()
        };
        point(
            after.start,
            before.start.x_mm + 5.0,
            before.start.y_mm - 3.0,
        );
        point(after.end, before.end.x_mm + 5.0, before.end.y_mm - 3.0);
        point(
            after.center,
            before.center.x_mm + 5.0,
            before.center.y_mm - 3.0,
        );
        assert_eq!(before.full_circle, after.full_circle);
        assert_eq!(before.source, after.source);
        assert_eq!(before.direction, after.direction);
        assert_eq!(bw, aw);
        assert_eq!(before.start == before.end, after.start == after.end);
        assert!(
            (before.start.distance_mm(before.center) - after.start.distance_mm(after.center)).abs()
                < 1e-6
        );
        assert!(
            (before.end.distance_mm(before.center) - after.end.distance_mm(after.center)).abs()
                < 1e-6
        );
        run.history("history.undo", "1");
        assert_eq!(run.objects(), original);
        run.history("history.redo", "2");
        assert_eq!(run.objects(), moved);
        // Preserve the historical sub-grid arc truth at its original FS resolution.
        run.service
            .set_manufacturing_precision(
                &run.id,
                "3",
                editor_service::ManufacturingPrecision {
                    resolution_mm: 0.000001,
                },
            )
            .unwrap();
        let scene = run.roundtrip("4");
        if name == "g74_zero" {
            assert!(
                matches!(scene.document.layers[0].objects[0].geometry,SemanticGeometry::Line{start,end,..} if start==end)
            );
        } else {
            let SemanticGeometry::Arc { path, .. } = &scene.document.layers[0].objects[0].geometry
            else {
                panic!()
            };
            point(path.center, after.center.x_mm, after.center.y_mm);
            assert_eq!(path.direction, after.direction);
        }
    }
}
#[test]
fn move_region_preserves_topology() {
    for name in ["region_arc", "region_cutin"] {
        let bytes = fixture(&format!("s1a/{name}.gbr"));
        let original = gerber_io::parse_s1(&bytes, "original").unwrap();
        let mut run = Run::new(name, &bytes);
        let before = run.objects();
        run.move_all("0");
        let moved = run.objects();
        run.history("history.undo", "1");
        assert_eq!(run.objects(), before);
        run.history("history.redo", "2");
        assert_eq!(run.objects(), moved);
        let output = run.roundtrip("3");
        // Independent fixed physical grid includes the cut-in's local transparent hole.
        for x in -4..=20 {
            for y in -4..=20 {
                let p = MmPoint::new(x as f64 * 0.25 + 0.013, y as f64 * 0.25 + 0.017);
                assert_eq!(
                    original
                        .document
                        .layer_coverage_at(&original.document.layers[0].id, p),
                    output.document.layer_coverage_at(
                        &output.document.layers[0].id,
                        MmPoint::new(p.x_mm + 5.0, p.y_mm - 3.0)
                    )
                );
            }
        }
        if name == "region_cutin" {
            let layer = &output.document.layers[0].id;
            assert_eq!(
                output
                    .document
                    .layer_coverage_at(layer, MmPoint::new(7.0, -1.0)),
                Some(false)
            );
            assert_eq!(
                output
                    .document
                    .layer_coverage_at(layer, MmPoint::new(5.5, -2.5)),
                Some(true)
            );
        }
    }
}
#[test]
fn multi_object_move_is_one_transaction() {
    let mut run = Run::new("multi", MIXED.as_bytes());
    let original = run.objects();
    let result = run.move_all("0");
    assert_eq!(result["undo_entries_added"], 1);
    assert_eq!(result["undo_entries"], 1);
    assert_mixed_geometry(&run.objects());
    run.history("history.undo", "1");
    assert_eq!(run.objects(), original);
    assert_eq!(run.info()["undo_entries"], 0);
}
#[test]
fn failed_move_is_atomic() {
    let mut run = Run::new("atomic", MIXED.as_bytes());
    let before = run.objects();
    let info = run.info();
    let id = before[0].object.object_id.clone();
    for (ids, dx, dy, code) in [
        (vec![], 5.0, -3.0, "INVALID_ARGUMENT"),
        (vec![id.clone(), "unknown".into()], 5.0, -3.0, "NOT_FOUND"),
        (vec![id.clone(), id.clone()], 5.0, -3.0, "INVALID_ARGUMENT"),
        (vec![id.clone()], 1e300, 0.0, "INVALID_ARGUMENT"),
    ] {
        let failed = run.call(
            "objects.move",
            Some("0"),
            json!({"layer_id":run.layer,"object_ids":ids,"dx_mm":dx,"dy_mm":dy}),
        );
        assert_eq!(failed["error"]["code"], code);
        assert_eq!(run.objects(), before);
        assert_eq!(run.info(), info);
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = run
            .service
            .objects_move(
                &run.id,
                "0",
                MoveParams {
                    layer_id: run.layer.clone(),
                    object_ids: vec![id.clone()],
                    dx_mm: invalid,
                    dy_mm: 0.0,
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "INVALID_ARGUMENT");
        assert_eq!(run.info(), info);
    }
    // First object is valid but the later arc loses numeric safety at this offset.
    let failed=run.call("objects.move",Some("0"),json!({"layer_id":run.layer,"object_ids":[id,before[2].object.object_id],"dx_mm":1e8,"dy_mm":0.0}));
    assert_eq!(failed["status"], "error");
    assert_eq!(run.objects(), before);
    assert_eq!(run.info(), info);
}
#[test]
fn undo_restores_original_geometry() {
    let mut run = Run::new("undo", MIXED.as_bytes());
    let original = run.objects();
    run.move_all("0");
    let result = run.history("history.undo", "1");
    assert_eq!(result["revision"], "2");
    assert_eq!(result["dirty"], false);
    assert_eq!(run.objects(), original);
}
#[test]
fn redo_restores_moved_geometry() {
    let mut run = Run::new("redo", MIXED.as_bytes());
    run.move_all("0");
    let moved = run.objects();
    run.history("history.undo", "1");
    let result = run.history("history.redo", "2");
    assert_eq!(result["revision"], "3");
    assert_eq!(result["dirty"], true);
    assert_eq!(run.objects(), moved);
}
#[test]
fn new_edit_clears_redo_branch() {
    let mut run = Run::new("branch", MIXED.as_bytes());
    run.move_all("0");
    run.history("history.undo", "1");
    let failed = run.call(
        "objects.move",
        Some("2"),
        json!({"layer_id":run.layer,"object_ids":[],"dx_mm":1.0,"dy_mm":0.0}),
    );
    assert_eq!(failed["status"], "error");
    assert_eq!(run.info()["redo_entries"], 1);
    run.move_all("2");
    assert_eq!(run.info()["redo_entries"], 0);
    let before = run.info();
    assert_eq!(
        run.call("history.redo", Some("3"), json!({}))["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(run.info(), before);
}
#[test]
fn stale_revision_returns_revision_conflict() {
    let mut run = Run::new("conflict", MIXED.as_bytes());
    let ids: Vec<_> = run
        .objects()
        .iter()
        .map(|o| o.object.object_id.clone())
        .collect();
    let page = run.ok(
        "objects.query",
        None,
        json!({"layer_id":run.layer,"limit":1}),
    );
    run.move_all("0");
    run.history("history.undo", "1");
    let before = run.info();
    assert_eq!(
        run.call(
            "objects.move",
            Some("0"),
            json!({"layer_id":run.layer,"object_ids":ids,"dx_mm":5,"dy_mm":-3})
        )["error"]["code"],
        "REVISION_CONFLICT"
    );
    assert_eq!(
        run.call(
            "objects.query",
            None,
            json!({"layer_id":run.layer,"limit":1,"cursor":page["next_cursor"]})
        )["error"]["code"],
        "REVISION_CONFLICT"
    );
    assert_eq!(
        run.call("history.redo", Some("0"), json!({}))["error"]["code"],
        "REVISION_CONFLICT"
    );
    assert_eq!(run.info(), before);
}
#[test]
fn export_reopen_matches_edited_document() {
    let mut run = Run::new("full-json-workflow", MIXED.as_bytes());
    let original = run.objects();
    run.move_all("0");
    let moved = run.objects();
    assert_mixed_geometry(&moved);
    run.history("history.undo", "1");
    assert_eq!(run.objects(), original);
    run.history("history.redo", "2");
    let scene = run.roundtrip("3");
    let objects: Vec<_> = scene.document.layers[0]
        .objects
        .iter()
        .map(|object| ObjectInfo {
            layer_id: scene.document.layers[0].id.clone(),
            object: object.clone(),
        })
        .collect();
    assert_mixed_geometry(&objects);
    for (a, b) in moved.iter().zip(&objects) {
        assert_eq!(a.object.exposure, b.object.exposure);
    }
    // Export is a copy: project dirty is relative to the Workspace baseline only.
    assert_eq!(run.info()["dirty"], true);
    run.history("history.undo", "3");
    assert_eq!(run.info()["dirty"], false);
    run.history("history.redo", "4");
    assert_eq!(run.info()["dirty"], true);
}
#[test]
fn source_file_is_never_modified() {
    let mut run = Run::new("source-protection", MIXED.as_bytes());
    run.move_all("0");
    let failed=run.call("gerber.export_layer",Some("1"),json!({"layer_id":run.layer,"path":"source.gbr","overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}}));
    // An existing file (here the imported original) is never replaced.
    assert_eq!(failed["status"], "confirmation_required");
    assert_eq!(run.info()["dirty"], true);
    run.roundtrip("1");
    assert_eq!(
        std::fs::read(run.dir.join("source.gbr")).unwrap(),
        run.source
    );
}
#[test]
fn close_discards_history_and_does_not_reuse_ids() {
    let mut run = Run::new("close", MIXED.as_bytes());
    run.move_all("0");
    assert_eq!(
        run.call("document.close", Some("1"), json!({}))["status"],
        "confirmation_required"
    );
    run.ok("document.close", Some("1"), json!({"discard_changes":true}));
    assert_eq!(
        run.call("history.undo", Some("1"), json!({}))["error"]["code"],
        "NOT_FOUND"
    );
    let opened = run.ok("document.open", None, json!({"path":"source.gbr"}));
    assert_ne!(opened["document_id"], run.id);
    assert_eq!(opened["undo_entries"], 0);
    assert_eq!(opened["revision"], "0");
}
#[test]
fn history_entry_budget_evicts_oldest_complete_transaction() {
    let mut run = Run::new("budget", MIXED.as_bytes());
    for i in 0..100 {
        run.move_all(&i.to_string());
    }
    let ids: Vec<_> = run
        .objects()
        .iter()
        .map(|o| o.object.object_id.clone())
        .collect();
    let result = run.ok(
        "objects.move",
        Some("100"),
        json!({"layer_id":run.layer,"object_ids":ids,"dx_mm":5,"dy_mm":-3}),
    );
    assert_eq!(result["revision"], "101");
    assert_eq!(result["undo_entries"], 100);
    assert_eq!(result["history_truncated_entries"], 1);
    assert!(result["history_truncated_bytes"].as_u64().unwrap() > 0);
}
#[test]
fn geometry_query_tracks_move_and_undo() {
    let mut run = Run::new("query", MIXED.as_bytes());
    let id = run.objects()[0].object.object_id.clone();
    let params = json!({"layer_id":run.layer,"geometry_type":"flash","region_mm":{"min_x_mm":1.8,"min_y_mm":1.8,"max_x_mm":2.2,"max_y_mm":2.2},"relation":"intersects"});
    assert_eq!(run.ok("objects.query", None, params.clone())["total"], 1);
    run.move_ids(&[id], "0");
    assert_eq!(run.ok("objects.query", None, params.clone())["total"], 0);
    run.history("history.undo", "1");
    assert_eq!(run.ok("objects.query", None, params)["total"], 1);
}

#[test]
fn rejected_json_and_failed_export_preserve_geometry_history_and_saved_baseline() {
    let mut run = Run::new("rejections", MIXED.as_bytes());
    let ids: Vec<_> = run
        .objects()
        .iter()
        .map(|o| o.object.object_id.clone())
        .collect();
    for params in [
        json!({"layer_id":run.layer,"object_ids":ids,"dx_mm":5,"dy_mm":-3,"future":true}),
        json!({"layer_id":run.layer,"object_ids":ids,"dx_mm":"NaN","dy_mm":0}),
        json!({"layer_id":"other-layer","object_ids":ids,"dx_mm":5,"dy_mm":-3}),
    ] {
        assert_eq!(
            run.call("objects.move", Some("0"), params)["status"],
            "error"
        );
        assert_eq!(run.info()["revision"], "0");
        assert_eq!(run.info()["undo_entries"], 0);
    }
    let too_many: Vec<_> = (0..=editor_core::edit::MAX_MOVE_TARGETS)
        .map(|i| format!("object-{i}"))
        .collect();
    assert_eq!(
        run.call(
            "objects.move",
            Some("0"),
            json!({"layer_id":run.layer,"object_ids":too_many,"dx_mm":1,"dy_mm":1})
        )["error"]["code"],
        "RESOURCE_LIMIT"
    );
    assert_eq!(
        run.call(
            "objects.move",
            None,
            json!({"layer_id":run.layer,"object_ids":ids,"dx_mm":1,"dy_mm":1})
        )["error"]["code"],
        "INVALID_ARGUMENT"
    );
    run.move_all("0");
    let before = run.info();
    let geometry = run.objects();
    std::fs::write(run.dir.join("existing.gbr"), b"sentinel").unwrap();
    for (rev, target) in [("0", "fresh.gbr"), ("1", "existing.gbr")] {
        let result=run.call("gerber.export_layer",Some(rev),json!({"layer_id":run.layer,"path":target,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}}));
        assert_ne!(result["status"], "completed");
        assert_eq!(run.info(), before);
        assert_eq!(run.objects(), geometry);
    }
    assert_eq!(
        std::fs::read(run.dir.join("existing.gbr")).unwrap(),
        b"sentinel"
    );
    assert!(!run.dir.join("fresh.gbr").exists());
    // A new inverse move is still a transaction, but its exact restored content is clean.
    run.ok(
        "objects.move",
        Some("1"),
        json!({"layer_id":run.layer,"object_ids":ids,"dx_mm":-5.0,"dy_mm":3.0}),
    );
    assert_eq!(run.info()["dirty"], false);
    assert_eq!(run.info()["undo_entries"], 2);
}

#[test]
fn rectangular_sweep_and_local_macro_aperture_roundtrip() {
    for (name, bytes) in [
        (
            "rect",
            b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10R,2X1*%\nD10*\nX0Y0D02*\nG01X3000000Y0D01*\nM02*\n"
                .to_vec(),
        ),
        ("macro", fixture("s1a/macro_hole_over_line.gbr")),
    ] {
        let original = gerber_io::parse_s1(&bytes, "original").unwrap();
        let mut run = Run::new(name, &bytes);
        run.move_all("0");
        let output = run.roundtrip("1");
        for x in -8..=24 {
            for y in -8..=24 {
                let p = MmPoint::new(x as f64 * 0.25 + 0.019, y as f64 * 0.25 + 0.011);
                assert_eq!(
                    original
                        .document
                        .layer_coverage_at(&original.document.layers[0].id, p),
                    output.document.layer_coverage_at(
                        &output.document.layers[0].id,
                        MmPoint::new(p.x_mm + 5.0, p.y_mm - 3.0)
                    )
                );
            }
        }
    }
}
