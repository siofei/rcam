mod common;
use editor_core::{MmPoint, board::*, pnp::*};
use rcam_project::*;
use serde_json::json;
fn board() -> BoardState {
    let mapping:PnpMapping=serde_json::from_value(json!({"delimiter":"csv","unit":"mm","refdes":0,"x":1,"y":2,"rotation":3,"side":4,"footprint":5,"value":6,"top_token":"Top","bottom_token":"Bottom","clockwise":false,"rotation_offset_deg":0,"invert_y":false})).unwrap();
    BoardState {
        components: std::sync::Arc::new(
            parse_pnp(
                include_bytes!("../../../fixtures/synthetic/s4d1/pnp.csv"),
                &mapping,
            )
            .components,
        ),
        mapping,
        provenance: PnpProvenance {
            basename: "pnp.csv".into(),
            sha256: "a".repeat(64),
            imported_unix_seconds: 123,
        },
        registration: Some(
            registration(RegistrationInput::Manual {
                transform: CoordinateTransform2D {
                    reflect_x: true,
                    rotation_deg: 37.,
                    translation: MmPoint::new(10., -12.),
                },
            })
            .unwrap(),
        ),
    }
}
#[test]
fn v1_unchanged_v2_roundtrip_determinism_and_invalid_board_rejection() {
    let mut p = common::big_project(4, 2);
    let v1 = encode_v1(&p).unwrap();
    assert_eq!(decode(&v1).unwrap(), p);
    assert_eq!(encode_v1(&decode(&v1).unwrap()).unwrap(), v1);
    p.board = Some(board());
    assert!(encode_v1(&p).is_err());
    p.format_version = 2;
    let bytes = encode_v1(&p).unwrap();
    assert_eq!(decode(&bytes).unwrap(), p);
    assert_eq!(encode_v1(&decode(&bytes).unwrap()).unwrap(), bytes);
    assert_eq!(
        p.to_semantic_document(),
        decode(&v1).unwrap().to_semantic_document()
    );
    for kind in 0..8 {
        let mut invalid = p.clone();
        let b = invalid.board.as_mut().unwrap();
        match kind {
            0 => std::sync::Arc::make_mut(&mut b.components)[0].position.x_mm = f64::NAN,
            1 => std::sync::Arc::make_mut(&mut b.components)[0].rotation_deg = f64::INFINITY,
            2 => std::sync::Arc::make_mut(&mut b.components)[0].refdes = String::new(),
            3 => {
                let duplicate = b.components[0].clone();
                std::sync::Arc::make_mut(&mut b.components).push(duplicate);
            }
            4 => b.provenance.basename = "/Users/private/customer.csv".into(),
            5 => b.registration.as_mut().unwrap().residual_mm = 1.,
            6 => b.mapping.side = b.mapping.x,
            _ => b.provenance.sha256 = "wrong".into(),
        };
        assert!(encode_v1(&invalid).is_err(), "{kind}");
    }
}
fn mutate_project(
    bytes: &[u8],
    version: u32,
    transform: impl FnOnce(&mut serde_json::Value),
) -> Vec<u8> {
    use rcam_project::zip_codec::*;
    let entries = read_zip(
        bytes,
        &ReadPolicy {
            max_entries: 20000,
            max_uncompressed_bytes: 512 * 1024 * 1024,
            max_entry_bytes: 128 * 1024 * 1024,
            max_path_len: 512,
        },
    )
    .unwrap();
    let mut files: Vec<_> = entries
        .into_iter()
        .filter(|e| e.path != "manifest.json")
        .map(|e| (e.path, e.data))
        .collect();
    let root = files.iter_mut().find(|(p, _)| p == "project.json").unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&root.1).unwrap();
    value["format_version"] = json!(version);
    transform(&mut value);
    root.1 = serde_json::to_vec(&value).unwrap();
    let manifest = manifest::build_versioned("pnp-test", version, &files);
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let mut zip = vec![ZipEntry {
        path: "manifest.json",
        data: &manifest,
    }];
    zip.extend(files.iter().map(|(path, data)| ZipEntry { path, data }));
    write_zip(&zip)
}
#[test]
fn legacy_placeholder_migration_and_hash_valid_corrupt_board_fail_closed() {
    let p = common::big_project(4, 2);
    let bytes = encode_v1(&p).unwrap();
    let legacy = mutate_project(&bytes, 1, |v| v["board"] = json!({}));
    assert!(decode(&legacy).unwrap().board.is_none());
    let nonempty = mutate_project(&bytes, 1, |v| v["board"] = json!({"components":[]}));
    assert!(decode(&nonempty).is_err());
    let invalid = mutate_project(&bytes, 2, |v| {
        v["board"] = json!({"components":[],"evil":1})
    });
    assert!(decode(&invalid).is_err());
    let valid = mutate_project(&bytes, 2, |v| v["board"] = json!(board()));
    assert!(decode(&valid).is_ok());
    let bad = mutate_project(&valid, 2, |v| {
        v["board"]["components"][0]["side"] = json!("mystery")
    });
    assert!(decode(&bad).is_err());
    let unknown = mutate_project(&valid, 3, |_| {});
    assert!(matches!(
        decode(&unknown),
        Err(ProjectError::UnknownFormatVersion(3))
    ));
    let mut truncated = valid.clone();
    truncated.truncate(valid.len() - 10);
    assert!(decode(&truncated).is_err());
}
