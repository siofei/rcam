use std::{sync::atomic::AtomicU64, time::Instant};
#[test]
fn bounded_overhead_and_snap_rate_limit() {
    let dir = std::env::temp_dir().join(format!("rcam-infra-overhead-{}", std::process::id()));
    let guard = rcam_diagnostics::Runtime::start(dir.clone(), "test", "test").unwrap();
    assert!(guard.install_sink());
    let runtime = guard.runtime();
    let mut observations = Vec::new();
    for level in [
        rcam_diagnostics::Level::Off,
        rcam_diagnostics::Level::Info,
        rcam_diagnostics::Level::Debug,
    ] {
        runtime.set_level(level);
        let start = Instant::now();
        let last = AtomicU64::new(0);
        for _ in 0..10_000 {
            if level == rcam_diagnostics::Level::Debug {
                rcam_diagnostics::rate_limited(
                    &last,
                    level,
                    "snap.acquire",
                    &[("candidate_query_us", 12)],
                );
            } else {
                rcam_diagnostics::Operation::begin("objects.move", Some(1)).end(Some(2), None);
            }
        }
        observations.push(serde_json::json!({"level":level,"iterations":10000,"duration_us":start.elapsed().as_micros()}));
        // Wait for a queue barrier off the measured hot path; bounded queue may be full briefly.
        let started = Instant::now();
        while !runtime.flush() {
            assert!(started.elapsed().as_secs() < 5);
            std::thread::yield_now();
        }
    }
    for _ in 0..1000 {
        rcam_diagnostics::render_exception(
            rcam_diagnostics::RenderException::LastGoodFrameFallback,
        );
    }
    for kind in [
        rcam_diagnostics::RenderException::DisplayPrepareFailed,
        rcam_diagnostics::RenderException::ResourceLimit,
        rcam_diagnostics::RenderException::SurfaceError,
        rcam_diagnostics::RenderException::DeviceLost,
    ] {
        rcam_diagnostics::render_exception(kind);
    }
    assert!(runtime.flush());
    let events = std::fs::read_to_string(dir.join("rcam.log")).unwrap();
    let snap: Vec<_> = events
        .lines()
        .filter(|line| line.contains("snap.acquire"))
        .collect();
    assert_eq!(snap.len(), 1);
    for name in [
        "display_prepare_failed",
        "last_good_frame_fallback",
        "resource_limit",
        "surface_error",
        "device_lost",
    ] {
        assert_eq!(
            events
                .lines()
                .filter(|line| line.contains(&format!("render.{name}")))
                .count(),
            1
        );
    }
    assert!(snap[0].contains("Debug"));
    println!(
        "{}",
        serde_json::json!({"observations":observations,"ring_count":runtime.recent().len(),"note":"bounded synthetic pressure; no interaction SLA claimed"})
    );
    drop(guard);
    std::fs::remove_dir_all(dir).unwrap();
}
