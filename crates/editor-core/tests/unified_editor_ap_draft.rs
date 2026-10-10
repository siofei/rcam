//! AP/geometry checkpoints and one cross-layer transaction, without size UI policy.
use editor_core::{edit::*, *};
fn document() -> SemanticDocument {
    SemanticDocument {
        id: "ap-checkpoint".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        source: SourceMetadata::default(),
        block_definitions: vec![],
        apertures: ["import-a", "import-b"]
            .into_iter()
            .enumerate()
            .map(|(n, id)| ApertureDefinition {
                id: id.into(),
                source_dcode: 10 + n as i32,
                shape: ApertureShape::Rectangle {
                    width_mm: 2.,
                    height_mm: 3.,
                    hole_diameter_mm: Some(0.5),
                },
            })
            .collect(),
        layers: ["a", "b"]
            .into_iter()
            .enumerate()
            .map(|(n, id)| SemanticLayer {
                id: id.into(),
                objects: (0..3)
                    .map(|i| SemanticObject {
                        object_id: format!("{id}-{i}"),
                        exposure: if i == 1 {
                            Exposure::Clear
                        } else {
                            Exposure::Dark
                        },
                        origin: ObjectOrigin::Imported { command_index: i },
                        geometry: SemanticGeometry::Flash {
                            center: MmPoint::new(i as f64 * 5., n as f64 * 10.),
                            aperture_id: format!("import-{id}"),
                            transform: LocalTransform::default(),
                        },
                    })
                    .collect(),
            })
            .collect(),
    }
}
fn groups() -> Vec<SelectionGroup> {
    ["a", "b"]
        .into_iter()
        .map(|id| SelectionGroup {
            layer_id: id.into(),
            object_ids: vec![format!("{id}-0"), format!("{id}-1")],
        })
        .collect()
}
fn size(g: Vec<SelectionGroup>, width: f64) -> DraftApertureSizeStep {
    DraftApertureSizeStep {
        groups: g,
        width_mm: width,
        height_mm: 3.,
    }
}
fn execute(
    d: &mut ManufacturingDraft,
    h: &EditHistory,
    doc: &SemanticDocument,
    g: Vec<SelectionGroup>,
    w: f64,
) {
    d.set_aperture_size_step(h, doc, d.generation(), size(g, w))
        .unwrap();
    d.execute_step(h, doc, &mut || Ok(()), &mut || Ok(()))
        .unwrap();
}
fn apply(h: &mut EditHistory, doc: &mut SemanticDocument, d: &ManufacturingDraft) -> Vec<String> {
    h.apply_manufacturing_draft(doc, d, &mut || Ok(()), &mut |_| Ok(()))
        .unwrap()
}
#[test]
fn cross_layer_apply_final_identity_one_undo_and_redo_are_exact() {
    let mut doc = document();
    doc.validate().unwrap();
    let entry = doc.clone();
    let mut h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 4.);
    assert_eq!(doc, entry);
    assert_eq!(d.work_candidate().apertures().len(), 2);
    let temporary: Vec<_> = d.work_candidate().apertures().cloned().collect();
    let mut prepared = None;
    let changed = h
        .apply_manufacturing_draft_prepared(
            &mut doc,
            &d,
            &mut || Ok(()),
            &mut |c| {
                let definitions: Vec<_> = c.apertures().cloned().collect();
                assert!(
                    definitions
                        .iter()
                        .all(|a| !temporary.iter().any(|t| t.id == a.id))
                );
                prepared = Some((
                    definitions,
                    c.objects().map(|(_, _, g)| g.clone()).collect::<Vec<_>>(),
                ));
                Ok(())
            },
            &mut |changed| {
                assert!(changed);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(changed.len(), 4);
    assert_eq!(h.undo_len(), 1);
    assert_eq!(doc.apertures.len(), 4);
    doc.validate().unwrap();
    let (ap, geometry) = prepared.unwrap();
    assert_eq!(&doc.apertures[2..], ap);
    assert_eq!(
        geometry,
        doc.layers
            .iter()
            .flat_map(|l| l.objects[..2].iter().map(|o| o.geometry.clone()))
            .collect::<Vec<_>>()
    );
    for (actual, old) in doc.layers.iter().zip(&entry.layers) {
        assert_eq!(actual.objects[2], old.objects[2]);
    }
    let committed = doc.clone();
    h.undo(&mut doc).unwrap();
    assert_eq!(doc, entry);
    h.redo(&mut doc).unwrap();
    assert_eq!(doc, committed);
}
#[test]
fn subset_cow_history_and_lineage_do_not_merge_imports() {
    let doc = document();
    let h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 4.);
    let first = d.work_geometry().to_vec();
    let subset = vec![SelectionGroup {
        layer_id: "a".into(),
        object_ids: vec!["a-0".into()],
    }];
    execute(&mut d, &h, &doc, subset.clone(), 5.);
    let second = d.work_geometry().to_vec();
    assert_eq!(d.work_candidate().apertures().len(), 3);
    assert_ne!(first[0], second[0]);
    assert_eq!(&first[1..], &second[1..]);
    d.undo().unwrap();
    assert_eq!(d.work_geometry(), first);
    assert_eq!(d.work_candidate().apertures().len(), 2);
    d.redo().unwrap();
    assert_eq!(d.work_geometry(), second);
    assert_eq!(d.work_candidate().apertures().len(), 3);
    execute(&mut d, &h, &doc, subset, 2.);
    assert_eq!(d.work_geometry()[0], d.entry_geometry()[0]);
    assert_eq!(d.work_candidate().apertures().len(), 2);
    d.reset().unwrap();
    assert_eq!(d.work_geometry(), d.entry_geometry());
    assert_eq!(d.work_candidate().apertures().len(), 0);
    assert_eq!(d.resources().unwrap().undo_entries, 0);
}
#[test]
fn exact_fold_nochange_preserves_main_redo_and_generated_counter() {
    let mut doc = document();
    let mut h = EditHistory::default();
    h.set_flash_size(&mut doc, "a", &["a-2".into()], 7., Some(3.))
        .unwrap();
    h.undo(&mut doc).unwrap();
    let entry = doc.clone();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 4.);
    execute(&mut d, &h, &doc, groups(), 2.);
    assert!(!d.has_work_changes());
    assert_eq!(d.work_candidate().apertures().len(), 0);
    assert!(apply(&mut h, &mut doc, &d).is_empty());
    assert_eq!(doc, entry);
    assert_eq!(h.redo_len(), 1);
    h.redo(&mut doc).unwrap();
    assert_eq!(
        doc.apertures.last().unwrap().id,
        "ap-checkpoint-generated-aperture-0"
    );
    h.undo(&mut doc).unwrap();
    let mut next = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut next, &h, &doc, groups(), 4.);
    apply(&mut h, &mut doc, &next);
    assert_eq!(doc.apertures[2].id, "ap-checkpoint-generated-aperture-1");
}
#[test]
fn restoring_ap_with_residual_move_is_changed_without_new_definition() {
    let mut doc = document();
    let mut h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 4.);
    d.set_step(
        &h,
        &doc,
        d.generation(),
        DraftStep {
            groups: groups(),
            operation: SelectionEdit::Move {
                dx_mm: 1.,
                dy_mm: 0.,
            },
        },
    )
    .unwrap();
    d.execute_step(&h, &doc, &mut || Ok(()), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 2.);
    assert!(d.has_work_changes());
    assert_eq!(d.work_candidate().apertures().len(), 0);
    assert_eq!(apply(&mut h, &mut doc, &d).len(), 4);
    assert_eq!(doc.apertures.len(), 2);
}
#[test]
fn preparation_cancel_and_commit_barrier_failures_publish_neither_domain() {
    let mut doc = document();
    let entry = doc.clone();
    let mut h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    d.set_aperture_size_step(&h, &doc, d.generation(), size(groups(), 4.))
        .unwrap();
    let generation = d.generation();
    assert!(
        d.execute_step_prepared(
            &h,
            &doc,
            &mut || Ok(()),
            &mut |_| Err(EditError::ResourceLimit),
            &mut || panic!("publication reached")
        )
        .is_err()
    );
    assert_eq!(d.generation(), generation);
    assert_eq!(d.work_geometry(), d.entry_geometry());
    assert_eq!(d.resources().unwrap().undo_entries, 0);
    assert!(
        h.apply_manufacturing_draft_prepared(
            &mut doc,
            &d,
            &mut || Ok(()),
            &mut |_| Err(EditError::ResourceLimit),
            &mut |_| panic!("commit reached")
        )
        .is_err()
    );
    assert_eq!(doc, entry);
    assert_eq!(h.undo_len(), 0);
    assert!(
        h.apply_manufacturing_draft(
            &mut doc,
            &d,
            &mut || Err(EditError::ResourceLimit),
            &mut |_| panic!("commit reached")
        )
        .is_err()
    );
    assert!(
        h.apply_manufacturing_draft(&mut doc, &d, &mut || Ok(()), &mut |_| Err(
            EditError::ResourceLimit
        ))
        .is_err()
    );
    assert_eq!(doc, entry);
    apply(&mut h, &mut doc, &d);
    assert_eq!(doc.apertures[2].id, "ap-checkpoint-generated-aperture-0");
}
#[test]
fn repeated_preview_execute_identity_and_new_input_namespace_are_stable() {
    let doc = document();
    let h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    d.set_aperture_size_step(&h, &doc, d.generation(), size(groups(), 4.))
        .unwrap();
    let mut seen = vec![];
    for _ in 0..2 {
        d.preview_step_prepared(
            &h,
            &doc,
            &mut || Ok(()),
            &mut |c| {
                seen.push(c.apertures().cloned().collect::<Vec<_>>());
                Ok(())
            },
            &mut || Ok(()),
        )
        .unwrap();
    }
    d.execute_step_prepared(
        &h,
        &doc,
        &mut || Ok(()),
        &mut |c| {
            seen.push(c.apertures().cloned().collect::<Vec<_>>());
            Ok(())
        },
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(seen[0], seen[1]);
    assert_eq!(seen[1], seen[2]);
    d.undo().unwrap();
    execute(&mut d, &h, &doc, groups(), 5.);
    assert_ne!(
        seen[0][0].id,
        d.work_candidate().apertures().next().unwrap().id
    );
    let mut foreign = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut foreign, &h, &doc, groups(), 5.);
    assert_ne!(
        foreign.work_candidate().apertures().next().unwrap().id,
        d.work_candidate().apertures().next().unwrap().id
    );
}
#[test]
fn late_layer_guard_and_identity_overflow_fail_atomically() {
    let mut doc = document();
    let mut h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 4.);
    doc.layers[1].objects[0].object_id = "replaced".into();
    let guarded = doc.clone();
    assert!(
        h.apply_manufacturing_draft(&mut doc, &d, &mut || Ok(()), &mut |_| Ok(()))
            .is_err()
    );
    assert_eq!(doc, guarded);
    assert_eq!(h.undo_len(), 0);
    let mut doc = document();
    doc.apertures[1].source_dcode = i32::MAX;
    let entry = doc.clone();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    d.set_aperture_size_step(&h, &doc, d.generation(), size(groups(), 4.))
        .unwrap();
    assert!(
        d.execute_step(&h, &doc, &mut || Ok(()), &mut || panic!(
            "publication reached"
        ))
        .is_err()
    );
    assert_eq!(doc, entry);
}
#[test]
fn undo_last_layer_guard_keeps_ap_and_earlier_layer_unchanged() {
    let mut doc = document();
    let mut h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 4.);
    apply(&mut h, &mut doc, &d);
    doc.layers[1].objects[1].object_id = "replaced".into();
    let guarded = doc.clone();
    assert!(h.undo(&mut doc).is_err());
    assert_eq!(doc, guarded);
    assert_eq!(h.undo_len(), 1);
    assert_eq!(h.redo_len(), 0);
}
#[test]
fn replaced_private_definitions_do_not_exhaust_temporary_dcodes() {
    let mut doc = document();
    doc.apertures[1].source_dcode = i32::MAX - 2;
    let h = EditHistory::default();
    let only = vec![groups().remove(0)];
    let mut d = h
        .begin_manufacturing_draft(&doc, only.clone(), &mut || Ok(()))
        .unwrap();
    for width in [4., 5., 6., 7., 8.] {
        execute(&mut d, &h, &doc, only.clone(), width);
        assert_eq!(d.work_candidate().apertures().len(), 1);
        assert_eq!(
            d.work_candidate().apertures().next().unwrap().source_dcode,
            i32::MAX - 1
        );
    }
}
#[test]
fn hostile_temporary_and_final_definition_collisions_fail_before_publication() {
    let mut doc = document();
    let h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    d.set_aperture_size_step(&h, &doc, d.generation(), size(groups(), 4.))
        .unwrap();
    let mut definition = None;
    d.preview_step_prepared(
        &h,
        &doc,
        &mut || Ok(()),
        &mut |c| {
            definition = c.apertures().next().cloned();
            Ok(())
        },
        &mut || Ok(()),
    )
    .unwrap();
    // Public core callers can supply hostile semantic IDs. Core may not publish a shadow definition.
    let mut collision = definition.unwrap();
    collision.source_dcode = 99;
    doc.apertures.push(collision);
    assert!(
        d.execute_step(&h, &doc, &mut || Ok(()), &mut || panic!(
            "publication reached"
        ))
        .is_err()
    );
    assert_eq!(d.resources().unwrap().undo_entries, 0);
    let mut doc = document();
    doc.apertures[0].id = "ap-checkpoint-generated-aperture-0".into();
    for o in &mut doc.layers[0].objects {
        if let SemanticGeometry::Flash { aperture_id, .. } = &mut o.geometry {
            *aperture_id = doc.apertures[0].id.clone();
        }
    }
    let mut h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    execute(&mut d, &h, &doc, groups(), 4.);
    let entry = doc.clone();
    assert!(
        h.apply_manufacturing_draft_prepared(
            &mut doc,
            &d,
            &mut || Ok(()),
            &mut |_| panic!("prepare reached"),
            &mut |_| panic!("commit reached")
        )
        .is_err()
    );
    assert_eq!(doc, entry);
    assert_eq!(h.undo_len(), 0);
}
#[test]
fn rectangle_hole_validation_and_joint_history_limit_preserve_work() {
    let doc = document();
    let h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    d.set_aperture_size_step(&h, &doc, d.generation(), size(groups(), 0.4))
        .unwrap();
    assert!(
        d.execute_step(&h, &doc, &mut || Ok(()), &mut || panic!(
            "publication reached"
        ))
        .is_err()
    );
    assert_eq!(d.work_geometry(), d.entry_geometry());
    for n in 0..100 {
        execute(&mut d, &h, &doc, groups(), 4. + n as f64);
    }
    let before = d.work_geometry().to_vec();
    let definitions = d.work_candidate().apertures().cloned().collect::<Vec<_>>();
    d.set_aperture_size_step(&h, &doc, d.generation(), size(groups(), 500.))
        .unwrap();
    assert_eq!(
        d.execute_step(&h, &doc, &mut || Ok(()), &mut || panic!(
            "publication reached"
        )),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d.work_geometry(), before);
    assert_eq!(
        d.work_candidate().apertures().cloned().collect::<Vec<_>>(),
        definitions
    );
    assert_eq!(d.resources().unwrap().undo_entries, 100);
}
#[test]
fn long_source_ap_lineage_is_charged_and_byte_refusal_retains_joint_history() {
    let mut doc = document();
    for n in 0..2 {
        let id = format!("import-{n}-{}", "x".repeat(1024 * 1024));
        doc.apertures[n].id = id.clone();
        for o in &mut doc.layers[n].objects {
            if let SemanticGeometry::Flash { aperture_id, .. } = &mut o.geometry {
                *aperture_id = id.clone();
            }
        }
    }
    let h = EditHistory::default();
    let mut d = h
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    let mut refused = false;
    for n in 0..100 {
        let geometry = d.work_geometry().to_vec();
        let defs = d.work_candidate().apertures().cloned().collect::<Vec<_>>();
        let entries = d.resources().unwrap().undo_entries;
        d.set_aperture_size_step(&h, &doc, d.generation(), size(groups(), 4. + n as f64))
            .unwrap();
        match d.execute_step(&h, &doc, &mut || Ok(()), &mut || Ok(())) {
            Ok(_) => {
                assert!(d.resources().unwrap().resident_bytes >= 2 * 1024 * 1024);
            }
            Err(EditError::ResourceLimit) => {
                assert!(entries < 100);
                assert_eq!(d.work_geometry(), geometry);
                assert_eq!(
                    d.work_candidate().apertures().cloned().collect::<Vec<_>>(),
                    defs
                );
                assert_eq!(d.resources().unwrap().undo_entries, entries);
                refused = true;
                break;
            }
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    assert!(
        refused,
        "lineage must count toward the unchanged 64 MiB byte limit"
    );
}
