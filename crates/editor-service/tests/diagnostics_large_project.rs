//! Opt-in local, read-only private input. Public output contains only numeric/hash summaries.
#[test]
#[ignore = "requires RCAM_DIAGNOSTICS_INPUT and a new RCAM_DIAGNOSTICS_OUTPUT directory"]
fn large_project_real_service_timings() {
    use editor_service::{ApplicationService, FileAccessPolicy};
    use std::{fs, path::PathBuf};
    let input = PathBuf::from(std::env::var_os("RCAM_DIAGNOSTICS_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let out = PathBuf::from(std::env::var_os("RCAM_DIAGNOSTICS_OUTPUT").unwrap());
    fs::create_dir(&out).unwrap();
    let out = out.canonicalize().unwrap();
    let original_hash = editor_core::hash::sha256_hex(&fs::read(&input).unwrap());
    let guard = rcam_diagnostics::Runtime::start(
        out.join("logs"),
        env!("CARGO_PKG_VERSION"),
        &std::env::var("RCAM_TEST_COMMIT").unwrap(),
    )
    .unwrap();
    assert!(guard.install_sink());
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        &out,
        [input.parent().unwrap().to_owned(), out.clone()],
        [out.clone()],
    ));
    let info = service.project_open(input.to_str().unwrap()).unwrap();
    let saved = out.join("private-output.rcam");
    service
        .project_save(
            &info.document_id,
            &info.revision,
            Some(saved.to_str().unwrap()),
            false,
        )
        .unwrap();
    let reopened = service.project_open(saved.to_str().unwrap()).unwrap();
    let layers = service.layers_list(&reopened.document_id).unwrap();
    let context = editor_service::diagnostic_context(
        &reopened,
        &layers,
        Some(&service.render_snapshot(&reopened.document_id).unwrap()),
        editor_core::units::DisplayUnit::Millimeter,
    );
    assert!(context.project.as_ref().unwrap().object_count >= 500_000);
    let bytes = fs::read(&saved).unwrap();
    let entries = rcam_project::zip_codec::read_zip(
        &bytes,
        &rcam_project::zip_codec::ReadPolicy {
            max_entries: 20000,
            max_uncompressed_bytes: 512 * 1024 * 1024,
            max_entry_bytes: 128 * 1024 * 1024,
            max_path_len: 512,
        },
    )
    .unwrap();
    let uncompressed: usize = entries.iter().map(|e| e.data.len()).sum();
    let summary = serde_json::json!({"schema_version":2,"compressed_file_bytes":bytes.len(),"uncompressed_entry_bytes":uncompressed,"compression_ratio":bytes.len() as f64/uncompressed as f64,"input_sha256_prefix":&original_hash[..16],"project":context.project,"source_unchanged":original_hash==editor_core::hash::sha256_hex(&fs::read(input).unwrap()),"geometry_in_public_evidence":false});
    assert_eq!(summary["source_unchanged"], true);
    fs::write(
        out.join("large-summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    guard
        .runtime()
        .export_with_context(&out.join("diagnostics.zip"), context)
        .unwrap();
    println!("{summary}");
    // Intentionally retain private output only in the ignored evidence directory, never public ZIP.
}
