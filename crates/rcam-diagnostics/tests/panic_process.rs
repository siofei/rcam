#[test]
fn panic_child() {
    let Some(dir) = std::env::var_os("RCAM_DIAGNOSTIC_PANIC_FIXTURE") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let guard = rcam_diagnostics::Runtime::start(dir.clone(), "test", "test").unwrap();
    // Count calls while retaining the test process's previous/default Rust hook.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("RCAM_PREVIOUS_HOOK_CALLED");
        previous(info);
    }));
    assert!(guard.install());
    assert!(!guard.install());
    if std::env::var_os("RCAM_CRASH_DIRECTORY_UNAVAILABLE").is_some() {
        std::fs::remove_dir(dir.join("crashes")).unwrap();
    }
    let _operation = rcam_diagnostics::Operation::begin("test.pending", Some(7));
    panic!("/Users/test/客户A/机密/top.gbr private user text");
}
#[test]
fn panic_is_reported_in_a_child_without_changing_parent_hook() {
    for unavailable in [false, true] {
        let dir = std::env::temp_dir().join(format!(
            "rcam-panic-parent-{}-{unavailable}",
            std::process::id()
        ));
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "panic_child", "--nocapture"])
            .env("RCAM_DIAGNOSTIC_PANIC_FIXTURE", &dir)
            .env("RUST_BACKTRACE", "1")
            .env_remove("RCAM_CRASH_DIRECTORY_UNAVAILABLE");
        if unavailable {
            command.env("RCAM_CRASH_DIRECTORY_UNAVAILABLE", "1");
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("panicked at"));
        assert_eq!(stderr.matches("RCAM_PREVIOUS_HOOK_CALLED").count(), 1);
        if unavailable {
            assert!(!dir.join("crashes").exists());
        } else {
            let files: Vec<_> = std::fs::read_dir(dir.join("crashes")).unwrap().collect();
            assert_eq!(files.len(), 1);
            let report = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
            let value: serde_json::Value = serde_json::from_str(&report).unwrap();
            assert!(
                value["recent_operations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|event| {
                        event["command_id"] == "test.pending" && event["phase"] == "begin"
                    })
            );
            assert!(!report.contains("/Users/"));
            assert!(!report.contains("private user text"));
        }
        // Only boolean observations are public: default stderr may contain private payloads.
        println!(
            "panic-child-output: {}",
            serde_json::json!({
                "crash_directory_unavailable": unavailable,
                "exit_nonzero": true,
                "previous_hook_calls": 1,
                "stderr_default_hook_seen": true,
                "backtrace_seen": stderr.contains("stack backtrace"),
                "crash_report_count": if unavailable { 0 } else { 1 },
                "second_install_rejected": true,
                "crash_json_privacy_checked": !unavailable,
                "pending_begin_verified": !unavailable,
            })
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
