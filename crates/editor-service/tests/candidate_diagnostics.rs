mod candidate_support;
use candidate_support::*;
#[test]
fn read_only_candidate_diagnostic_package_has_counts_timings_and_no_payload() {
    let r = setup(3);
    let guard =
        rcam_diagnostics::Runtime::start(r.dir.join("diagnostics"), "test", "test").unwrap();
    assert!(guard.install_sink());
    let runtime = guard.runtime();
    let before = r.info();
    let q = query(&r);
    let page = nearby(&r, &q).unwrap();
    assert_eq!(r.info(), before);
    let events = runtime.recent();
    runtime.export(&r.dir.join("diagnostics.zip")).unwrap();
    let archive = rcam_project::zip_codec::read_zip(
        &std::fs::read(r.dir.join("diagnostics.zip")).unwrap(),
        &rcam_project::zip_codec::ReadPolicy {
            max_entries: 100,
            max_entry_bytes: 30 * 1024 * 1024,
            max_uncompressed_bytes: 100 * 1024 * 1024,
            max_path_len: 512,
        },
    )
    .unwrap();
    let summary: serde_json::Value = serde_json::from_slice(
        &archive
            .iter()
            .find(|e| e.path == "performance_summary.json")
            .unwrap()
            .data,
    )
    .unwrap();
    let measurements: Vec<rcam_diagnostics::Event> =
        serde_json::from_value(summary["events"].clone()).unwrap();
    let counts = measurements
        .iter()
        .find(|e| e.command_id == "components.nearby.counts")
        .unwrap();
    assert_eq!(counts.metrics["candidate_count"], page.total as u64);
    assert_eq!(counts.metrics["layer_count"], 1);
    assert!(counts.metrics.contains_key("world_index_us"));
    assert!(counts.metrics.contains_key("classify_us"));
    assert!(counts.metrics.contains_key("sort_us"));
    assert!(counts.metrics.contains_key("query_us"));
    let end = events
        .iter()
        .find(|e| e.command_id == "components.nearby_manufacturing" && e.phase == "ok")
        .unwrap();
    assert_eq!(end.revision_before, end.revision_after);
    let text = serde_json::to_string(&events).unwrap();
    for secret in [
        "C15",
        "C16",
        "secret",
        "pnp.csv",
        &q.component_id,
        r.dir.to_str().unwrap(),
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
    for c in page.items {
        for id in c.member_object_ids {
            assert!(!text.contains(&id));
        }
    }
}
