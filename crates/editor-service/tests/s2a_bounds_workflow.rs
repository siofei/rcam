//! S2-A.1 bounds: real JSON/file workflow, independent numeric expectations.
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
        let base = std::env::var_os("RCAM_S2A_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!(
            "bounds-{}-{}",
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
        let mut request = json!({"api_version":1,"request_id":format!("bounds-{}",self.calls.len()),"op":op,"params":params});
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
    fn bounds(&mut self) -> Value {
        self.ok("document.bounds", None, json!({}))["bounds"].clone()
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
        if std::env::var_os("RCAM_S2A_EVIDENCE").is_some() {
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
fn bounds_workspace_visibility_does_not_change_manufacturing_bounds() {
    let mut r = Run::new();
    let before = r.info();
    let expected = json!({"min_x_mm":1.0,"min_y_mm":2.0,"max_x_mm":3.0,"max_y_mm":4.0});
    assert_eq!(r.bounds(), expected);
    assert_eq!(
        r.info(),
        before,
        "read-only bounds must not touch any document state"
    );
    r.update(json!({"visible":false,"locked":true,"display_name":"hidden"}));
    let after = r.info();
    assert_eq!(r.bounds(), expected);
    let b = r.ok("layer.bounds", None, json!({"layer_id":r.layer}));
    assert_eq!(b["bounds"], expected);
    assert_eq!(b["revision"], "0");
    assert_eq!(r.info(), after);
    assert_eq!(after["dirty"], false);
}

#[test]
fn bounds_after_move_undo_redo_export_reopen_and_source_preservation() {
    let mut r = Run::new();
    let source = std::fs::read(r.dir.join("source.gbr")).unwrap();
    let initial = r.bounds();
    r.move_one();
    let moved = json!({"min_x_mm":6.0,"min_y_mm":1.0,"max_x_mm":8.0,"max_y_mm":3.0});
    assert_eq!(r.bounds(), moved);
    r.ok("history.undo", Some("1"), json!({}));
    assert_eq!(r.bounds(), initial);
    r.update(json!({"locked":true}));
    r.ok("history.redo", Some("2"), json!({}));
    assert_eq!(r.bounds(), moved);
    let response = r.call("document.bounds", None, json!({}));
    assert_eq!(response["revision"], "3");
    assert_eq!(response["result"]["revision"], "3");
    r.export("saved.gbr");
    let reopened = r.ok("document.open", None, json!({"path":"saved.gbr"}));
    let result = r
        .service
        .document_bounds(reopened["document_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(serde_json::to_value(result.bounds).unwrap(), moved);
    assert_eq!(std::fs::read(r.dir.join("source.gbr")).unwrap(), source);
}

#[test]
fn delete_all_bounds_are_null_undo_restores() {
    let mut r = Run::new();
    let initial = r.bounds();
    r.ok(
        "objects.delete",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":[r.object]}),
    );
    assert!(r.bounds().is_null());
    assert!(r.ok("layer.bounds", None, json!({"layer_id":r.layer}))["bounds"].is_null());
    r.ok("history.undo", Some("1"), json!({}));
    assert_eq!(r.bounds(), initial);
}

#[test]
fn strict_bounds_contract_and_failures_leave_state_unchanged() {
    let mut r = Run::new();
    let before = r.info();
    for (op, rev, params, code) in [
        (
            "layer.bounds",
            None,
            json!({"layer_id":""}),
            "INVALID_ARGUMENT",
        ),
        (
            "layer.bounds",
            None,
            json!({"layer_id":"missing"}),
            "NOT_FOUND",
        ),
        ("layer.bounds", None, json!({}), "INVALID_ARGUMENT"),
        (
            "layer.bounds",
            None,
            json!({"layer_id":r.layer,"visible":true}),
            "INVALID_ARGUMENT",
        ),
        (
            "document.bounds",
            None,
            json!({"layer_id":r.layer}),
            "INVALID_ARGUMENT",
        ),
        ("document.bounds", Some("0"), json!({}), "INVALID_ARGUMENT"),
        (
            "layer.bounds",
            Some("0"),
            json!({"layer_id":r.layer}),
            "INVALID_ARGUMENT",
        ),
    ] {
        let response = r.call(op, rev, params);
        assert_eq!(response["error"]["code"], code, "{response}");
        if code == "NOT_FOUND" {
            assert_eq!(response["error"]["details"]["entity"], "layer");
        }
        assert_eq!(r.info(), before);
    }
    let id = r.id.clone();
    r.id = "absent".into();
    let response = r.call("document.bounds", None, json!({}));
    assert_eq!(response["error"]["code"], "NOT_FOUND");
    assert_eq!(response["error"]["details"]["entity"], "document");
    r.id = id;
    assert_eq!(r.info(), before);
}

#[test]
fn bounds_dto_roundtrip_preserves_f64_and_large_revision() {
    use editor_core::BoundsMm;
    use editor_service::BoundsResult;
    let b = BoundsResult {
        document_id: "文档 #1".into(),
        revision: "9007199254740993".into(),
        bounds: Some(BoundsMm {
            min_x_mm: -123.123456789,
            min_y_mm: 0.000000123456789,
            max_x_mm: 123.987654321,
            max_y_mm: 456.123456789,
        }),
    };
    let decoded: BoundsResult = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
    assert_eq!(decoded, b);
}

#[test]
fn bounds_capabilities_match_handlers_and_close_invalidates_document() {
    let mut r = Run::new();
    let ops = r.ok("system.capabilities", None, json!({}))["supported_operations"].clone();
    for op in ["document.bounds", "layer.bounds"] {
        assert!(ops.as_array().unwrap().contains(&json!(op)));
    }
    assert!(
        !ApplicationService::new()
            .capabilities()
            .supported_operations
            .contains(&"document.bounds".into())
    );
    assert_eq!(
        r.call(
            "document.close",
            Some("0"),
            json!({"discard_changes":false})
        )["status"],
        "confirmation_required"
    );
    r.ok("document.close", Some("0"), json!({"discard_changes":true}));
    assert_eq!(
        r.call("document.bounds", None, json!({}))["error"]["code"],
        "NOT_FOUND"
    );
}

#[test]
fn layer_bounds_real_parser_flash_line_arc_region_and_cutin() {
    let mut r = Run::new();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
    for (i, (file, expected)) in [
        ("s0c/am_1.gbr", [-1., -1., 1., 1.]),
        ("s0c/rectangular_draw.gbr", [-1., -0.5, 4., 2.5]),
        ("s0c/region.gbr", [0., 0., 2., 1.]),
        ("s1a/region_cutin.gbr", [0., 0., 4., 4.]),
        ("s1a1/g75_exact.gbr", [-0.1, -0.1, 1.1, 1.1]),
        ("s1a1/g75_large_deviation.gbr", [-0.1, -0.1, 1.6, 2.1]),
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("fixture-{i}.gbr");
        std::fs::copy(fixtures.join(file), r.dir.join(&name)).unwrap();
        let info = r.ok("document.open", None, json!({"path":name}));
        r.id = info["document_id"].as_str().unwrap().into();
        r.layer = info["layer_ids"][0].as_str().unwrap().into();
        let b = r.ok("layer.bounds", None, json!({"layer_id":r.layer}))["bounds"].clone();
        for (key, value) in ["min_x_mm", "min_y_mm", "max_x_mm", "max_y_mm"]
            .into_iter()
            .zip(expected)
        {
            assert!(
                (b[key].as_f64().unwrap() - value).abs() < 1e-9,
                "{file}: {b}"
            );
        }
        r.calls
            .push(json!({"fixture":file,"expected_bounds":expected}));
    }
}
