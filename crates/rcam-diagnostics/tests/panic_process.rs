#[test]
fn panic_child() {
    let Some(dir) = std::env::var_os("RCAM_DIAGNOSTIC_PANIC_FIXTURE") else {
        return;
    };
    let guard = rcam_diagnostics::Runtime::start(dir.into(), "test", "test").unwrap();
    assert!(guard.install());
    let _operation = rcam_diagnostics::Operation::begin("test.pending", Some(7));
    panic!("/Users/test/客户A/机密/top.gbr private user text");
}
#[test]
fn panic_is_reported_in_a_child_without_changing_parent_hook() {
    let dir = std::env::temp_dir().join(format!("rcam-panic-parent-{}", std::process::id()));
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "panic_child"])
        .env("RCAM_DIAGNOSTIC_PANIC_FIXTURE", &dir)
        .status()
        .unwrap();
    assert!(!status.success());
    let files: Vec<_> = std::fs::read_dir(dir.join("crashes")).unwrap().collect();
    assert_eq!(files.len(), 1);
    let report = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
    assert!(report.contains("test.pending"));
    assert!(report.contains("begin"));
    assert!(!report.contains("/Users/"));
    assert!(!report.contains("private user text"));
    std::fs::remove_dir_all(dir).unwrap();
}
