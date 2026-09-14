use editor_service::ApplicationService;
use serde_json::{Value, json};
fn response(service: &mut ApplicationService, request: &str) -> Value {
    service.execute_json(request)
}

#[test]
fn resource_limits_report_kind_limit_actual_and_preserve_document() {
    let mut service = ApplicationService::new();
    service
        .open_s0(
            "good",
            include_bytes!("../../../fixtures/synthetic/s0_polarity.gbr"),
        )
        .unwrap();
    let before = service.snapshot("good").unwrap();
    for (source, resource, limit, actual) in [
        (
            " ".repeat(2 * 1024 * 1024 + 1),
            "source_bytes",
            2 * 1024 * 1024,
            2 * 1024 * 1024 + 1,
        ),
        (
            format!(
                "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1*%\nD10*\n{}M02*\n",
                "X0Y0D03*\n".repeat(100001)
            ),
            "objects",
            100000,
            100001,
        ),
    ] {
        let error = service.open_s0("bad", source.as_bytes()).unwrap_err();
        assert_eq!(error.code, "RESOURCE_LIMIT");
        let error = serde_json::to_value(error).unwrap();
        assert_eq!(
            error["details"],
            json!({"resource":resource, "limit":limit, "actual":actual})
        );
        assert_eq!(before, service.snapshot("good").unwrap());
        assert!(service.snapshot("bad").is_err());
    }
    let invalid = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1X2*%\nD10*\nX0Y0D03*\nM02*\n";
    assert_ne!(
        service.open_s0("bad", invalid.as_bytes()).unwrap_err().code,
        "RESOURCE_LIMIT"
    );
}

#[test]
fn failures_echo_request_id_in_a_uniform_response() {
    let mut service = ApplicationService::new();
    for (op, version, params, code) in [
        ("objects.move", 1, json!({}), "UNSUPPORTED_OPERATION"),
        (
            "system.capabilities",
            2,
            json!({}),
            "UNSUPPORTED_API_VERSION",
        ),
        (
            "system.capabilities",
            1,
            json!({"unknown":true}),
            "INVALID_ARGUMENT",
        ),
        ("document.snapshot", 1, json!({}), "NOT_FOUND"),
    ] {
        let request = json!({"api_version":version,"request_id":"correlate","op":op,"params":params,"document_id":"missing"});
        let result = response(&mut service, &request.to_string());
        assert_eq!(result["request_id"], "correlate");
        assert_eq!(result["status"], "error");
        assert_eq!(result["error"]["code"], code);
        assert!(result["error"]["details"].is_object());
        for field in [
            "api_version",
            "document_id",
            "revision",
            "result",
            "warnings",
            "error",
            "job_id",
        ] {
            assert!(result.get(field).is_some(), "{field}");
        }
    }
}

#[test]
fn malformed_json_has_null_id_and_decode_location() {
    let result = response(&mut ApplicationService::new(), "{broken");
    assert!(result.get("request_id").unwrap().is_null());
    assert_eq!(result["error"]["code"], "INVALID_ARGUMENT");
    assert!(result["error"]["details"]["line"].as_u64().unwrap() > 0);
}

#[test]
fn capabilities_publish_actual_budgets_only_four_read_only_operations() {
    let caps = serde_json::to_value(ApplicationService::new().capabilities()).unwrap();
    assert_eq!(caps["supported_operations"].as_array().unwrap().len(), 4);
    assert_eq!(caps["read_only"], true);
    assert_eq!(caps["resource_limits"]["max_source_bytes"], 2 * 1024 * 1024);
    assert_eq!(caps["resource_limits"]["max_objects"], 100000);
}

#[test]
fn strict_envelope_types_fields_and_ambiguous_ids_are_rejected() {
    let mut service = ApplicationService::new();
    for extra in [
        json!({"trusted":true}),
        json!({"expected_revision":"0"}),
        json!({"document_id":"irrelevant"}),
    ] {
        let mut request =
            json!({"api_version":1,"request_id":"strict","op":"system.capabilities","params":{}});
        request
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = service.execute_json(&request.to_string());
        assert_eq!(result["error"]["code"], "INVALID_ARGUMENT");
        assert_eq!(result["request_id"], "strict");
    }
    for raw in [
        r#"{"api_version":1,"request_id":"a","request_id":"b","op":"system.capabilities","params":{}}"#,
        r#"{"api_version":1,"request_id":42,"op":"system.capabilities","params":{}}"#,
    ] {
        let result = service.execute_json(raw);
        assert_eq!(result["error"]["code"], "INVALID_ARGUMENT");
        assert!(result["request_id"].is_null());
    }
    let caps = service.execute_json(
        r#"{"api_version":1,"request_id":"cap","op":"system.capabilities","params":{}}"#,
    );
    assert!(caps["revision"].is_null());
    assert!(caps["error"].is_null());
}

#[test]
fn exact_resource_boundaries_remain_usable() {
    let base = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    let mut bytes = base.as_bytes().to_vec();
    bytes.resize(gerber_io::MAX_SOURCE_BYTES, b' ');
    let mut service = ApplicationService::new();
    service.open_s0("bytes-at-limit", &bytes).unwrap();
    let source = base.replace(
        "M02*",
        &format!("{}M02*", "D03*\n".repeat(gerber_io::MAX_OBJECTS - 1)),
    );
    service
        .open_s0("objects-at-limit", source.as_bytes())
        .unwrap();
    assert_eq!(
        service.snapshot("objects-at-limit").unwrap().layers[0]
            .objects
            .len(),
        gerber_io::MAX_OBJECTS
    );
    let too_many = source.replace("M02*", "D03*\nM02*");
    let request = json!({"api_version":1,"request_id":"budget","op":"document.open_s0","document_id":"over-limit","params":{"source":too_many}});
    let result = service.execute_json(&request.to_string());
    assert_eq!(result["status"], "error");
    assert_eq!(result["request_id"], "budget");
    assert_eq!(result["error"]["code"], "RESOURCE_LIMIT");
    assert_eq!(
        result["error"]["details"]["actual"],
        gerber_io::MAX_OBJECTS + 1
    );
    assert!(service.snapshot("over-limit").is_err());
}
