//! S1-B2c public JSON workspace/manufacturing boundary regression.
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
        let base = std::env::var_os("RCAM_S1B2C_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!(
            "workspace-{}-{}",
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
        let mut request = json!({"api_version":1,"request_id":format!("workspace-{}",self.calls.len()),"op":op,"params":params});
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
        if std::env::var_os("RCAM_S1B2C_EVIDENCE").is_some() {
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
fn workspace_clean(patch: Value) {
    let mut r = Run::new();
    let before = r.info();
    let objects = r.objects();
    let after = r.update(patch);
    for field in [
        "revision",
        "dirty",
        "undo_entries",
        "redo_entries",
        "last_saved_path",
        "source_sha256",
    ] {
        assert_eq!(after[field], before[field]);
    }
    assert_eq!(after["workspace_revision"], "1");
    assert_eq!(r.objects(), objects);
}
#[test]
fn workspace_lock_does_not_change_manufacturing_dirty() {
    workspace_clean(json!({"locked":true}));
}
#[test]
fn workspace_visibility_does_not_change_manufacturing_dirty() {
    workspace_clean(json!({"visible":false}));
}
#[test]
fn workspace_rename_does_not_change_manufacturing_dirty() {
    workspace_clean(json!({"display_name":"钢网 # 顶层"}));
}
#[test]
fn workspace_update_increments_only_workspace_revision() {
    workspace_clean(json!({"visible":false,"locked":true,"display_name":"new"}));
}
#[test]
fn workspace_noop_does_not_increment_revision() {
    let mut r = Run::new();
    let before = r.info();
    assert_eq!(r.update(json!({})), before);
    assert_eq!(
        r.update(json!({"visible":true,"locked":false,"display_name":"source"})),
        before
    );
}
fn locked_edits(ops: &[&str]) {
    let mut r = Run::new();
    r.move_one();
    r.ok("history.undo", Some("1"), json!({}));
    r.update(json!({"locked":true}));
    let before = r.info();
    let objects = r.objects();
    for op in ops {
        let mut p = json!({"layer_id":r.layer,"object_ids":[r.object]});
        match *op {
            "objects.move" | "objects.duplicate" => {
                p["dx_mm"] = json!(1);
                p["dy_mm"] = json!(0);
            }
            "objects.rotate" => {
                p["angle_deg"] = json!(37);
                p["pivot_mm"] = json!({"x_mm":0,"y_mm":0});
            }
            "objects.mirror" => {
                p["axis"] = json!({"kind":"horizontal","coordinate_mm":0});
            }
            _ => (),
        }
        assert_eq!(
            r.call(op, Some("2"), p)["error"]["code"],
            "LAYER_LOCKED",
            "{op}"
        );
        assert_eq!(r.info(), before);
        assert_eq!(r.objects(), objects);
    }
    r.ok("history.redo", Some("2"), json!({}));
    assert_eq!(r.info()["dirty"], true);
}
#[test]
fn locked_layer_rejects_new_move_atomically() {
    locked_edits(&["objects.move"]);
}
#[test]
fn locked_layer_rejects_duplicate_delete_rotate_mirror() {
    locked_edits(&[
        "objects.duplicate",
        "objects.delete",
        "objects.rotate",
        "objects.mirror",
    ]);
}
#[test]
fn lock_after_move_does_not_block_undo() {
    let mut r = Run::new();
    let original = r.objects();
    r.move_one();
    r.update(json!({"locked":true}));
    r.ok("history.undo", Some("1"), json!({}));
    assert_eq!(r.objects(), original);
    assert_eq!(r.info()["dirty"], false);
}
#[test]
fn lock_after_undo_does_not_block_redo() {
    let mut r = Run::new();
    r.move_one();
    let moved = r.objects();
    r.ok("history.undo", Some("1"), json!({}));
    r.update(json!({"locked":true}));
    r.ok("history.redo", Some("2"), json!({}));
    assert_eq!(r.objects(), moved);
    assert_eq!(r.info()["revision"], "3");
}
#[test]
fn workspace_state_not_written_to_export() {
    let mut r = Run::new();
    let source = std::fs::read(r.dir.join("source.gbr")).unwrap();
    let before = r.export("before.gbr");
    r.update(json!({"visible":false,"locked":true,"display_name":"Workspace-only"}));
    let after = r.export("after.gbr");
    assert_eq!(before, after);
    let reopened = r.ok("document.open", None, json!({"path":"after.gbr"}));
    let id = reopened["document_id"].as_str().unwrap();
    let layers = r.service.layers_list(id).unwrap();
    assert!(layers[0].visible);
    assert!(!layers[0].locked);
    assert_ne!(layers[0].display_name, "Workspace-only");
    let scene = gerber_io::parse_s1(&after, "truth").unwrap();
    if let editor_core::SemanticGeometry::Flash { center, .. } =
        scene.document.layers[0].objects[0].geometry
    {
        assert_eq!(center, editor_core::MmPoint::new(2., 3.));
    } else {
        panic!("expected flash");
    }
    assert_eq!(source, std::fs::read(r.dir.join("source.gbr")).unwrap());
}
#[test]
fn persisted_workspace_change_requires_project_close_confirmation() {
    let mut r = Run::new();
    r.update(json!({"locked":true,"visible":false,"display_name":"x"}));
    assert_eq!(r.info()["project_dirty"], true);
    assert_eq!(
        r.call(
            "document.close",
            Some("0"),
            json!({"discard_changes":false})
        )["status"],
        "confirmation_required"
    );
    r.ok("document.close", Some("0"), json!({"discard_changes":true}));
}
#[test]
fn manufacturing_edit_still_changes_dirty() {
    let mut r = Run::new();
    r.update(json!({"visible":false}));
    r.move_one();
    assert_eq!(r.info()["dirty"], true);
    assert_eq!(
        r.call(
            "document.close",
            Some("1"),
            json!({"discard_changes":false})
        )["status"],
        "confirmation_required"
    );
}
#[test]
fn export_never_clears_project_dirty_or_links_the_output() {
    let mut r = Run::new();
    r.move_one();
    r.update(json!({"locked":true}));
    r.export("saved.gbr");
    let info = r.info();
    assert_eq!(info["dirty"], true, "Export is a copy, not a Save");
    assert_eq!(info["revision"], "1");
    assert_eq!(info["workspace_revision"], "1");
    assert_eq!(info["last_saved_path"], Value::Null);
}
#[test]
fn layer_update_unknown_layer_is_typed_not_found() {
    let mut r = Run::new();
    let before = r.info();
    let e = r.call(
        "layer.update",
        Some("0"),
        json!({"layer_id":"missing","expected_workspace_revision":"0","locked":true}),
    );
    assert_eq!(e["error"]["code"], "NOT_FOUND");
    assert_eq!(
        e["error"]["details"],
        json!({"entity":"layer","id":"missing"})
    );
    assert_eq!(r.info(), before);
}
#[test]
fn layer_update_strict_json_fields() {
    let mut r = Run::new();
    let before = r.info();
    for patch in [
        json!({"objects":[]}),
        json!({"locked":"true"}),
        json!({"display_name":""}),
        json!({"display_name":"字".repeat(342)}),
        json!({"expected_workspace_revision":4}),
    ] {
        let mut p = json!({"layer_id":r.layer,"expected_workspace_revision":"0"});
        for (k, v) in patch.as_object().unwrap() {
            p[k] = v.clone();
        }
        assert_eq!(
            r.call("layer.update", Some("0"), p)["error"]["code"],
            "INVALID_ARGUMENT"
        );
        assert_eq!(r.info(), before);
    }
    assert_eq!(
        r.call(
            "layer.update",
            Some("0"),
            json!({"layer_id":"","expected_workspace_revision":"0"})
        )["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        r.call(
            "layer.update",
            None,
            json!({"layer_id":r.layer,"expected_workspace_revision":"0"})
        )["error"]["code"],
        "INVALID_ARGUMENT"
    );
}
#[test]
fn workspace_stale_requests_preserve_state_and_history() {
    let mut r = Run::new();
    r.move_one();
    r.update(json!({"visible":false}));
    let before = r.info();
    for (rev, wr) in [("0", "1"), ("1", "0")] {
        assert_eq!(
            r.call(
                "layer.update",
                Some(rev),
                json!({"layer_id":r.layer,"expected_workspace_revision":wr,"locked":true})
            )["error"]["code"],
            "REVISION_CONFLICT"
        );
        assert_eq!(r.info(), before);
    }
    assert!(!r.service.layers_list(&r.id).unwrap()[0].locked);
}
#[test]
fn workspace_capability_and_dto_roundtrip() {
    let mut r = Run::new();
    assert!(
        r.service
            .capabilities()
            .supported_operations
            .contains(&"layer.update".into())
    );
    let p = editor_service::LayerUpdateParams {
        layer_id: r.layer.clone(),
        expected_workspace_revision: "9007199254740993".into(),
        display_name: Some("钢网".into()),
        visible: Some(false),
        locked: None,
        ..Default::default()
    };
    assert_eq!(
        serde_json::from_str::<editor_service::LayerUpdateParams>(
            &serde_json::to_string(&p).unwrap()
        )
        .unwrap(),
        p
    );
    let info = r.update(json!({"display_name":"顶层","visible":false,"locked":true}));
    assert_eq!(info["workspace_revision"], "1");
    let layer = r.service.layers_list(&r.id).unwrap().remove(0);
    assert_eq!(layer.display_name, "顶层");
    assert!(!layer.visible);
    assert!(layer.locked);
    assert_eq!(layer.object_count, 1);
}
