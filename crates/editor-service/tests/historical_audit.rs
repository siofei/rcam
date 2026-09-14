// Migrated from the local S0-A review harness; all 35 assertions retained.
use editor_core::MmPoint;
use gerber_io::parse_s0;
#[test]
fn historical_35_checks() {
    let base = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,10*%\nD10*\nX0Y0D03*\nM02*\n";
    let mut failures = Vec::new();
    let mut checks = 0;
    let mut check = |name: &str, ok: bool| {
        checks += 1;
        println!("{} {name}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            failures.push(name.to_string());
        }
    };
    println!(
        "VALID_PARSE_RESULT {:?}",
        parse_s0(base.as_bytes(), "audit")
    );
    check(
        "valid 2.6 circle",
        parse_s0(base.as_bytes(), "audit").is_ok(),
    );
    let cases = [
        ("M99 rejected", base.replace("M02*", "M99*")),
        ("trailing command rejected", format!("{base}X1Y2D03*\n")),
        (
            "unknown extended command rejected",
            base.replace("D10*", "%ZZgarbage*%\nD10*"),
        ),
        (
            "duplicate X rejected",
            base.replace("X0Y0D03*", "X0X100Y0D03*"),
        ),
        (
            "reverse Y X rejected",
            base.replace("X0Y0D03*", "Y0X100D03*"),
        ),
        (
            "undefined first X rejected",
            base.replace("X0Y0D03*", "Y0D03*"),
        ),
        (
            "undefined first Y rejected",
            base.replace("X0Y0D03*", "X0D03*"),
        ),
        ("undefined both rejected", base.replace("X0Y0D03*", "D03*")),
        (
            "future aperture rejected",
            base.replace("%ADD10C,10*%\n", "")
                .replace("M02*", "%ADD10C,10*%\nM02*"),
        ),
        (
            "duplicate aperture rejected",
            base.replace("%ADD10C,10*%", "%ADD10C,10*%\n%ADD10C,5*%"),
        ),
        (
            "late unit rejected",
            base.replace("%MOMM*%\n", "")
                .replace("M02*", "%MOMM*%\nM02*"),
        ),
        (
            "duplicate MO rejected",
            base.replace("%MOMM*%", "%MOMM*%\n%MOMM*%"),
        ),
        (
            "duplicate FS rejected",
            base.replace("%MOMM*%", "%MOMM*%\n%FSLAX26Y26*%"),
        ),
        (
            "comment appended instruction rejected",
            base.replace("D10*", "G04 comment*G74*\nD10*"),
        ),
        (
            "infinite aperture rejected",
            base.replace("C,10*", "C,inf*"),
        ),
        (
            "exponent aperture rejected",
            base.replace("C,10*", "C,1e1*"),
        ),
        ("aperture below10 rejected", base.replace("D10", "D09")),
        (
            "unknown tail before end rejected",
            base.replace("M02*", "garbage*\nM02*"),
        ),
        (
            "oversized coordinate rejected",
            base.replace("X0Y0", "X99999999999999999999Y0"),
        ),
        (
            "mismatched format rejected",
            base.replace("X26Y26", "X26Y36"),
        ),
    ];
    for (name, input) in cases {
        check(name, parse_s0(input.as_bytes(), "audit").is_err());
    }
    let shift = base.replace("X0Y0D03*", "X1234567Y-2345678D03*");
    match parse_s0(shift.as_bytes(), "audit") {
        Ok(s) => {
            let layer = &s.document.layers[0];
            check(
                "independent shifted centre coverage",
                layer.coverage_at(MmPoint::new(1.234567, -2.345678))
                    && !layer.coverage_at(MmPoint::new(6.3, -2.345678)),
            );
        }
        Err(_) => check("independent shifted centre coverage", false),
    }
    let inch = base
        .replace("%MOMM*%", "%MOIN*%")
        .replace("C,10*", "C,1*")
        .replace("X0Y0D03*", "X1000000Y0D03*");
    match parse_s0(inch.as_bytes(), "audit") {
        Ok(s) => {
            let layer = &s.document.layers[0];
            check(
                "inch conversion centre and radius",
                layer.coverage_at(MmPoint::new(25.4, 0.0))
                    && layer.coverage_at(MmPoint::new(38.0, 0.0))
                    && !layer.coverage_at(MmPoint::new(38.2, 0.0)),
            );
        }
        Err(_) => check("inch conversion centre and radius", false),
    }
    let modal = base.replace("X0Y0D03*", "X1000000Y2000000D03*\nX20000000D03*");
    match parse_s0(modal.as_bytes(), "audit") {
        Ok(s) => check(
            "modal Y retained",
            s.document.layers[0].coverage_at(MmPoint::new(20.0, 2.0))
                && !s.document.layers[0].coverage_at(MmPoint::new(20.0, -4.0)),
        ),
        Err(_) => check("modal Y retained", false),
    }
    let mut svc = editor_service::ApplicationService::new();
    let a = svc.open_s0("stable", base.as_bytes());
    let original = svc.snapshot("stable").ok();
    check(
        "import snapshot contains only source geometry",
        original
            .as_ref()
            .is_some_and(|d| d.layers.len() == 1 && d.layers[0].objects.len() == 1),
    );
    let b = svc.open_s0("stable", shift.as_bytes());
    check(
        "duplicate document leaves snapshot unchanged",
        a.is_ok() && b.is_err() && original == svc.snapshot("stable").ok(),
    );
    let failed = svc.open_s0("bad", b"M99*\n");
    check(
        "invalid import leaves prior document unchanged",
        failed.is_err() && original == svc.snapshot("stable").ok(),
    );

    let requests = [
        (
            "unknown API version",
            r#"{"api_version":2,"request_id":"a","op":"system.capabilities","params":{}}"#,
            "UNSUPPORTED_API_VERSION",
        ),
        (
            "unknown operation",
            r#"{"api_version":1,"request_id":"a","op":"objects.move","params":{}}"#,
            "UNSUPPORTED_OPERATION",
        ),
        (
            "unknown envelope field",
            r#"{"api_version":1,"request_id":"a","op":"system.capabilities","trusted":true,"params":{}}"#,
            "INVALID_ARGUMENT",
        ),
        (
            "unknown parameter",
            r#"{"api_version":1,"request_id":"a","op":"system.capabilities","params":{"force":true}}"#,
            "INVALID_ARGUMENT",
        ),
        (
            "blank request id",
            r#"{"api_version":1,"request_id":" ","op":"system.capabilities","params":{}}"#,
            "INVALID_ARGUMENT",
        ),
        ("malformed JSON", "{broken", "INVALID_ARGUMENT"),
        (
            "unknown snapshot document",
            r#"{"api_version":1,"request_id":"a","op":"document.snapshot","document_id":"missing","params":{}}"#,
            "NOT_FOUND",
        ),
    ];
    for (name, raw, code) in requests {
        check(name, svc.execute_json(raw)["error"]["code"] == code);
    }
    let request = r#"{"api_version":1,"request_id":"audit-correlate","op":"system.capabilities","params":{}}"#;
    check("capability request id roundtrip", {
        let r = svc.execute_json(request);
        r["request_id"] == "audit-correlate" && r["status"] == "completed"
    });
    println!("SUMMARY checks={checks} failures={}", failures.len());
    assert_eq!(checks, 35);
    assert!(failures.is_empty(), "{failures:?}");
}
