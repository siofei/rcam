#[test]
fn semantic_cancel_is_all_or_error_and_never_compatibility_fallback() {
    let source = include_bytes!("../../../fixtures/synthetic/s5m1/P10K_CROP.gbr");
    for compatibility in [false, true] {
        for stop in [1, 4, 64, 1024] {
            let mut checks = 0;
            let error = gerber_io::parse_s1_cancellable(source, "cancel", compatibility, || {
                checks += 1;
                checks == stop
            })
            .unwrap_err();
            assert_eq!(checks, stop);
            assert_eq!(error, gerber_io::S1Error::Cancelled);
            assert!(!error.allows_compatibility_fallback());
        }
        let expected = if compatibility {
            gerber_io::parse_s1_compat(source, "same")
        } else {
            gerber_io::parse_s1(source, "same")
        }
        .unwrap();
        let actual =
            gerber_io::parse_s1_cancellable(source, "same", compatibility, || false).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn cancellation_during_final_validation_does_not_return_a_scene() {
    let source = include_bytes!("../../../fixtures/synthetic/s5m1/P10K_CROP.gbr");
    let mut total = 0;
    gerber_io::parse_s1_cancellable(source, "validate", false, || {
        total += 1;
        false
    })
    .unwrap();
    assert!(total > 10000);
    let mut count = 0;
    let error = gerber_io::parse_s1_cancellable(source, "validate", false, || {
        count += 1;
        count == total - 10
    })
    .unwrap_err();
    assert_eq!(count, total - 10);
    assert_eq!(error, gerber_io::S1Error::Cancelled);
}
