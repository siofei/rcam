//! Opt-in local acceptance; never embeds or overwrites a private sample.
#[test]
#[ignore = "requires RCAM_COMPRESSION_INPUT and RCAM_COMPRESSION_OUTPUT"]
fn legacy_project_compression_preserves_every_entry_and_semantic_value() {
    use rcam_project::{
        decode, encode_v1,
        zip_codec::{ReadPolicy, read_zip},
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
    for (a, b) in old.iter().zip(&new) {
        assert_eq!(a.path, b.path);
        assert_eq!(a.data, b.data, "entry {} changed", a.path);
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
