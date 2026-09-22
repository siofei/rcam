//! S4-B1: whole-layer add/remove transactions, aperture identity and classification.
use editor_core::edit::{EditError, EditHistory, LayerAdd, LayerEffect};
use editor_core::workspace::{DisplayClass, aperture_shape_map, classify_object};
use editor_core::{
    ApertureDefinition, ApertureShape, Exposure, LocalTransform, MmPoint, ObjectOrigin,
    SemanticDocument, SemanticFormat, SemanticGeometry, SemanticLayer, SemanticObject,
    SourceMetadata,
};

fn format() -> SemanticFormat {
    SemanticFormat {
        integer: 3,
        decimal: 6,
        leading_zero_omission: true,
        absolute: true,
    }
}

fn empty_doc() -> SemanticDocument {
    SemanticDocument {
        id: "doc".into(),
        unit: "mm".into(),
        format: format(),
        layers: vec![],
        apertures: vec![],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}

fn aperture(namespace: &str, dcode: i32, diameter: f64) -> ApertureDefinition {
    ApertureDefinition {
        id: format!("{namespace}::aperture-{dcode}"),
        source_dcode: dcode,
        shape: ApertureShape::Circle {
            diameter_mm: diameter,
            hole_diameter_mm: None,
        },
    }
}

fn flash(id: &str, aperture_id: &str, x: f64) -> SemanticObject {
    SemanticObject {
        object_id: id.into(),
        geometry: SemanticGeometry::Flash {
            center: MmPoint::new(x, 0.),
            aperture_id: aperture_id.into(),
            transform: LocalTransform::default(),
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Imported { command_index: 0 },
    }
}

fn line(id: &str, x: f64) -> SemanticObject {
    SemanticObject {
        object_id: id.into(),
        geometry: SemanticGeometry::Line {
            start: MmPoint::new(x, 0.),
            end: MmPoint::new(x + 1., 0.),
            width_mm: 0.2,
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Imported { command_index: 1 },
    }
}

/// Layer `n` with one flash of its own namespaced D10 and one line.
fn source_layer(n: usize, diameter: f64) -> LayerAdd {
    let ns = format!("src-{n}");
    let ap = aperture(&ns, 10, diameter);
    LayerAdd {
        layer: SemanticLayer {
            id: format!("layer-{n}"),
            objects: vec![
                flash(&format!("{ns}-obj-1"), &ap.id, 0.),
                line(&format!("{ns}-obj-2"), 2.),
            ],
        },
        apertures: vec![ap],
    }
}

#[test]
fn same_dcode_in_different_sources_keeps_distinct_apertures() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    // A, B and C all define D10 with different diameters.
    history
        .add_layers(
            &mut doc,
            vec![
                source_layer(1, 0.5),
                source_layer(2, 1.0),
                source_layer(3, 1.5),
            ],
        )
        .unwrap();
    assert_eq!(doc.layers.len(), 3);
    assert_eq!(doc.apertures.len(), 3);
    assert!(doc.validate().is_ok());
    let diameters: Vec<f64> = doc
        .apertures
        .iter()
        .map(|a| match a.shape {
            ApertureShape::Circle { diameter_mm, .. } => diameter_mm,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(diameters, [0.5, 1.0, 1.5]);
    // The same (namespace, dcode) cannot be added twice.
    let clash = LayerAdd {
        layer: SemanticLayer {
            id: "layer-9".into(),
            objects: vec![],
        },
        apertures: vec![aperture("src-1", 10, 9.)],
    };
    assert!(history.add_layers(&mut doc, vec![clash]).is_err());
    assert_eq!(doc.layers.len(), 3);
    assert_eq!(
        history.undo_len(),
        1,
        "a rejected add leaves history untouched"
    );
}

#[test]
fn batch_add_is_one_undo_and_round_trips_exactly() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    let ids = history
        .add_layers(&mut doc, vec![source_layer(1, 0.5), source_layer(2, 1.0)])
        .unwrap();
    assert_eq!(ids, ["layer-1", "layer-2"]);
    let after_add = doc.clone();
    assert_eq!(history.undo_len(), 1);
    assert_eq!(
        history.peek_undo_layer_effect(),
        Some(LayerEffect::Removed(vec![
            "layer-1".into(),
            "layer-2".into()
        ]))
    );
    history.undo(&mut doc).unwrap();
    assert!(doc.layers.is_empty() && doc.apertures.is_empty());
    assert_eq!(
        history.peek_redo_layer_effect(),
        Some(LayerEffect::Added(vec!["layer-1".into(), "layer-2".into()]))
    );
    history.redo(&mut doc).unwrap();
    assert_eq!(doc, after_add, "redo restores the identical document");
}

#[test]
fn remove_layer_takes_only_its_own_apertures_and_undo_restores_position() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    history
        .add_layers(
            &mut doc,
            vec![
                source_layer(1, 0.5),
                source_layer(2, 1.0),
                source_layer(3, 1.5),
            ],
        )
        .unwrap();
    let before = doc.clone();
    history
        .remove_layer(&mut doc, "layer-2", Some("src-2"))
        .unwrap();
    assert_eq!(
        doc.layers.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
        ["layer-1", "layer-3"]
    );
    assert_eq!(doc.apertures.len(), 2);
    assert!(doc.apertures.iter().all(|a| !a.id.starts_with("src-2::")));
    assert!(doc.validate().is_ok());
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, before, "undo puts the layer back at the same position");
    history.redo(&mut doc).unwrap();
    assert_eq!(doc.layers.len(), 2);
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, before);
}

#[test]
fn removing_the_last_layer_is_allowed_and_undoable() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    history
        .add_layers(&mut doc, vec![source_layer(1, 0.5)])
        .unwrap();
    history
        .remove_layer(&mut doc, "layer-1", Some("src-1"))
        .unwrap();
    assert!(doc.layers.is_empty() && doc.apertures.is_empty());
    history.undo(&mut doc).unwrap();
    assert_eq!(doc.layers.len(), 1);
}

#[test]
fn unknown_layer_is_a_structured_error() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    assert!(matches!(
        history.remove_layer(&mut doc, "layer-404", None),
        Err(EditError::NotFound {
            entity: "layer",
            ..
        })
    ));
}

#[test]
fn object_edits_interleave_with_layer_transactions_lifo() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    history
        .add_layers(&mut doc, vec![source_layer(1, 0.5), source_layer(2, 1.0)])
        .unwrap();
    history
        .move_objects(&mut doc, "layer-1", &["src-1-obj-2".to_string()], 5., 0.)
        .unwrap();
    history
        .remove_layer(&mut doc, "layer-2", Some("src-2"))
        .unwrap();
    // Undo remove, undo move, undo add — each exactly one step.
    history.undo(&mut doc).unwrap();
    assert_eq!(doc.layers.len(), 2);
    history.undo(&mut doc).unwrap();
    match &doc.layers[0].objects[1].geometry {
        SemanticGeometry::Line { start, .. } => assert_eq!(start.x_mm, 2.),
        other => panic!("unexpected {other:?}"),
    }
    history.undo(&mut doc).unwrap();
    assert!(doc.layers.is_empty());
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 3);
}

#[test]
fn removing_a_layer_keeps_apertures_still_used_by_other_layers() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    // Two layers of ONE source share the same namespace and aperture.
    let ap = aperture("src-1", 10, 0.5);
    let a = LayerAdd {
        layer: SemanticLayer {
            id: "layer-1".into(),
            objects: vec![flash("src-1-obj-1", &ap.id, 0.)],
        },
        apertures: vec![ap.clone()],
    };
    let b = LayerAdd {
        layer: SemanticLayer {
            id: "layer-2".into(),
            objects: vec![flash("src-1-obj-2", &ap.id, 3.)],
        },
        apertures: vec![],
    };
    history.add_layers(&mut doc, vec![a, b]).unwrap();
    history.remove_layer(&mut doc, "layer-1", None).unwrap();
    assert_eq!(doc.apertures.len(), 1, "still used by layer-2");
    assert!(doc.validate().is_ok());
}

#[test]
fn classification_follows_origin_and_aperture_shape() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    history
        .add_layers(&mut doc, vec![source_layer(1, 0.5)])
        .unwrap();
    let shapes = aperture_shape_map(&doc.apertures);
    let flash_obj = &doc.layers[0].objects[0];
    assert_eq!(
        classify_object(flash_obj, &shapes),
        DisplayClass::FlashCircle
    );
    let stroke = &doc.layers[0].objects[1];
    assert_eq!(classify_object(stroke, &shapes), DisplayClass::Stroke);
    let mut text = line("t", 0.);
    text.origin = ObjectOrigin::GeneratedText {
        operation_id: "op".into(),
    };
    assert_eq!(classify_object(&text, &shapes), DisplayClass::GeneratedText);
}

#[test]
fn history_byte_budget_counts_layer_payload() {
    let mut doc = empty_doc();
    let mut history = EditHistory::default();
    history
        .add_layers(&mut doc, vec![source_layer(1, 0.5)])
        .unwrap();
    let resident = history.bytes();
    history
        .remove_layer(&mut doc, "layer-1", Some("src-1"))
        .unwrap();
    let held = history.bytes();
    assert!(
        held >= resident,
        "removed layer data is accounted for while held"
    );
    history.undo(&mut doc).unwrap();
    assert!(history.bytes() <= held);
}
