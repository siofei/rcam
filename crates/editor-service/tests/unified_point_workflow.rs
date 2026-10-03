//! B resolved world pivot through real service, atomic history and safe writer.
use editor_core::{MmPoint, SemanticGeometry};
use editor_service::*;
#[test]
fn three_layer_resolved_rotation_undo_redo_export_reopen_independent_coordinates() {
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/s5i2b");
    let out = std::env::temp_dir().join(format!(
        "rcam-i2-b-headless-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&out).unwrap();
    let mut s = ApplicationService::new();
    s.grant_file_access(&fixture, false).unwrap();
    s.grant_file_access(&out, true).unwrap();
    let d = s.document_new().unwrap();
    s.import_gerber_layers(
        &d.document_id,
        &d.revision,
        ImportGerberLayersParams {
            paths: ["layer_a.gbr", "layer_b.gbr", "layer_c.gbr"]
                .map(|n| fixture.join(n).to_str().unwrap().to_owned())
                .to_vec(),
        },
    )
    .unwrap();
    let info = s.document_get(&d.document_id).unwrap();
    let original = s.render_snapshot(&d.document_id).unwrap();
    let groups = original
        .layers
        .iter()
        .map(|l| SelectionGroup {
            layer_id: l.id.clone(),
            object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
        })
        .collect::<Vec<_>>();
    let query = s
        .geometry_selection_centers(
            &d.document_id,
            &info.revision,
            SelectionCentersParams {
                groups: groups.clone(),
                semantics: SelectionMaterialSemantics::SelectedLayerComposite,
            },
        )
        .unwrap();
    assert_eq!(query.bounding_center_mm, Some(MmPoint::new(10.25, 1.75)));
    let SelectionMaterialResult::Computed {
        value: CompositeMaterial::Ready { centroid_mm, .. },
    } = query.material
    else {
        panic!()
    };
    let a = std::f64::consts::PI * (1. - 0.25f64.powi(2));
    let total = 9. + 2. * a;
    assert!(
        centroid_mm.distance_mm(MmPoint::new(
            (24. + 22. * a + 20.) / total,
            (8. + 2. * a + 4.) / total
        )) < 1e-9
    );
    assert_eq!(s.document_get(&d.document_id).unwrap(), info);
    s.objects_edit_selection(
        &d.document_id,
        &info.revision,
        EditSelectionParams {
            groups,
            operation: SelectionEdit::Rotate {
                angle_deg: 90.,
                pivot_mm: query.bounding_center_mm.unwrap(),
            },
        },
    )
    .unwrap();
    let changed = s.document_get(&d.document_id).unwrap();
    assert_eq!(changed.undo_entries, info.undo_entries + 1);
    let rotated = s.render_snapshot(&d.document_id).unwrap();
    for (l, old) in rotated.layers.iter().zip(&original.layers) {
        assert_eq!(l.id, old.id);
        for (o, b) in l.objects.iter().zip(&old.objects) {
            assert_eq!(o.object_id, b.object_id);
            assert_eq!(o.exposure, b.exposure);
            let (
                SemanticGeometry::Flash { center, .. },
                SemanticGeometry::Flash { center: before, .. },
            ) = (&o.geometry, &b.geometry)
            else {
                panic!()
            };
            assert!(center.distance_mm(MmPoint::new(12. - before.y_mm, before.x_mm - 8.5)) < 1e-12);
        }
    }
    s.history_undo(&d.document_id, &changed.revision).unwrap();
    assert_eq!(
        s.render_snapshot(&d.document_id).unwrap().layers,
        original.layers
    );
    let undo = s.document_get(&d.document_id).unwrap();
    s.history_redo(&d.document_id, &undo.revision).unwrap();
    assert_eq!(
        s.render_snapshot(&d.document_id).unwrap().layers,
        rotated.layers
    );
    let rev = s.document_get(&d.document_id).unwrap().revision;
    for (n, l) in rotated.layers.iter().enumerate() {
        let path = out.join(format!("layer-{n}.gbr"));
        s.export_layer(
            &d.document_id,
            &rev,
            ExportParams {
                layer_id: l.id.clone(),
                path: path.to_str().unwrap().into(),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy: MetadataPolicy {
                    mode: "require_confirmation".into(),
                    categories: None,
                },
                compatibility_precision_override_mm: None,
            },
        )
        .unwrap();
        s.grant_file_access(&path, false).unwrap();
        let reopened = s.open(path.to_str().unwrap()).unwrap();
        let snap = s.render_snapshot(&reopened.document_id).unwrap();
        assert_eq!(snap.layers[0].objects.len(), l.objects.len());
        for (a, b) in snap.layers[0].objects.iter().zip(&l.objects) {
            assert_eq!(a.exposure, b.exposure);
            let (
                SemanticGeometry::Flash { center: x, .. },
                SemanticGeometry::Flash { center: y, .. },
            ) = (&a.geometry, &b.geometry)
            else {
                panic!()
            };
            assert!(x.distance_mm(*y) <= 0.0001);
        }
    }
    assert_eq!(
        s.document_get(&d.document_id).unwrap().revision,
        rev,
        "export/reopen does not mutate the source workspace"
    );
    // Preserve run artifacts; no directory removal.
}
