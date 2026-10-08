//! Real parser/service calls with complete synthetic target sets; no window/GPU.
use editor_core::edit::{MAX_HISTORY_BYTES, MAX_MOVE_OBJECTS, MAX_MOVE_TARGETS};
use editor_core::{MmPoint, SemanticGeometry};
use editor_service::*;
use std::sync::atomic::{AtomicU64, Ordering};
fn run(n: usize, budget: usize) -> (ApplicationService, String, std::path::PathBuf) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rcam-move-demand-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("synthetic.gbr");
    let mut source = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\n");
    for i in 0..n {
        source.push_str(&format!("X{}Y0D02*X{}Y1000000D01*\n", i * 100, i * 100));
    }
    source.push_str("M02*\n");
    std::fs::write(&path, source).unwrap();
    let mut service = ApplicationService::with_file_access_and_history_limits(
        FileAccessPolicy::new(&dir, [dir.clone()], [dir.clone()]),
        100,
        budget,
    )
    .unwrap();
    let id = service.open(path.to_str().unwrap()).unwrap().document_id;
    (service, id, dir)
}
fn group(s: &ApplicationService, id: &str) -> SelectionGroup {
    let snapshot = s.render_snapshot(id).unwrap();
    SelectionGroup {
        layer_id: snapshot.layers[0].id.clone(),
        object_ids: snapshot.layers[0]
            .objects
            .iter()
            .map(|o| o.object_id.clone())
            .collect(),
    }
}
#[test]
fn eighty_thousand_service_move_returns_full_ids_one_revision_and_exact_history() {
    let (mut s, id, dir) = run(80_000, MAX_HISTORY_BYTES);
    let g = group(&s, &id);
    assert_eq!(g.object_ids.len(), 80_000);
    let capabilities = s.capabilities();
    assert_eq!(
        capabilities.resource_limits.max_move_objects,
        MAX_MOVE_TARGETS
    );
    assert_eq!(
        capabilities.resource_limits.max_edit_objects,
        MAX_MOVE_OBJECTS
    );
    let before_info = s.document_get(&id).unwrap();
    let before = s.render_snapshot(&id).unwrap();
    let demand = s
        .selection_move_demand(&id, &before_info.revision, std::slice::from_ref(&g))
        .unwrap();
    assert_eq!(s.document_get(&id).unwrap(), before_info);
    assert_eq!(s.render_snapshot(&id).unwrap(), before);
    assert_eq!(demand.object_count, 80_000);
    assert!(!demand.multi_layer_route);
    let edited = s
        .objects_move(
            &id,
            &before_info.revision,
            MoveParams {
                layer_id: g.layer_id,
                object_ids: g.object_ids.clone(),
                dx_mm: 1.,
                dy_mm: 2.,
            },
        )
        .unwrap();
    assert_eq!(edited.changed_object_ids, g.object_ids);
    assert_eq!(edited.undo_entries_added, 1);
    assert_eq!(edited.revision, "1");
    assert_eq!(edited.history_bytes, demand.history_bytes);
    let after = s.render_snapshot(&id).unwrap();
    for (a, b) in before.layers[0]
        .objects
        .iter()
        .zip(&after.layers[0].objects)
    {
        assert_eq!(a.object_id, b.object_id);
        assert_eq!(a.exposure, b.exposure);
        assert_eq!(a.origin, b.origin);
        let SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } = a.geometry
        else {
            panic!()
        };
        assert_eq!(
            b.geometry,
            SemanticGeometry::Line {
                start: MmPoint::new(start.x_mm + 1., start.y_mm + 2.),
                end: MmPoint::new(end.x_mm + 1., end.y_mm + 2.),
                width_mm
            }
        );
    }
    s.history_undo(&id, "1").unwrap();
    let restored = s.render_snapshot(&id).unwrap();
    assert_eq!(restored.layers[0].objects, before.layers[0].objects);
    s.history_redo(&id, "2").unwrap();
    let redone = s.render_snapshot(&id).unwrap();
    assert_eq!(redone.layers[0].objects, after.layers[0].objects);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn configured_budget_errors_match_preflight_and_commit_without_any_mutation() {
    let (mut s, id, dir) = run(20, 1);
    let g = group(&s, &id);
    let before = s.document_get(&id).unwrap();
    let scene = s.render_snapshot(&id).unwrap();
    let error = s
        .selection_move_demand(&id, &before.revision, std::slice::from_ref(&g))
        .unwrap_err();
    assert_eq!(error.code, "RESOURCE_LIMIT");
    assert_eq!(error.details["demand"]["history_limit_bytes"], 1);
    let commit_error = s
        .objects_move(
            &id,
            &before.revision,
            MoveParams {
                layer_id: g.layer_id,
                object_ids: g.object_ids,
                dx_mm: 1.,
                dy_mm: 1.,
            },
        )
        .unwrap_err();
    assert_eq!(error, commit_error);
    assert_eq!(s.document_get(&id).unwrap(), before);
    assert_eq!(s.render_snapshot(&id).unwrap(), scene);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_delta_precedes_budget_stale_revision_and_permissions_are_revalidated() {
    let (mut s, id, dir) = run(2, 1);
    let g = group(&s, &id);
    let before = s.document_get(&id).unwrap();
    for (x, y) in [(0., 0.), (f64::NAN, 0.), (1e10, 0.)] {
        let e = s
            .objects_move(
                &id,
                &before.revision,
                MoveParams {
                    layer_id: g.layer_id.clone(),
                    object_ids: g.object_ids.clone(),
                    dx_mm: x,
                    dy_mm: y,
                },
            )
            .unwrap_err();
        assert_eq!(e.code, "INVALID_ARGUMENT");
        assert_eq!(s.document_get(&id).unwrap(), before);
        let e = s
            .objects_edit_selection(
                &id,
                &before.revision,
                EditSelectionParams {
                    groups: vec![g.clone()],
                    operation: SelectionEdit::Move { dx_mm: x, dy_mm: y },
                },
            )
            .unwrap_err();
        assert_eq!(e.code, "INVALID_ARGUMENT");
        assert_eq!(s.document_get(&id).unwrap(), before);
    }
    std::fs::remove_dir_all(dir).unwrap();
    let (mut s, id, dir) = run(2, MAX_HISTORY_BYTES);
    let g = group(&s, &id);
    let before = s.document_get(&id).unwrap();
    s.selection_move_demand(&id, &before.revision, std::slice::from_ref(&g))
        .unwrap();
    s.layers_update_many(
        &id,
        &before.revision,
        UpdateLayersParams {
            expected_workspace_revision: before.workspace_revision,
            updates: vec![LayerPatch {
                layer_id: g.layer_id.clone(),
                locked: Some(true),
                ..Default::default()
            }],
        },
    )
    .unwrap();
    let e = s
        .objects_move(
            &id,
            &before.revision,
            MoveParams {
                layer_id: g.layer_id.clone(),
                object_ids: g.object_ids.clone(),
                dx_mm: 1.,
                dy_mm: 1.,
            },
        )
        .unwrap_err();
    assert_eq!(e.code, "LAYER_LOCKED");
    assert_eq!(s.document_get(&id).unwrap().undo_entries, 0);
    assert!(s.selection_move_demand(&id, "999", &[g]).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn malformed_repeated_groups_refuse_before_permissions_and_cancelled_probe_is_readonly() {
    let (mut s, id, dir) = run(2, MAX_HISTORY_BYTES);
    let g = group(&s, &id);
    let before = s.document_get(&id).unwrap();
    s.layers_update_many(
        &id,
        &before.revision,
        UpdateLayersParams {
            expected_workspace_revision: before.workspace_revision,
            updates: vec![LayerPatch {
                layer_id: g.layer_id.clone(),
                locked: Some(true),
                ..Default::default()
            }],
        },
    )
    .unwrap();
    let before = s.document_get(&id).unwrap();
    // Locked source would return LAYER_LOCKED if expensive permission scans ran.
    let repeated = vec![g.clone(); 10_000];
    assert_eq!(
        s.selection_move_demand(&id, &before.revision, &repeated)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        s.objects_edit_selection(
            &id,
            &before.revision,
            EditSelectionParams {
                groups: repeated,
                operation: SelectionEdit::Move {
                    dx_mm: 1.,
                    dy_mm: 1.
                }
            }
        )
        .unwrap_err()
        .code,
        "INVALID_ARGUMENT"
    );
    let cancel = task::CancellationToken::default();
    cancel.cancel();
    assert_eq!(
        s.selection_move_demand_with_cancel(&id, &before.revision, &[g], Some(&cancel))
            .unwrap_err()
            .code,
        "CANCELLED"
    );
    assert_eq!(s.document_get(&id).unwrap(), before);
    std::fs::remove_dir_all(dir).unwrap();
}
