//! Synthetic real-service draft workflows; no GUI or private samples.
use editor_core::SemanticGeometry;
use editor_service::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Run {
    s: ApplicationService,
    id: String,
    dir: PathBuf,
    layer_order: Vec<String>,
}
impl Run {
    fn new() -> Self {
        Self::with_limits(100, 64 * 1024 * 1024)
    }
    fn with_limits(entries: usize, bytes: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-i1-service-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = ApplicationService::with_file_access_and_history_limits(
            FileAccessPolicy::new(dir.clone(), [dir.clone()], [dir.clone()]),
            entries,
            bytes,
        )
        .unwrap();
        let a = dir.join("lower.gbr");
        let b = dir.join("upper.gbr");
        std::fs::write(&a,b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX2000000Y3000000D03*\nX6000000Y3000000D03*\n%LPC*%\nX6000000Y3000000D03*\nM02*\n").unwrap();
        std::fs::write(&b,b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG75*\nX1000000Y0D02*\nG03X0Y1000000I-1000000J0D01*\nM02*\n").unwrap();
        let info = s.document_new().unwrap();
        let id = info.document_id;
        let imported = s
            .import_gerber_layers(
                &id,
                &info.revision,
                ImportGerberLayersParams {
                    paths: vec![a.to_string_lossy().into(), b.to_string_lossy().into()],
                },
            )
            .unwrap();
        let layer_order = imported.layers.into_iter().map(|l| l.layer_id).collect();
        Self {
            s,
            id,
            dir,
            layer_order,
        }
    }
    fn info(&self) -> DocumentInfo {
        self.s.document_get(&self.id).unwrap()
    }
    fn scene(&self) -> RenderSnapshot {
        let mut scene = self.s.render_snapshot(&self.id).unwrap();
        scene
            .layers
            .sort_by_key(|l| self.layer_order.iter().position(|id| id == &l.id).unwrap());
        scene
    }
    fn groups(&self) -> Vec<SelectionGroup> {
        self.scene()
            .layers
            .iter()
            .map(|l| SelectionGroup {
                layer_id: l.id.clone(),
                object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
            })
            .collect()
    }
    fn edit(
        &mut self,
        groups: Vec<SelectionGroup>,
        operation: SelectionEdit,
    ) -> Result<EditSelectionResult, ServiceError> {
        let rev = self.info().revision;
        self.s
            .objects_edit_selection(&self.id, &rev, EditSelectionParams { groups, operation })
    }
    fn draft(&self) -> UnifiedEditorSession {
        let info = self.info();
        self.s
            .unified_editor_begin(
                &self.id,
                &info.revision,
                &info.workspace_revision,
                self.groups(),
                None,
            )
            .unwrap()
    }
    fn set(
        &self,
        d: &mut UnifiedEditorSession,
        groups: Vec<SelectionGroup>,
        op: SelectionEdit,
    ) -> u64 {
        self.s
            .unified_editor_set_step(
                d,
                d.generation().unwrap(),
                DraftStep {
                    groups,
                    operation: op,
                },
            )
            .unwrap()
    }
    fn apply(
        &mut self,
        d: &mut UnifiedEditorSession,
    ) -> Result<UnifiedEditorApplyResult, ServiceError> {
        let ticket = self.s.unified_editor_begin_apply(d)?;
        self.s.unified_editor_complete_apply(d, ticket, None)
    }
    fn unchanged(&self, info: &DocumentInfo, scene: &RenderSnapshot) {
        assert_eq!(&self.info(), info);
        let now = self.scene();
        assert_eq!(now.layers, scene.layers);
        assert_eq!(now.apertures, scene.apertures);
        assert_eq!(now.block_definitions, scene.block_definitions);
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn move_by(x: f64) -> SelectionEdit {
    SelectionEdit::Move {
        dx_mm: x,
        dy_mm: 0.,
    }
}
fn first_x(d: &UnifiedEditorSession) -> f64 {
    match &d.work_geometry().unwrap()[0] {
        SemanticGeometry::Flash { center, .. } => center.x_mm,
        _ => panic!(),
    }
}
#[test]
fn repeated_preview_is_fixed_step_and_execution_accumulates_exact_history() {
    let r = Run::new();
    let groups = r.groups();
    let info = r.info();
    let scene = r.scene();
    let mut d = r.draft();
    let entry = d.entry_geometry().unwrap().to_vec();
    let generation = r.set(&mut d, groups.clone(), move_by(0.1));
    for _ in 0..3 {
        r.s.unified_editor_preview(&mut d, generation, None)
            .unwrap();
    }
    assert_eq!(first_x(&d), 2.);
    let gen2 =
        r.s.unified_editor_execute(&mut d, generation, None)
            .unwrap();
    assert!(
        r.s.unified_editor_execute(&mut d, generation, None)
            .is_err()
    );
    assert!(
        r.s.unified_editor_preview(&mut d, generation, None)
            .is_err()
    );
    assert_eq!(d.generation().unwrap(), gen2);
    let generation = r.set(&mut d, groups, move_by(0.2));
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    let final_work = d.work_geometry().unwrap().to_vec();
    assert!((first_x(&d) - 2.3).abs() < 1e-9);
    assert_eq!(d.entry_geometry().unwrap(), entry);
    r.s.unified_editor_undo(&mut d).unwrap();
    assert_eq!(first_x(&d), 2.1);
    r.s.unified_editor_redo(&mut d).unwrap();
    assert_eq!(d.work_geometry().unwrap(), final_work);
    d.reset().unwrap();
    assert!(!d.is_closed());
    assert_eq!(d.work_geometry().unwrap(), entry);
    assert_eq!(d.resources().unwrap().undo_entries, 0);
    assert_eq!(d.resources().unwrap().redo_entries, 0);
    assert!(d.preview_geometry().unwrap().is_none());
    r.unchanged(&info, &scene);
    d.cancel();
    assert!(d.is_closed());
    r.unchanged(&info, &scene);
}
#[test]
fn unpreviewed_latest_apply_is_one_cross_layer_transaction_and_exact_main_undo_redo() {
    let mut r = Run::new();
    let groups = r.groups();
    let info = r.info();
    let scene = r.scene();
    let mut d = r.draft();
    let generation = r.set(&mut d, groups.clone(), move_by(0.1));
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    r.set(&mut d, groups, move_by(0.2));
    assert!(d.preview_geometry().unwrap().is_none());
    let applied = r.apply(&mut d).unwrap();
    assert!(applied.changed);
    assert!(d.is_closed());
    assert_eq!(applied.edit.undo_entries_added, 1);
    assert_eq!(r.info().undo_entries, info.undo_entries + 1);
    assert_eq!(
        r.info().revision.parse::<u64>().unwrap(),
        info.revision.parse::<u64>().unwrap() + 1
    );
    let after = r.scene();
    for (a, b) in scene.layers.iter().zip(&after.layers) {
        for (a, b) in a.objects.iter().zip(&b.objects) {
            assert_eq!(a.object_id, b.object_id);
            assert_eq!(a.exposure, b.exposure);
            assert_eq!(
                b.geometry,
                move_by(0.1)
                    .preview_geometry(&a.geometry)
                    .and_then(|g| move_by(0.2).preview_geometry(&g))
                    .unwrap()
            );
        }
    }
    assert!(r.apply(&mut d).is_err());
    r.s.history_undo(&r.id, &r.info().revision).unwrap();
    assert_eq!(r.scene().layers, scene.layers);
    r.s.history_redo(&r.id, &r.info().revision).unwrap();
    assert_eq!(r.scene().layers, after.layers);
}
#[test]
fn pending_ticket_blocks_repeats_and_reset_cancel_invalidate_old_completion() {
    let mut r = Run::new();
    let mut d = r.draft();
    let info = r.info();
    let scene = r.scene();
    r.set(&mut d, r.groups(), move_by(1.));
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    assert!(d.is_apply_pending());
    assert!(r.s.unified_editor_begin_apply(&mut d).is_err());
    let generation = d.generation().unwrap();
    assert!(
        r.s.unified_editor_set_step(
            &mut d,
            generation,
            DraftStep {
                groups: r.groups(),
                operation: move_by(2.)
            }
        )
        .is_err()
    );
    assert!(
        r.s.unified_editor_execute(&mut d, generation, None)
            .is_err()
    );
    assert!(r.s.unified_editor_undo(&mut d).is_err());
    d.reset().unwrap();
    let current = r.s.unified_editor_begin_apply(&mut d).unwrap();
    assert!(
        r.s.unified_editor_complete_apply(&mut d, ticket, None)
            .is_err()
    );
    assert!(d.is_apply_pending());
    let out =
        r.s.unified_editor_complete_apply(&mut d, current, None)
            .unwrap();
    assert!(!out.changed);
    assert!(d.is_closed());
    r.unchanged(&info, &scene);
    let mut d = r.draft();
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    d.cancel();
    assert!(
        r.s.unified_editor_complete_apply(&mut d, ticket, None)
            .is_err()
    );
    r.unchanged(&info, &scene);
}
#[test]
fn exact_no_change_closes_without_dirty_revision_or_main_redo_mutation() {
    let mut r = Run::new();
    r.edit(r.groups(), move_by(1.)).unwrap();
    r.s.history_undo(&r.id, &r.info().revision).unwrap();
    let info = r.info();
    let scene = r.scene();
    assert!(info.redo_entries > 0);
    let mut d = r.draft();
    let generation = r.set(
        &mut d,
        r.groups(),
        SelectionEdit::Mirror {
            axis: MirrorAxis::Vertical { coordinate_mm: 0. },
        },
    );
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    r.set(
        &mut d,
        r.groups(),
        SelectionEdit::Mirror {
            axis: MirrorAxis::Vertical { coordinate_mm: 0. },
        },
    );
    let out = r.apply(&mut d).unwrap();
    assert!(!out.changed);
    assert_eq!(out.edit.undo_entries_added, 0);
    assert!(d.is_closed());
    r.unchanged(&info, &scene);
    r.s.history_redo(&r.id, &r.info().revision).unwrap();
}
#[test]
fn newest_invalid_input_never_commits_previous_valid_preview() {
    let mut r = Run::new();
    let mut d = r.draft();
    let info = r.info();
    let scene = r.scene();
    let generation = r.set(&mut d, r.groups(), move_by(1.));
    r.s.unified_editor_preview(&mut d, generation, None)
        .unwrap();
    assert!(
        r.s.unified_editor_set_step(
            &mut d,
            generation,
            DraftStep {
                groups: r.groups(),
                operation: move_by(f64::NAN)
            }
        )
        .is_err()
    );
    assert!(r.apply(&mut d).is_err());
    assert!(!d.is_closed());
    assert_eq!(first_x(&d), 2.);
    r.unchanged(&info, &scene);
    r.set(&mut d, r.groups(), move_by(2.));
    r.apply(&mut d).unwrap();
}
#[test]
fn different_step_subsets_and_foreign_targets_are_not_global_all_selected() {
    let mut r = Run::new();
    let groups = r.groups();
    let mut d = r.draft();
    let generation = r.set(&mut d, vec![groups[0].clone()], move_by(1.));
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    r.set(&mut d, vec![groups[1].clone()], move_by(2.));
    r.apply(&mut d).unwrap();
    match &r.scene().layers[0].objects[0].geometry {
        SemanticGeometry::Flash { center, .. } => assert_eq!(center.x_mm, 3.),
        _ => panic!(),
    };
    let mut d =
        r.s.unified_editor_begin(
            &r.id,
            &r.info().revision,
            &r.info().workspace_revision,
            vec![groups[0].clone()],
            None,
        )
        .unwrap();
    assert!(
        r.s.unified_editor_set_step(
            &mut d,
            0,
            DraftStep {
                groups: vec![groups[1].clone()],
                operation: move_by(1.)
            }
        )
        .is_err()
    );
    assert!(r.apply(&mut d).is_err());
}
#[test]
fn stale_revision_and_foreign_service_fail_without_publishing_work() {
    let mut r = Run::new();
    let mut d = r.draft();
    r.set(&mut d, r.groups(), move_by(1.));
    let other = Run::new();
    assert_eq!(r.id, other.id);
    assert!(other.s.unified_editor_begin_apply(&mut d).is_err());
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    r.edit(r.groups(), move_by(3.)).unwrap();
    let info = r.info();
    let scene = r.scene();
    assert!(
        r.s.unified_editor_complete_apply(&mut d, ticket, None)
            .is_err()
    );
    assert!(!d.is_closed());
    assert!(!d.is_apply_pending());
    r.unchanged(&info, &scene);
    d.reset().unwrap();
    assert_eq!(first_x(&d), 2.);
    d.cancel();
}
#[test]
fn cancellation_before_apply_preserves_draft_and_main_state() {
    let mut r = Run::new();
    let mut d = r.draft();
    let info = r.info();
    let scene = r.scene();
    r.set(&mut d, r.groups(), move_by(1.));
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let token = task::CancellationToken::default();
    token.start().unwrap();
    token.cancel();
    let error =
        r.s.unified_editor_complete_apply(&mut d, ticket, Some(&token))
            .unwrap_err();
    assert_eq!(error.code, "CANCELLED");
    assert!(!d.is_closed());
    assert!(!d.is_apply_pending());
    assert_eq!(first_x(&d), 2.);
    r.unchanged(&info, &scene);
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let token = task::CancellationToken::default();
    token.start().unwrap();
    assert!(
        r.s.unified_editor_complete_apply(&mut d, ticket, Some(&token))
            .unwrap()
            .changed
    );
    assert_eq!(token.state(), task::TaskState::Committing);
}

#[test]
fn history_budget_refusal_keeps_work_reference_and_main_state() {
    let r = Run::with_limits(2, 64 * 1024 * 1024);
    let info = r.info();
    let scene = r.scene();
    let mut d = r.draft();
    for _ in 0..2 {
        let generation = r.set(&mut d, r.groups(), move_by(1.));
        r.s.unified_editor_execute(&mut d, generation, None)
            .unwrap();
    }
    let work = d.work_geometry().unwrap().to_vec();
    let reference = d.entry_geometry().unwrap().to_vec();
    let generation = r.set(&mut d, r.groups(), move_by(2.));
    assert_eq!(
        r.s.unified_editor_execute(&mut d, generation, None)
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(d.work_geometry().unwrap(), work);
    assert_eq!(d.entry_geometry().unwrap(), reference);
    assert_eq!(d.resources().unwrap().undo_entries, 2);
    r.unchanged(&info, &scene);
    d.reset().unwrap();
    assert_eq!(d.work_geometry().unwrap(), reference);
}
#[test]
fn workspace_permission_changes_and_document_close_invalidate_pending_apply() {
    let mut r = Run::new();
    let mut d = r.draft();
    r.set(&mut d, r.groups(), move_by(1.));
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let info = r.info();
    r.s.layers_update_many(
        &r.id,
        &info.revision,
        UpdateLayersParams {
            expected_workspace_revision: info.workspace_revision,
            updates: vec![LayerPatch {
                layer_id: r.groups()[1].layer_id.clone(),
                locked: Some(true),
                ..Default::default()
            }],
        },
    )
    .unwrap();
    let info = r.info();
    let scene = r.scene();
    assert!(
        r.s.unified_editor_complete_apply(&mut d, ticket, None)
            .is_err()
    );
    r.unchanged(&info, &scene);
    assert!(!d.is_closed());
    d.cancel();
    // A closed document cannot be substituted by a subsequently opened one.
    let mut r = Run::new();
    let mut d = r.draft();
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let old = r.id.clone();
    r.s.close(&r.id, &r.info().revision, true).unwrap();
    let next = r.s.document_new().unwrap();
    assert_ne!(next.document_id, old);
    assert!(
        r.s.unified_editor_complete_apply(&mut d, ticket, None)
            .is_err()
    );
    assert!(!d.is_closed());
    d.cancel();
}
#[test]
fn latest_late_layer_unsupported_transform_rolls_back_all_work_and_main() {
    let mut r = Run::new();
    let info = r.info();
    let groups = r.groups();
    // Rectangular aperture sweep is imported as a true manufacturing sweep; arbitrary rotation is unsupported.
    let path = r.dir.join("sweep.gbr");
    std::fs::write(
        &path,
        b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10R,1X2*%\nD10*\nX0Y0D02*\nX2000000Y0D01*\nM02*\n",
    )
    .unwrap();
    r.s.import_gerber_layers(
        &r.id,
        &info.revision,
        ImportGerberLayersParams {
            paths: vec![path.to_string_lossy().into()],
        },
    )
    .unwrap();
    let info = r.info();
    let scene = r.s.render_snapshot(&r.id).unwrap();
    let sweep = scene
        .layers
        .iter()
        .find(|l| !groups.iter().any(|g| g.layer_id == l.id))
        .unwrap();
    let mut all = groups;
    all.push(SelectionGroup {
        layer_id: sweep.id.clone(),
        object_ids: sweep.objects.iter().map(|o| o.object_id.clone()).collect(),
    });
    let mut d =
        r.s.unified_editor_begin(
            &r.id,
            &info.revision,
            &info.workspace_revision,
            all.clone(),
            None,
        )
        .unwrap();
    let work = d.work_geometry().unwrap().to_vec();
    r.set(
        &mut d,
        all,
        SelectionEdit::Rotate {
            angle_deg: 30.,
            pivot_mm: editor_core::MmPoint::new(0., 0.),
        },
    );
    let error = r.apply(&mut d).unwrap_err();
    assert_eq!(error.code, "UNSUPPORTED_FEATURE");
    assert!(!d.is_closed());
    assert!(!d.is_apply_pending());
    assert_eq!(d.work_geometry().unwrap(), work);
    assert_eq!(r.info(), info);
    assert_eq!(r.s.render_snapshot(&r.id).unwrap(), scene);
}

#[test]
fn synthetic_worker_snapshot_history_and_apply_peak_measurement() {
    std::thread::spawn(|| {
        let mut r = Run::new();
        let path = r.dir.join("synthetic-region.gbr");
        let mut source =
            String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\nG36*\nX10000000Y0D02*\n");
        for i in 1..=4096 {
            let a = std::f64::consts::TAU * (i % 4096) as f64 / 4096.;
            let x = (10_000_000. * a.cos()).round() as i64;
            let y = (10_000_000. * a.sin()).round() as i64;
            source.push_str(&format!("X{x}Y{y}D01*\n"));
        }
        source.push_str("G37*\nM02*\n");
        std::fs::write(&path, source).unwrap();
        r.s.import_gerber_layers(
            &r.id,
            &r.info().revision,
            ImportGerberLayersParams {
                paths: vec![path.to_string_lossy().into()],
            },
        )
        .unwrap();
        let scene = r.s.render_snapshot(&r.id).unwrap();
        let groups: Vec<_> = scene
            .layers
            .iter()
            .map(|l| SelectionGroup {
                layer_id: l.id.clone(),
                object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
            })
            .collect();
        drop(scene);
        let info = r.info();
        let mut d =
            r.s.unified_editor_begin(
                &r.id,
                &info.revision,
                &info.workspace_revision,
                groups.clone(),
                None,
            )
            .unwrap();
        println!("WORKER_MEASUREMENT entry {:?}", d.resources().unwrap());
        for _ in 0..64 {
            let generation = r.set(&mut d, groups.clone(), move_by(0.01));
            r.s.unified_editor_execute(&mut d, generation, None)
                .unwrap();
        }
        println!("WORKER_MEASUREMENT history64 {:?}", d.resources().unwrap());
        let generation = r.set(&mut d, groups, move_by(0.01));
        r.s.unified_editor_preview(&mut d, generation, None)
            .unwrap();
        println!(
            "WORKER_MEASUREMENT history64_preview {:?}",
            d.resources().unwrap()
        );
        assert!(r.apply(&mut d).unwrap().changed);
        assert!(d.is_closed());
        assert_eq!(r.info().undo_entries, info.undo_entries + 1);
        println!("WORKER_MEASUREMENT final_undo {}", r.info().undo_entries);
    })
    .join()
    .unwrap();
}

#[test]
fn real_precision_setter_fences_draft_even_without_geometry_edit() {
    let mut r = Run::new();
    let mut d = r.draft();
    r.set(&mut d, r.groups(), move_by(1.));
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    r.s.set_manufacturing_precision(
        &r.id,
        &r.info().revision,
        ManufacturingPrecision {
            resolution_mm: 0.00001,
        },
    )
    .unwrap();
    let info = r.info();
    let scene = r.scene();
    assert!(
        r.s.unified_editor_complete_apply(&mut d, ticket, None)
            .is_err()
    );
    assert!(!d.is_closed());
    r.unchanged(&info, &scene);
}

#[test]
fn noncommuting_sequence_has_independent_coordinates_and_arc_direction() {
    let mut r = Run::new();
    let mut d = r.draft();
    let groups = r.groups();
    let scene = r.scene();
    for op in [
        move_by(1.),
        SelectionEdit::Rotate {
            angle_deg: 90.,
            pivot_mm: editor_core::MmPoint::new(0., 0.),
        },
        SelectionEdit::Mirror {
            axis: MirrorAxis::Vertical { coordinate_mm: 0. },
        },
    ] {
        let generation = r.set(&mut d, groups.clone(), op);
        r.s.unified_editor_execute(&mut d, generation, None)
            .unwrap();
    }
    match &d.work_geometry().unwrap()[0] {
        SemanticGeometry::Flash { center, .. } => {
            assert_eq!(*center, editor_core::MmPoint::new(3., 3.))
        }
        _ => panic!(),
    };
    match &d.work_geometry().unwrap()[3] {
        SemanticGeometry::Arc { path, .. } => {
            assert_eq!(path.start, editor_core::MmPoint::new(0., 2.));
            assert_eq!(path.end, editor_core::MmPoint::new(1., 1.));
            assert_eq!(path.center, editor_core::MmPoint::new(0., 1.));
            assert_eq!(path.direction, editor_core::ArcDirection::Clockwise);
            match &scene.layers[1].objects[0].geometry {
                SemanticGeometry::Arc { path: old, .. } => assert_eq!(path.source, old.source),
                _ => panic!(),
            };
        }
        _ => panic!(),
    };
    r.apply(&mut d).unwrap();
    r.s.history_undo(&r.id, &r.info().revision).unwrap();
    assert_eq!(r.scene().layers, scene.layers);
}
#[test]
fn apply_reports_only_exact_changed_subset_ids() {
    let mut r = Run::new();
    let groups = r.groups();
    let mut d = r.draft();
    r.set(&mut d, vec![groups[0].clone()], move_by(1.));
    let out = r.apply(&mut d).unwrap();
    let mut actual = out.edit.changed_object_ids;
    actual.sort();
    let mut expected = groups[0].object_ids.clone();
    expected.sort();
    assert_eq!(actual, expected);
}

fn preparation_refusal() -> ServiceError {
    ServiceError {
        code: "DISPLAY_PREPARE_REFUSED".into(),
        message: "synthetic prospective artifact refusal".into(),
        details: serde_json::json!({"artifact":"scene-and-snap"}),
    }
}
#[test]
fn prospective_candidate_maps_exact_original_slots_with_reversed_groups() {
    let mut r = Run::new();
    let source = r.scene();
    let mut groups = r.groups();
    groups.reverse();
    for group in &mut groups {
        group.object_ids.reverse();
    }
    let info = r.info();
    let mut d =
        r.s.unified_editor_begin(
            &r.id,
            &info.revision,
            &info.workspace_revision,
            groups.clone(),
            None,
        )
        .unwrap();
    let entry: Vec<_> = d
        .work_candidate()
        .unwrap()
        .objects()
        .map(|(l, o, g)| (l.to_owned(), o.to_owned(), g.clone()))
        .collect();
    for (l, id, geometry) in &entry {
        let object = source
            .layers
            .iter()
            .find(|s| &s.id == l)
            .unwrap()
            .objects
            .iter()
            .find(|o| &o.object_id == id)
            .unwrap();
        assert_eq!(&object.geometry, geometry);
    }
    let op = move_by(7.);
    let generation = r.set(&mut d, groups, op.clone());
    let mut calls = 0;
    let token = task::CancellationToken::default();
    token.start().unwrap();
    r.s.unified_editor_execute_prepared(&mut d, generation, Some(&token), &mut |candidate| {
        calls += 1;
        assert_eq!(token.state(), task::TaskState::Running);
        assert_eq!(candidate.objects().len(), entry.len());
        for ((l, id, g), (before_l, before_id, before)) in candidate.objects().zip(&entry) {
            assert_eq!((l, id), (before_l.as_str(), before_id.as_str()));
            assert_eq!(g, &op.preview_geometry(before).unwrap());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(token.cancel(), task::CancelOutcome::TooLate);
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let mut final_candidate = Vec::new();
    r.s.unified_editor_complete_apply_prepared(&mut d, ticket, None, &mut |candidate| {
        final_candidate = candidate
            .objects()
            .map(|(l, o, g)| (l.to_owned(), o.to_owned(), g.clone()))
            .collect();
        Ok(())
    })
    .unwrap();
    let final_scene = r.scene();
    for (before, after) in source.layers.iter().zip(&final_scene.layers) {
        assert_eq!(before.id, after.id);
        for (old, new) in before.objects.iter().zip(&after.objects) {
            assert_eq!(old.object_id, new.object_id);
            assert_eq!(old.exposure, new.exposure);
            assert_eq!(old.origin, new.origin);
            let geometry = &final_candidate
                .iter()
                .find(|(l, o, _)| l == &after.id && o == &new.object_id)
                .unwrap()
                .2;
            assert_eq!(&new.geometry, geometry);
        }
    }
    assert_eq!(final_scene.apertures, source.apertures);
    assert_eq!(final_scene.block_definitions, source.block_definitions);
    assert_eq!(r.info().undo_entries, info.undo_entries + 1);
    r.s.history_undo(&r.id, &r.info().revision).unwrap();
    assert_eq!(r.scene().layers, source.layers);
}
#[test]
fn prepare_preview_and_execute_failure_preserves_last_artifacts_work_and_input() {
    let r = Run::new();
    let mut d = r.draft();
    let generation = r.set(&mut d, r.groups(), move_by(1.));
    r.s.unified_editor_preview(&mut d, generation, None)
        .unwrap();
    let preview = d.preview_geometry().unwrap().unwrap().to_vec();
    let work = d.work_geometry().unwrap().to_vec();
    let main = r.info();
    let scene = r.scene();
    for execute in [false, true] {
        let token = task::CancellationToken::default();
        token.start().unwrap();
        let err = if execute {
            r.s.unified_editor_execute_prepared(&mut d, generation, Some(&token), &mut |_| {
                Err(preparation_refusal())
            })
            .map(|_| ())
        } else {
            r.s.unified_editor_preview_prepared(&mut d, generation, Some(&token), &mut |_| {
                Err(preparation_refusal())
            })
        }
        .unwrap_err();
        assert_eq!(err, preparation_refusal());
        assert_eq!(token.state(), task::TaskState::Running);
        assert_eq!(d.generation().unwrap(), generation);
        assert_eq!(d.work_geometry().unwrap(), work);
        assert_eq!(d.preview_geometry().unwrap().unwrap(), preview);
        assert_eq!(d.resources().unwrap().undo_entries, 0);
        r.unchanged(&main, &scene);
    }
    // Refused artifacts have not lost the accepted newest parameters.
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    assert_ne!(d.work_geometry().unwrap(), work);
}
#[test]
fn prepared_exact_history_and_reset_failure_preserve_checkpoints() {
    let r = Run::new();
    let mut d = r.draft();
    let entry = d.entry_geometry().unwrap().to_vec();
    let main = r.info();
    let source = r.scene();
    let generation = r.set(&mut d, r.groups(), move_by(4.));
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    let moved = d.work_geometry().unwrap().to_vec();
    for action in [UnifiedEditorHistory::Undo, UnifiedEditorHistory::Reset] {
        let generation = d.generation().unwrap();
        let undo = d.resources().unwrap().undo_entries;
        let err =
            r.s.unified_editor_history_prepared(&mut d, action, None, &mut |candidate| {
                assert!(candidate.objects().map(|(_, _, g)| g).eq(entry.iter()));
                Err(preparation_refusal())
            })
            .unwrap_err();
        assert_eq!(err, preparation_refusal());
        assert_eq!(d.generation().unwrap(), generation);
        assert_eq!(d.work_geometry().unwrap(), moved);
        assert_eq!(d.resources().unwrap().undo_entries, undo);
    }
    r.s.unified_editor_history_prepared(
        &mut d,
        UnifiedEditorHistory::Undo,
        None,
        &mut |candidate| {
            assert!(candidate.objects().map(|(_, _, g)| g).eq(entry.iter()));
            Ok(())
        },
    )
    .unwrap();
    let generation = d.generation().unwrap();
    let err =
        r.s.unified_editor_history_prepared(
            &mut d,
            UnifiedEditorHistory::Redo,
            None,
            &mut |candidate| {
                assert!(candidate.objects().map(|(_, _, g)| g).eq(moved.iter()));
                Err(preparation_refusal())
            },
        )
        .unwrap_err();
    assert_eq!(err, preparation_refusal());
    assert_eq!(d.generation().unwrap(), generation);
    assert_eq!(d.work_geometry().unwrap(), entry);
    assert_eq!(d.resources().unwrap().redo_entries, 1);
    r.s.unified_editor_history_prepared(&mut d, UnifiedEditorHistory::Redo, None, &mut |_| Ok(()))
        .unwrap();
    assert_eq!(d.work_geometry().unwrap(), moved);
    r.set(&mut d, r.groups(), move_by(8.));
    r.s.unified_editor_history_prepared(&mut d, UnifiedEditorHistory::Reset, None, &mut |_| Ok(()))
        .unwrap();
    assert_eq!(d.work_geometry().unwrap(), entry);
    assert_eq!(
        (
            d.resources().unwrap().undo_entries,
            d.resources().unwrap().redo_entries
        ),
        (0, 0)
    );
    let mut r = r;
    assert!(!r.apply(&mut d).unwrap().changed); // Reset cleared unexecuted +8 input.
    r.unchanged(&main, &source);
}
#[test]
fn cancellation_during_preparation_keeps_work_history_and_main_on_all_paths() {
    let mut r = Run::new();
    let source = r.scene();
    let main = r.info();
    for action in 0..6 {
        let mut d = r.draft();
        let generation = r.set(&mut d, r.groups(), move_by(2.));
        if (2..=4).contains(&action) {
            r.s.unified_editor_execute(&mut d, generation, None)
                .unwrap();
            if action == 3 {
                r.s.unified_editor_undo(&mut d).unwrap();
            }
        }
        let work = d.work_geometry().unwrap().to_vec();
        let resources = d.resources().unwrap();
        let generation = d.generation().unwrap();
        let token = task::CancellationToken::default();
        token.start().unwrap();
        let mut calls = 0;
        let mut prepare = |_: DraftCandidate<'_>| {
            calls += 1;
            token.cancel();
            Ok(())
        };
        let result = match action {
            0 => {
                r.s.unified_editor_preview_prepared(&mut d, generation, Some(&token), &mut prepare)
            }
            1 => {
                r.s.unified_editor_execute_prepared(&mut d, generation, Some(&token), &mut prepare)
                    .map(|_| ())
            }
            2 => {
                r.s.unified_editor_history_prepared(
                    &mut d,
                    UnifiedEditorHistory::Undo,
                    Some(&token),
                    &mut prepare,
                )
                .map(|_| ())
            }
            3 => {
                r.s.unified_editor_history_prepared(
                    &mut d,
                    UnifiedEditorHistory::Redo,
                    Some(&token),
                    &mut prepare,
                )
                .map(|_| ())
            }
            4 => {
                r.s.unified_editor_history_prepared(
                    &mut d,
                    UnifiedEditorHistory::Reset,
                    Some(&token),
                    &mut prepare,
                )
                .map(|_| ())
            }
            _ => {
                let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
                r.s.unified_editor_complete_apply_prepared(
                    &mut d,
                    ticket,
                    Some(&token),
                    &mut prepare,
                )
                .map(|_| ())
            }
        };
        assert_eq!(result.unwrap_err().code, "CANCELLED");
        assert_eq!(calls, 1);
        assert_eq!(d.work_geometry().unwrap(), work);
        assert_eq!(d.generation().unwrap(), generation);
        assert_eq!(
            (
                d.resources().unwrap().undo_entries,
                d.resources().unwrap().redo_entries
            ),
            (resources.undo_entries, resources.redo_entries)
        );
        assert!(!d.is_apply_pending());
        assert!(!d.is_closed());
        r.unchanged(&main, &source);
    }
}
#[test]
fn prepare_apply_failure_is_retryable_without_repeating_the_transform() {
    let mut r = Run::new();
    let mut d = r.draft();
    let source = r.scene();
    let main = r.info();
    let generation = r.set(&mut d, r.groups(), move_by(3.));
    let entry = d.work_geometry().unwrap().to_vec();
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let mut prepared = Vec::new();
    let err =
        r.s.unified_editor_complete_apply_prepared(&mut d, ticket, None, &mut |candidate| {
            prepared = candidate.objects().map(|(_, _, g)| g.clone()).collect();
            Err(preparation_refusal())
        })
        .unwrap_err();
    assert_eq!(err, preparation_refusal());
    assert!(!d.is_apply_pending());
    assert!(!d.is_closed());
    assert_eq!(d.generation().unwrap(), generation);
    assert_eq!(d.work_geometry().unwrap(), entry);
    r.unchanged(&main, &source);
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let mut calls = 0;
    let token = task::CancellationToken::default();
    token.start().unwrap();
    assert!(
        r.s.unified_editor_complete_apply_prepared(
            &mut d,
            ticket,
            Some(&token),
            &mut |candidate| {
                calls += 1;
                assert_eq!(token.state(), task::TaskState::Running);
                assert!(candidate.objects().map(|(_, _, g)| g).eq(prepared.iter()));
                Ok(())
            }
        )
        .unwrap()
        .changed
    );
    assert_eq!(calls, 1);
    assert_eq!(token.cancel(), task::CancelOutcome::TooLate);
    assert!(d.is_closed());
    assert_eq!(r.info().undo_entries, main.undo_entries + 1);
}
#[test]
fn prepare_nochange_failure_and_success_preserve_main_redo_and_revision() {
    let mut r = Run::new();
    r.edit(r.groups(), move_by(9.)).unwrap();
    r.s.history_undo(&r.id, &r.info().revision).unwrap();
    let source = r.scene();
    let main = r.info();
    let mut d = r.draft();
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    assert_eq!(
        r.s.unified_editor_complete_apply_prepared(&mut d, ticket, None, &mut |_| Err(
            preparation_refusal()
        ))
        .unwrap_err(),
        preparation_refusal()
    );
    assert!(!d.is_closed());
    r.unchanged(&main, &source);
    let ticket = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let mut calls = 0;
    assert!(
        !r.s.unified_editor_complete_apply_prepared(&mut d, ticket, None, &mut |_| {
            calls += 1;
            Ok(())
        })
        .unwrap()
        .changed
    );
    assert_eq!(calls, 1);
    assert!(d.is_closed());
    r.unchanged(&main, &source);
}
#[test]
fn prepared_checkpoint_publication_barrier_makes_late_cancel_too_late() {
    let r = Run::new();
    let mut d = r.draft();
    let generation = r.set(&mut d, r.groups(), move_by(5.));
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    for action in [
        UnifiedEditorHistory::Undo,
        UnifiedEditorHistory::Redo,
        UnifiedEditorHistory::Reset,
    ] {
        let token = task::CancellationToken::default();
        token.start().unwrap();
        let mut calls = 0;
        r.s.unified_editor_history_prepared(&mut d, action, Some(&token), &mut |_| {
            calls += 1;
            assert_eq!(token.state(), task::TaskState::Running);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(token.cancel(), task::CancelOutcome::TooLate);
    }
}

#[test]
fn prepared_reset_rejects_foreign_service_and_cancelled_token_before_host_work() {
    let r = Run::new();
    let foreign = Run::new();
    let mut d = r.draft();
    let generation = r.set(&mut d, r.groups(), move_by(1.));
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    let work = d.work_geometry().unwrap().to_vec();
    let generation = d.generation().unwrap();
    let mut calls = 0;
    assert_eq!(
        foreign
            .s
            .unified_editor_history_prepared(&mut d, UnifiedEditorHistory::Reset, None, &mut |_| {
                calls += 1;
                Ok(())
            })
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    let token = task::CancellationToken::default();
    token.start().unwrap();
    token.cancel();
    assert_eq!(
        r.s.unified_editor_history_prepared(
            &mut d,
            UnifiedEditorHistory::Reset,
            Some(&token),
            &mut |_| {
                calls += 1;
                Ok(())
            }
        )
        .unwrap_err()
        .code,
        "CANCELLED"
    );
    assert_eq!(calls, 0);
    assert_eq!(d.generation().unwrap(), generation);
    assert_eq!(d.work_geometry().unwrap(), work);
}

#[test]
fn prepared_own_stale_reset_discards_draft_without_touching_new_main_revision() {
    let mut r = Run::new();
    let mut d = r.draft();
    let entry = d.entry_geometry().unwrap().to_vec();
    let generation = r.set(&mut d, r.groups(), move_by(5.));
    r.s.unified_editor_execute(&mut d, generation, None)
        .unwrap();
    r.edit(r.groups(), move_by(13.)).unwrap();
    let main = r.info();
    let source = r.scene();
    let token = task::CancellationToken::default();
    token.start().unwrap();
    let mut calls = 0;
    r.s.unified_editor_history_prepared(
        &mut d,
        UnifiedEditorHistory::Reset,
        Some(&token),
        &mut |candidate| {
            calls += 1;
            assert!(candidate.objects().map(|(_, _, g)| g).eq(entry.iter()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(token.cancel(), task::CancelOutcome::TooLate);
    assert_eq!(d.work_geometry().unwrap(), entry);
    assert_eq!(
        (
            d.resources().unwrap().undo_entries,
            d.resources().unwrap().redo_entries
        ),
        (0, 0)
    );
    assert!(!d.is_closed());
    assert!(r.s.unified_editor_begin_apply(&mut d).is_err());
    r.unchanged(&main, &source);
}
