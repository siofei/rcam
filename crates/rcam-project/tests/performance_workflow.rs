use rcam_project::*;
use std::time::Instant;
mod common;
use common::big_project;

#[test]
fn project_400_openings_times_100_instances_does_not_scale_like_flattened_primitives() {
    let project = big_project(400, 100);
    let start = Instant::now();
    let bytes = encode_v1(&project).unwrap();
    let encode_elapsed = start.elapsed();

    let start = Instant::now();
    let decoded = decode(&bytes).unwrap();
    let decode_elapsed = start.elapsed();
    decoded.validate().unwrap();

    // A flattened project (40,000 independent Flash objects instead of 100
    // instances of a 400-object definition) would be roughly two orders of
    // magnitude larger; a generous 2 MB ceiling proves the definition is
    // stored once, not duplicated per instance.
    assert!(
        bytes.len() < 2 * 1024 * 1024,
        "unexpectedly large encode ({} bytes) for a shared-definition project",
        bytes.len()
    );
    assert_eq!(decoded.block_definitions[0].objects.len(), 400);
    assert_eq!(decoded.layers[0].layer.objects.len(), 100);
    assert!(
        encode_elapsed.as_secs() < 5 && decode_elapsed.as_secs() < 5,
        "encode {encode_elapsed:?} / decode {decode_elapsed:?} exceeded a generous bound"
    );
    println!(
        "S4B2_CODEC_PERF bytes={} encode_ms={:.3} decode_ms={:.3} definition_objects={} project_instances={}",
        bytes.len(),
        encode_elapsed.as_secs_f64() * 1e3,
        decode_elapsed.as_secs_f64() * 1e3,
        decoded.block_definitions[0].objects.len(),
        decoded.layers[0].layer.objects.len(),
    );
}
