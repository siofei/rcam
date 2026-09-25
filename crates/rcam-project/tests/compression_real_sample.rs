//! Opt-in local acceptance; never embeds or overwrites a private sample.
#[test]
#[ignore = "requires RCAM_COMPRESSION_INPUT and RCAM_COMPRESSION_OUTPUT"]
fn legacy_project_compression_preserves_every_entry_and_semantic_value() {
    use rcam_project::{
        decode, encode_v1,
        zip_codec::{ReadPolicy, ZipEntry, read_zip, write_zip},
    };
    let input = std::env::var("RCAM_COMPRESSION_INPUT").unwrap();
    let output = std::env::var("RCAM_COMPRESSION_OUTPUT").unwrap();
    assert_ne!(input, output);
    let before = std::fs::read(&input).unwrap();
    let project = decode(&before).unwrap();
    let compressed = encode_v1(&project).unwrap();
    assert!(compressed.len() < before.len() / 2);
    assert_eq!(decode(&compressed).unwrap(), project);
    assert_eq!(encode_v1(&project).unwrap(), compressed);
    let policy = ReadPolicy {
        max_entries: 20000,
        max_uncompressed_bytes: 512 * 1024 * 1024,
        max_entry_bytes: 128 * 1024 * 1024,
        max_path_len: 512,
    };
    let old = read_zip(&before, &policy).unwrap();
    let new = read_zip(&compressed, &policy).unwrap();
    assert_eq!(old.len(), new.len());
    // The codec itself must preserve every legacy entry byte-for-byte.
    let raw_entries: Vec<_> = old
        .iter()
        .map(|e| ZipEntry {
            path: &e.path,
            data: &e.data,
        })
        .collect();
    let lossless_zip = write_zip(&raw_entries);
    assert!(lossless_zip.len() < before.len() / 2);
    assert!(decode(&lossless_zip).unwrap() == project);
    let lossless = read_zip(&lossless_zip, &policy).unwrap();
    for (a, b) in old.iter().zip(&lossless) {
        assert_eq!(a.path, b.path);
        assert!(a.data == b.data, "raw codec entry changed: {}", a.path);
    }
    // S4-C1 introduced two serde-default view settings. Legacy decode fills
    // them; encode materializes them. This is not a manufacturing change.
    // Permit exactly those known defaults, not arbitrary JSON normalization.
    let entry = |entries: &[rcam_project::zip_codec::ReadEntry], name: &str| {
        entries
            .iter()
            .find(|e| e.path == name)
            .unwrap()
            .data
            .clone()
    };
    let project_bytes = entry(&new, "project.json");
    let mut expected: serde_json::Value =
        serde_json::from_slice(&entry(&old, "project.json")).unwrap();
    let snap = expected["workspace"]["snap"].as_object_mut().unwrap();
    snap.entry("manufacturing_boundary")
        .or_insert(serde_json::json!(true));
    snap.entry("original_path")
        .or_insert(serde_json::json!(false));
    assert!(
        expected == serde_json::from_slice::<serde_json::Value>(&project_bytes).unwrap(),
        "project changed beyond the two S4-C1 view defaults"
    );
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&entry(&old, "manifest.json")).unwrap();
    let project_entry = manifest["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["path"] == "project.json")
        .unwrap();
    project_entry["sha256"] = serde_json::json!(editor_core::hash::sha256_hex(&project_bytes));
    project_entry["uncompressed_size"] = serde_json::json!(project_bytes.len());
    assert!(
        manifest
            == serde_json::from_slice::<serde_json::Value>(&entry(&new, "manifest.json")).unwrap(),
        "manifest changed beyond verified project settings"
    );
    for (a, b) in old.iter().zip(&new) {
        assert_eq!(a.path, b.path);
        if !matches!(a.path.as_str(), "project.json" | "manifest.json") {
            assert!(a.data == b.data, "manufacturing entry changed: {}", a.path);
        }
    }
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap()
        .write_all(&compressed)
        .unwrap();
    assert_eq!(std::fs::read(input).unwrap(), before);
    println!(
        "raw_codec_all_entries_identical=true manufacturing_entries_identical=true allowed_view_defaults=manufacturing_boundary:true,original_path:false"
    );
    println!(
        "original_bytes={} compressed_bytes={} entries={} layers={} objects={}",
        before.len(),
        compressed.len(),
        new.len(),
        project.layers.len(),
        project
            .layers
            .iter()
            .map(|l| l.layer.objects.len())
            .sum::<usize>()
    );
}
