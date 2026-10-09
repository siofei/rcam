//! Synthetic capacity/peak measurements, independent from UI/native acceptance.
use editor_core::edit::*;
use editor_core::*;
fn document(edges: usize) -> SemanticDocument {
    let points: Vec<_> = (0..edges)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / edges as f64;
            MmPoint::new(10. * a.cos(), 10. * a.sin())
        })
        .collect();
    let contour = RegionContour {
        role: RegionRole::Solid,
        edges: (0..edges)
            .map(|i| RegionEdge::Line {
                start: points[i],
                end: points[(i + 1) % edges],
            })
            .collect(),
    };
    SemanticDocument {
        id: "synthetic".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![SemanticObject {
                object_id: "region".into(),
                geometry: SemanticGeometry::Region {
                    contours: vec![contour],
                },
                exposure: Exposure::Dark,
                origin: ObjectOrigin::Imported { command_index: 0 },
            }],
        }],
        apertures: vec![],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}
fn groups() -> Vec<SelectionGroup> {
    vec![SelectionGroup {
        layer_id: "layer".into(),
        object_ids: vec!["region".into()],
    }]
}
fn step() -> DraftStep {
    DraftStep {
        groups: groups(),
        operation: SelectionEdit::Move {
            dx_mm: 0.01,
            dy_mm: 0.,
        },
    }
}
#[test]
fn snapshot_history_and_transaction_peak_charge_measurement() {
    let mut doc = document(4096);
    doc.validate().unwrap();
    let before = doc.clone();
    let mut history = EditHistory::default();
    let mut d = history
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    println!("MEASUREMENT entry {:?}", d.resources().unwrap());
    for _ in 0..64 {
        let generation = d.generation();
        d.set_step(&history, &doc, generation, step()).unwrap();
        d.execute_step(&history, &doc, &mut || Ok(()), &mut || Ok(()))
            .unwrap();
    }
    println!("MEASUREMENT history64 {:?}", d.resources().unwrap());
    let work = d.work_geometry().to_vec();
    d.set_step(&history, &doc, d.generation(), step()).unwrap();
    d.preview_step(&history, &doc, &mut || Ok(()), &mut || Ok(()))
        .unwrap();
    println!("MEASUREMENT history64_preview {:?}", d.resources().unwrap());
    assert!(
        !history
            .apply_manufacturing_draft(&mut doc, &d, &mut || Ok(()), &mut |_| Ok(()))
            .unwrap()
            .is_empty()
    );
    println!("MEASUREMENT final_main_history_bytes {}", history.bytes());
    assert_eq!(history.undo_len(), 1);
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, before);
    history.redo(&mut doc).unwrap();
    assert_ne!(doc, before);
    assert_eq!(d.work_geometry(), work);
}
#[test]
fn byte_refusal_keeps_existing_checkpoints_and_original_geometry() {
    let doc = document(1024);
    let before = doc.clone();
    let history = EditHistory::with_limits(100, 3 * 1024 * 1024).unwrap();
    let mut d = history
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    let mut executed = 0;
    loop {
        let work = d.work_geometry().to_vec();
        let generation = d.generation();
        if let Err(error) = d.set_step(&history, &doc, generation, step()) {
            assert_eq!(error, EditError::ResourceLimit);
            assert_eq!(d.work_geometry(), work);
            break;
        }
        if let Err(error) = d.execute_step(&history, &doc, &mut || Ok(()), &mut || Ok(())) {
            assert_eq!(error, EditError::ResourceLimit);
            assert_eq!(d.work_geometry(), work);
            break;
        }
        executed += 1;
        assert!(executed < 100);
    }
    assert!(executed > 0);
    println!(
        "MEASUREMENT refused_after_steps {executed} {:?}",
        d.resources().unwrap()
    );
    assert_eq!(doc, before);
    d.reset().unwrap();
    assert_eq!(d.entry_geometry(), d.work_geometry());
    assert_eq!(history.undo_len(), 0);
    assert_eq!(doc, before);
    let tiny = EditHistory::with_limits(100, 1024).unwrap();
    assert_eq!(
        tiny.begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
            .unwrap_err(),
        EditError::ResourceLimit
    );
}

#[test]
fn publication_barrier_refusal_preserves_work_history_generation_and_preview() {
    let doc = document(32);
    let history = EditHistory::default();
    let mut d = history
        .begin_manufacturing_draft(&doc, groups(), &mut || Ok(()))
        .unwrap();
    d.set_step(&history, &doc, d.generation(), step()).unwrap();
    let generation = d.generation();
    let work = d.work_geometry().to_vec();
    assert_eq!(
        d.preview_step(&history, &doc, &mut || Ok(()), &mut || Err(
            EditError::ResourceLimit
        )),
        Err(EditError::ResourceLimit)
    );
    assert!(d.preview_geometry().is_none());
    d.preview_step(&history, &doc, &mut || Ok(()), &mut || Ok(()))
        .unwrap();
    let preview = d.preview_geometry().unwrap().to_vec();
    assert_eq!(
        d.execute_step(&history, &doc, &mut || Ok(()), &mut || Err(
            EditError::ResourceLimit
        )),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d.generation(), generation);
    assert_eq!(d.work_geometry(), work);
    assert_eq!(d.preview_geometry().unwrap(), preview);
    assert_eq!(d.resources().unwrap().undo_entries, 0);
    d.execute_step(&history, &doc, &mut || Ok(()), &mut || Ok(()))
        .unwrap();
    assert_ne!(d.work_geometry(), work);
    assert_eq!(d.resources().unwrap().undo_entries, 1);
}
