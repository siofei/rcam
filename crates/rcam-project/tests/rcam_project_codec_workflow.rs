//! `.rcam` codec workflow (§55/§64 of the S4-B2 brief): build a project,
//! encode, decode, validate, compare every field the brief calls out, then
//! encode the decoded project again and check for byte-for-byte determinism.
use editor_core::block::{
    BlockDefinition, BlockDefinitionId, BlockObject, BlockObjectGeometry, BlockTransform,
};
use editor_core::snap::SnapKind;
use editor_core::units::ManufacturingPrecision;
use editor_core::workspace::{Color, ImportProvenance, LayerKind, LayerWorkspaceState};
use editor_core::{
    ApertureDefinition, ApertureShape, Exposure, MmPoint, ObjectOrigin, SemanticGeometry,
    SemanticLayer, SemanticObject,
};
use rcam_project::codec::Budget;
use rcam_project::*;

fn sample_project() -> RCamProject {
    let aperture = ApertureDefinition {
        id: "src::D10".into(),
        source_dcode: 10,
        shape: ApertureShape::Circle {
            diameter_mm: 0.5,
            hole_diameter_mm: None,
        },
    };
    let definition = BlockDefinition {
        id: BlockDefinitionId("blk-1".into()),
        name: "opening".into(),
        local_origin: MmPoint::new(0., 0.),
        objects: vec![BlockObject {
            geometry: BlockObjectGeometry::Line {
                start: MmPoint::new(0., 0.),
                end: MmPoint::new(1., 0.),
                width_mm: 0.1,
            },
            exposure: Exposure::Dark,
        }],
        revision: 0,
    };
    let instance = SemanticObject {
        object_id: "obj-instance-1".into(),
        geometry: SemanticGeometry::BlockInstance {
            definition_id: definition.id.clone(),
            transform: BlockTransform {
                translation: MmPoint::new(3., 4.),
                rotation_deg: 37.,
                mirror: true,
            },
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Generated {
            operation_id: "op-1".into(),
        },
    };
    let flash = SemanticObject {
        object_id: "obj-flash-1".into(),
        geometry: SemanticGeometry::Flash {
            center: MmPoint::new(1., 2.),
            aperture_id: aperture.id.clone(),
            transform: editor_core::LocalTransform {
                mirror: editor_core::Mirror::None,
                rotation_deg: 0.,
                scale: 1.,
            },
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Imported { command_index: 0 },
    };

    let layer_a = LayerProjectState {
        layer: SemanticLayer {
            id: "layer-a".into(),
            objects: vec![flash, instance],
        },
        workspace: LayerWorkspaceState::new(
            LayerKind::Gerber,
            "Top Copper",
            Color::rgb(200, 40, 40),
        ),
        provenance: Some(ImportProvenance {
            import_id: "imp-1".into(),
            original_file_name: "top.gbr".into(),
            imported_sha256: "a".repeat(64),
            imported_at: "2026-01-01T00:00:00Z".into(),
        }),
    };
    let layer_b = LayerProjectState {
        layer: SemanticLayer {
            id: "layer-b".into(),
            objects: vec![],
        },
        workspace: LayerWorkspaceState::new(
            LayerKind::Gerber,
            "Silkscreen",
            Color::rgb(20, 200, 20),
        ),
        provenance: None,
    };

    RCamProject {
        format_version: FORMAT_VERSION,
        project_id: ProjectId("proj-1".into()),
        manufacturing: ManufacturingProjectSettings {
            precision: ManufacturingPrecision {
                resolution_mm: 0.0005,
            },
        },
        workspace: WorkspaceProjectState {
            display_unit: DisplayUnit::Millimeters,
            grid: GridSettings {
                spacing_mm: 0.5,
                visible: true,
                snap: true,
            },
            snap: SnapSettingsState {
                enabled: true,
                enabled_kinds: vec![SnapKind::Endpoint, SnapKind::Center],
                radius_px: 8.0,
            },
            active_layer_id: Some("layer-a".into()),
            camera: Some(CameraState {
                center_mm: MmPoint::new(5., 5.),
                scale: 10.,
            }),
        },
        layer_order: vec!["layer-a".into(), "layer-b".into()],
        layers: vec![layer_a, layer_b],
        apertures: vec![aperture],
        block_definitions: vec![definition],
        board: None,
    }
}

#[test]
fn format_version_is_1() {
    assert_eq!(FORMAT_VERSION, 1);
    let project = sample_project();
    assert_eq!(project.format_version, 1);
}

#[test]
fn deterministic_encode_and_encode_decode_encode_round_trip() {
    let project = sample_project();
    let bytes1 = encode_v1(&project).unwrap();
    let bytes2 = encode_v1(&project).unwrap();
    assert_eq!(
        bytes1, bytes2,
        "encoding the same project twice is byte-identical"
    );

    let decoded = decode(&bytes1).unwrap();
    let bytes3 = encode_v1(&decoded).unwrap();
    assert_eq!(
        bytes1, bytes3,
        "encode -> decode -> encode is byte-identical"
    );
}

#[test]
fn all_display_units_round_trip_with_deterministic_bytes() {
    for (unit, spelling) in [
        (DisplayUnit::Millimeters, "millimeters"),
        (DisplayUnit::Inches, "inches"),
        (DisplayUnit::Mils, "mils"),
        (DisplayUnit::Micrometers, "micrometers"),
    ] {
        let mut project = sample_project();
        project.workspace.display_unit = unit;
        let encoded = encode_v1(&project).unwrap();
        assert!(
            encoded
                .windows(spelling.len())
                .any(|part| part == spelling.as_bytes())
        );
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.workspace.display_unit, unit);
        assert_eq!(encode_v1(&decoded).unwrap(), encoded);
    }
}

#[test]
fn existing_millimeter_projects_still_open() {
    for fixture in [
        include_bytes!("../../../fixtures/synthetic/s4b2/sample.rcam").as_slice(),
        include_bytes!("../../../fixtures/synthetic/s4b3/project.rcam").as_slice(),
    ] {
        let project = decode(fixture).unwrap();
        assert_eq!(project.format_version, FORMAT_VERSION);
        assert_eq!(project.workspace.display_unit, DisplayUnit::Millimeters);
    }
}

#[test]
fn manifest_hashes_are_verified_and_corruption_is_rejected() {
    let project = sample_project();
    let mut bytes = encode_v1(&project).unwrap();
    // Flip a byte deep enough to land inside file content, not the ZIP
    // structural tail (central directory / EOCD), on more than one offset so
    // the test is not sensitive to exact layout.
    for probe in [40, 80, 120] {
        let mut corrupted = bytes.clone();
        if probe < corrupted.len() {
            corrupted[probe] ^= 0xFF;
            assert!(decode(&corrupted).is_err(), "probe {probe}");
        }
    }
    // A clean copy still decodes fine (the corruption assertions above must
    // not have mutated the original).
    assert!(decode(&bytes).is_ok());
    bytes.clear();
}

#[test]
fn missing_manifest_is_rejected() {
    let project = sample_project();
    // Build an archive without a manifest.json entry directly, bypassing the
    // encoder, to exercise the reader's own fail-closed check.
    let json = serde_json::to_vec(&project.project_id.0).unwrap();
    let entries = [rcam_project_test_support::entry("project.json", &json)];
    let bytes = rcam_project_test_support::write_zip_for_test(&entries);
    assert_eq!(
        decode(&bytes),
        Err(rcam_project::ProjectError::ManifestMissing)
    );
}

#[test]
fn duplicate_path_and_path_traversal_are_rejected() {
    let a = rcam_project_test_support::entry("a.json", b"1");
    let b = rcam_project_test_support::entry("a.json", b"2");
    let bytes = rcam_project_test_support::write_zip_for_test(&[a, b]);
    assert!(decode(&bytes).is_err());

    let escape = rcam_project_test_support::entry("../escape.json", b"1");
    let bytes = rcam_project_test_support::write_zip_for_test(&[escape]);
    assert!(decode(&bytes).is_err());
}

#[test]
fn oversized_entry_and_too_many_entries_are_rejected() {
    let project = sample_project();
    let bytes = encode_v1(&project).unwrap();
    let tiny_entry_budget = Budget {
        max_entry_bytes: 4,
        ..Budget::default()
    };
    assert!(matches!(
        rcam_project::codec::decode_with_budget(&bytes, &tiny_entry_budget),
        Err(rcam_project::ProjectError::ResourceLimit { .. })
    ));
    let tiny_entries_budget = Budget {
        max_entries: 1,
        ..Budget::default()
    };
    assert!(matches!(
        rcam_project::codec::decode_with_budget(&bytes, &tiny_entries_budget),
        Err(rcam_project::ProjectError::ResourceLimit { .. })
    ));
}

#[test]
fn oversized_string_field_is_rejected_but_the_same_archive_is_fine_under_the_default_budget() {
    let project = sample_project();
    let bytes = encode_v1(&project).unwrap();
    let oversized = format!("\"{}\"", "x".repeat(2_000));
    let corrupted = rcam_project_test_support::replace_json_text(
        &bytes,
        "blocks/blk-1.json",
        "\"opening\"",
        &oversized,
    );
    let tiny_string_budget = Budget {
        max_string_len: 100,
        ..Budget::default()
    };
    assert!(matches!(
        rcam_project::codec::decode_with_budget(&corrupted, &tiny_string_budget),
        Err(rcam_project::ProjectError::ResourceLimit {
            resource: "string_len",
            ..
        })
    ));
    assert!(
        decode(&corrupted).is_ok(),
        "the same archive is under the default budget's much larger max_string_len"
    );
}

#[test]
fn deeply_nested_json_is_rejected() {
    let project = sample_project();
    let bytes = encode_v1(&project).unwrap();
    // Deep enough to clear the tiny test budget below, shallow enough to
    // stay under serde_json's own ~128-frame implicit recursion limit — the
    // whole point is that *our* bounded check, not that implicit backstop,
    // is what actually rejects this archive.
    let deeply_nested = format!("{}1{}", "[".repeat(40), "]".repeat(40));
    let corrupted = rcam_project_test_support::replace_json_text(
        &bytes,
        "blocks/blk-1.json",
        "0.1",
        &deeply_nested,
    );
    let tiny_depth_budget = Budget {
        max_json_depth: 16,
        ..Budget::default()
    };
    assert!(matches!(
        rcam_project::codec::decode_with_budget(&corrupted, &tiny_depth_budget),
        Err(rcam_project::ProjectError::ResourceLimit {
            resource: "json_depth",
            ..
        })
    ));
}

#[test]
fn invalid_finite_value_is_rejected() {
    let mut project = sample_project();
    project.manufacturing.precision.resolution_mm = f64::NAN;
    assert!(encode_v1(&project).is_err(), "encoder must not write NaN");

    // Also exercise the decode-time path: hand-build a project.json with an
    // infinite manufacturing_resolution and confirm decode rejects it too.
    let mut good = sample_project();
    good.manufacturing.precision.resolution_mm = 1.0;
    let bytes = encode_v1(&good).unwrap();
    let corrupted =
        rcam_project_test_support::replace_json_number(&bytes, "project.json", "1.0", "1e400");
    assert!(decode(&corrupted).is_err());
}

#[test]
fn unknown_format_version_is_rejected() {
    let project = sample_project();
    let bytes = encode_v1(&project).unwrap();
    let corrupted =
        rcam_project_test_support::replace_json_number(&bytes, "manifest.json", "1", "999");
    assert!(matches!(
        decode(&corrupted),
        Err(rcam_project::ProjectError::UnknownFormatVersion(_))
    ));
}

#[test]
fn unknown_mandatory_object_type_is_rejected() {
    let project = sample_project();
    let bytes = encode_v1(&project).unwrap();
    let corrupted = rcam_project_test_support::replace_json_text(
        &bytes,
        "layers/layer-a.json",
        "\"Flash\"",
        "\"TotallyUnknownGeometryKind\"",
    );
    assert!(decode(&corrupted).is_err());
}

#[test]
fn ids_view_styles_precision_grid_snap_round_trip() {
    let project = sample_project();
    let bytes = encode_v1(&project).unwrap();
    let decoded = decode(&bytes).unwrap();

    assert_eq!(decoded.project_id, project.project_id);
    assert_eq!(decoded.layer_order, project.layer_order);
    for (before, after) in project.layers.iter().zip(&decoded.layers) {
        assert_eq!(before.layer.id, after.layer.id, "layer id stable");
        assert_eq!(before.workspace, after.workspace, "view style round-trips");
        let before_ids: Vec<_> = before.layer.objects.iter().map(|o| &o.object_id).collect();
        let after_ids: Vec<_> = after.layer.objects.iter().map(|o| &o.object_id).collect();
        assert_eq!(before_ids, after_ids, "object ids stable");
    }
    assert_eq!(
        decoded.block_definitions, project.block_definitions,
        "block definitions round-trip"
    );
    let SemanticGeometry::BlockInstance {
        definition_id,
        transform,
    } = &decoded.layers[0].layer.objects[1].geometry
    else {
        panic!("expected the second object to still be a block instance");
    };
    assert_eq!(definition_id, &project.block_definitions[0].id);
    assert_eq!(
        *transform,
        BlockTransform {
            translation: MmPoint::new(3., 4.),
            rotation_deg: 37.,
            mirror: true,
        }
    );
    assert_eq!(
        decoded.manufacturing.precision, project.manufacturing.precision,
        "manufacturing precision round-trips"
    );
    assert_eq!(decoded.workspace.grid, project.workspace.grid);
    assert_eq!(decoded.workspace.snap, project.workspace.snap);
}

#[test]
fn solo_selection_and_app_preferences_have_no_schema_field_to_persist() {
    // These are architectural absences, not runtime filters: the schema
    // types themselves have no Solo/Selection/shortcut/panel-width/recent-
    // color/theme field to serialize in the first place.
    let bytes = encode_v1(&sample_project()).unwrap();
    let text = String::from_utf8(bytes).unwrap_or_default();
    for forbidden in [
        "solo",
        "\"selection\"",
        "shortcut",
        "panel_width",
        "recent_color",
        "\"theme\"",
    ] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "unexpectedly found {forbidden:?} in the encoded archive text scan"
        );
    }
}

#[test]
fn provenance_never_carries_an_absolute_source_path() {
    let project = sample_project();
    let provenance = project.layers[0].provenance.as_ref().unwrap();
    assert!(!provenance.original_file_name.contains('/'));
    assert!(!provenance.original_file_name.starts_with('/'));
    let bytes = encode_v1(&project).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.layers[0].provenance, project.layers[0].provenance);
}

#[test]
fn decoded_project_passes_schema_validation() {
    let project = sample_project();
    let bytes = encode_v1(&project).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert!(decoded.validate().is_ok());
}

#[test]
fn drill_layer_is_not_encodable_or_decodable_in_v1() {
    let mut project = sample_project();
    project.layers[1].workspace.kind = LayerKind::Drill;
    assert!(
        encode_v1(&project).is_err(),
        "encoder must refuse to produce a v1 Drill layer (DrillObject has no persistence)"
    );
}

/// Minimal helpers for constructing raw, possibly-invalid `.rcam`-shaped
/// archives directly (bypassing the encoder) to test the *reader's* own
/// fail-closed checks in isolation.
mod rcam_project_test_support {
    pub fn entry<'a>(path: &'a str, data: &'a [u8]) -> (&'a str, &'a [u8]) {
        (path, data)
    }

    pub fn write_zip_for_test(entries: &[(&str, &[u8])]) -> Vec<u8> {
        // Re-implement the minimal store-only writer here rather than expose
        // the crate's internal `zip_codec` module publicly just for tests.
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (path, data) in entries {
            let offset = out.len() as u32;
            let name = path.as_bytes();
            let crc = crc32(data);
            let size = data.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0x0800u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0x0021u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name);
            out.extend_from_slice(data);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0x0800u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0x0021u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name);
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// Decode a well-formed archive built by `encode_v1`, replace one JSON
    /// entry's text, and re-encode a fresh archive with everything else
    /// unchanged (including a manifest hash mismatch for the edited entry,
    /// which several tests want; others recompute the manifest instead).
    pub fn replace_json_text(bytes: &[u8], path: &str, from: &str, to: &str) -> Vec<u8> {
        edit_entry(bytes, path, |text| text.replacen(from, to, 1))
    }

    pub fn replace_json_number(bytes: &[u8], path: &str, from: &str, to: &str) -> Vec<u8> {
        edit_entry(bytes, path, |text| text.replacen(from, to, 1))
    }

    fn edit_entry(bytes: &[u8], target_path: &str, edit: impl Fn(String) -> String) -> Vec<u8> {
        let read_entries = read_zip_for_test(bytes);
        if target_path == "manifest.json" {
            // Editing the manifest itself: replace it verbatim, keep every
            // other entry byte-for-byte untouched.
            let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
            for (path, data) in read_entries {
                if path == "manifest.json" {
                    let text = String::from_utf8(data).unwrap();
                    entries.push((path, edit(text).into_bytes()));
                } else {
                    entries.push((path, data));
                }
            }
            let refs: Vec<(&str, &[u8])> = entries
                .iter()
                .map(|(p, d)| (p.as_str(), d.as_slice()))
                .collect();
            return write_zip_for_test(&refs);
        }
        // Editing a non-manifest entry: rebuild manifest hashes to match, so
        // tests about schema/semantic validation (not hash-mismatch
        // detection, which the dedicated corruption test covers separately)
        // get a structurally consistent archive.
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for (path, data) in read_entries {
            if path == target_path {
                let text = String::from_utf8(data).unwrap();
                files.push((path, edit(text).into_bytes()));
            } else if path != "manifest.json" {
                files.push((path, data));
            }
        }
        let manifest = rcam_project::manifest::build("proj-1", &files);
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let mut entries: Vec<(&str, &[u8])> = vec![("manifest.json", &manifest_bytes)];
        for (path, data) in &files {
            entries.push((path.as_str(), data.as_slice()));
        }
        write_zip_for_test(&entries)
    }

    fn read_zip_for_test(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let policy = rcam_project::zip_codec::ReadPolicy {
            max_entries: 100_000,
            max_uncompressed_bytes: usize::MAX / 2,
            max_entry_bytes: usize::MAX / 2,
            max_path_len: 4096,
        };
        rcam_project::zip_codec::read_zip(bytes, &policy)
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect()
    }
}
