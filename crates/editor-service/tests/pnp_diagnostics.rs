mod array_support;
use array_support::Run;
use editor_core::pnp::*;
use editor_service::*;
use serde_json::json;
#[test]
fn private_pnp_payload_and_paths_are_absent_from_success_failure_diagnostics() {
    let mut r = Run::new(1);
    let guard = rcam_diagnostics::Runtime::start(r.dir.join("logs"), "test", "test").unwrap();
    assert!(guard.install_sink());
    let runtime = guard.runtime();
    let mapping:PnpMapping=serde_json::from_value(json!({"delimiter":"csv","unit":"mm","refdes":0,"x":1,"y":2,"rotation":3,"side":4,"footprint":5,"value":6,"top_token":"Top","bottom_token":"Bottom","clockwise":false,"rotation_offset_deg":0,"invert_y":false})).unwrap();
    let secret = "PRIVATE_COMPONENT_PAYLOAD";
    let input = format!("R,X,Y,A,S,F,V\n{secret},1,2,37,Top,PRIVATE_FOOTPRINT,PRIVATE_VALUE\n");
    std::fs::write(r.dir.join("private.csv"), &input).unwrap();
    let preview = r
        .service
        .components_preview_pnp("private.csv", &mapping)
        .unwrap();
    r.service
        .components_import_pnp(
            &r.document,
            &r.info().revision,
            ImportPnpParams {
                path: "private.csv".into(),
                mapping: mapping.clone(),
                preview_sha256: preview.sha256,
                allow_replace: false,
            },
        )
        .unwrap();
    let input = input.replace(",1,2,", ",NaN,2,");
    std::fs::write(r.dir.join("private.csv"), input).unwrap();
    let preview = r
        .service
        .components_preview_pnp("private.csv", &mapping)
        .unwrap();
    assert!(
        r.service
            .components_import_pnp(
                &r.document,
                &r.info().revision,
                ImportPnpParams {
                    path: "private.csv".into(),
                    mapping,
                    preview_sha256: preview.sha256,
                    allow_replace: true
                }
            )
            .is_err()
    );
    let events = runtime.recent();
    let ops: Vec<_> = events
        .iter()
        .filter(|e| {
            e.command_id == "components.import_pnp" && ["ok", "error"].contains(&e.phase.as_str())
        })
        .collect();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[1].revision_before, ops[1].revision_after);
    let path = r.dir.join("diagnostics.zip");
    runtime.export(&path).unwrap();
    runtime.flush();
    let zip = std::fs::read(path).unwrap();
    let files = rcam_project::zip_codec::read_zip(
        &zip,
        &rcam_project::zip_codec::ReadPolicy {
            max_entries: 100,
            max_uncompressed_bytes: 100_000_000,
            max_entry_bytes: 20_000_000,
            max_path_len: 512,
        },
    )
    .unwrap();
    assert!(
        files
            .iter()
            .filter(|f| f.path.ends_with(".json") || f.path.ends_with(".log"))
            .any(|f| {
                String::from_utf8_lossy(&f.data).lines().any(|line| {
                    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|v| {
                        v["command_id"] == "components.import.diagnostics"
                            && v["metrics"]["invalid_number"] == 1
                    })
                })
            })
    );
    for f in files {
        let text = String::from_utf8_lossy(&f.data);
        for forbidden in [
            secret,
            "PRIVATE_FOOTPRINT",
            "PRIVATE_VALUE",
            "private.csv",
            r.dir.to_str().unwrap(),
            "NaN",
        ] {
            assert!(!text.contains(forbidden), "{} leaked {forbidden}", f.path);
        }
    }
}
