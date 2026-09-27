mod array_support;
use array_support::Run;
#[test]
fn array_diagnostics_counts_sign_source_noop_failure_and_privacy() {
    let mut r = Run::new(3);
    let guard =
        rcam_diagnostics::Runtime::start(r.dir.join("diagnostics"), "test", "test").unwrap();
    assert!(guard.install_sink());
    let runtime = guard.runtime();
    let mut p = r.params(3, 4);
    p.pitch_x_mm = -10.;
    rcam_diagnostics::with_source(rcam_diagnostics::Source::Context, || r.array(p.clone()))
        .unwrap();
    p.rows = 1;
    p.columns = 1;
    r.array(p.clone()).unwrap();
    p.rows = 2;
    p.pitch_y_mm = 0.;
    assert!(r.array(p).is_err());
    let events = runtime.recent();
    let ends: Vec<_> = events
        .iter()
        .filter(|e| {
            e.command_id == "objects.array_rectangular" && (e.phase == "ok" || e.phase == "error")
        })
        .collect();
    assert_eq!(ends.len(), 3);
    let e = ends[0];
    assert_eq!(e.metrics["rows"], 3);
    assert_eq!(e.metrics["columns"], 4);
    assert_eq!(e.metrics["cell_count"], 12);
    assert_eq!(e.metrics["source_object_count"], 3);
    assert_eq!(e.metrics["created_object_count"], 33);
    assert_eq!(e.metrics["pitch_x_sign"], 2);
    assert_eq!(e.metrics["pitch_y_sign"], 1);
    assert!(matches!(e.source, rcam_diagnostics::Source::Context));
    assert_eq!((e.revision_before, e.revision_after), (Some(0), Some(1)));
    assert_eq!(ends[1].revision_before, ends[1].revision_after);
    assert_eq!(ends[1].metrics["created_object_count"], 0);
    assert_eq!(ends[2].phase, "error");
    assert_eq!(ends[2].revision_before, ends[2].revision_after);
    let path = r.dir.join("diagnostics.zip");
    runtime.export(&path).unwrap();
    assert!(path.is_file());
    for e in ends {
        let text = serde_json::to_string(e).unwrap();
        assert!(!text.contains("source.gbr"));
        assert!(!text.contains(r.dir.to_str().unwrap()));
        assert!(!text.contains("pitch_x_mm"));
        for id in r.ids() {
            assert!(!text.contains(&id));
        }
    }
}
