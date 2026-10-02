use crate::state::{Action, Model};
use editor_core::MmPoint;
use editor_service::task::{CancelOutcome, TaskContext, TaskState};

fn model() -> Model {
    let mut model = Model::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s4c3/blocks.gbr");
    model.open(&path).unwrap();
    model
}
fn task(model: &Model, id: u64) -> TaskContext {
    TaskContext::new(id, model.task_version().unwrap())
}
fn content(model: &Model) -> serde_json::Value {
    let id = &model.view.info.as_ref().unwrap().document_id;
    serde_json::to_value(model.service.render_snapshot(id).unwrap()).unwrap()
}
fn assert_rejected(model: &mut Model, old: TaskContext) {
    let before = content(model);
    let info = model
        .service
        .document_get(&model.view.info.as_ref().unwrap().document_id)
        .unwrap();
    model.run_task(old, Action::CreateEmptyLayer(Some("must-not-exist".into())));
    assert_eq!(model.view.error.as_ref().unwrap().code, "STALE_TASK");
    assert_eq!(before, content(model));
    assert_eq!(info, model.service.document_get(&info.document_id).unwrap());
    assert_eq!(
        model.view.task_receipt.as_ref().unwrap().state,
        TaskState::Failed
    );
}
#[test]
fn changed_document_undo_redo_workspace_and_policy_reject_old_work() {
    let mut m = model();
    let old = task(&m, 1);
    m.run(Action::CreateEmptyLayer(None));
    assert_rejected(&mut m, old);
    let old = task(&m, 2);
    m.run(Action::History(false));
    assert_rejected(&mut m, old);
    let old = task(&m, 3);
    m.run(Action::History(true));
    assert_rejected(&mut m, old);
    let old = task(&m, 4);
    m.run(Action::SetAllLayersVisible(false));
    assert_rejected(&mut m, old);
    let old = task(&m, 5);
    m.view.rule_revision += 1;
    assert_rejected(&mut m, old);
    let old = task(&m, 6);
    m.view.task_generation += 1;
    assert_rejected(&mut m, old);
    let mut old = task(&m, 7);
    old.input.geometry_policy_hash = "old geometry policy".into();
    assert_rejected(&mut m, old);
}
#[test]
fn close_and_replace_cannot_receive_old_task_even_at_same_revision() {
    let mut m = model();
    let old = task(&m, 1);
    m.run_task(task(&m, 2), Action::Close(true));
    assert!(m.view.info.is_none());
    m.run_task(old, Action::NewWorkspace);
    assert_eq!(m.view.error.as_ref().unwrap().code, "STALE_TASK");
    assert!(m.view.info.is_none());
    m.run(Action::NewWorkspace);
    let old = task(&m, 3);
    m.run(Action::DiscardNewWorkspace);
    assert_rejected(&mut m, old);
}
#[test]
fn queued_cancel_preserves_manufacturing_and_selection_and_can_continue() {
    let mut m = model();
    let old = task(&m, 1);
    let before = content(&m);
    assert_eq!(old.cancel_token.cancel(), CancelOutcome::Requested);
    m.run_task(old, Action::CreateEmptyLayer(None));
    assert_eq!(m.view.error.as_ref().unwrap().code, "CANCELLED");
    assert_eq!(content(&m), before);
    let t = task(&m, 2);
    let token = t.cancel_token.clone();
    m.run_task(t, Action::CreateEmptyLayer(None));
    assert!(m.view.error.is_none());
    assert_eq!(token.state(), TaskState::Completed);
    assert_eq!(token.cancel(), CancelOutcome::TooLate);
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    m.run_task(task(&m, 3), Action::History(false));
    assert_eq!(m.view.info.as_ref().unwrap().redo_entries, 1);
}
#[test]
fn worker_checks_service_revision_not_cached_view() {
    let mut m = model();
    let old = task(&m, 1);
    let info = m.view.info.clone().unwrap();
    m.service
        .create_empty_layer(
            &info.document_id,
            &info.revision,
            editor_service::CreateEmptyLayerParams { display_name: None },
        )
        .unwrap();
    assert_rejected(&mut m, old);
}
#[test]
fn superseded_viewport_cancels_only_after_new_request_is_accepted() {
    let mut app = crate::modal::tests::app();
    let (tx, rx) = std::sync::mpsc::sync_channel(2);
    app.tx = tx;
    let bounds = editor_core::BoundsMm {
        min_x_mm: 0.,
        min_y_mm: 0.,
        max_x_mm: 1.,
        max_y_mm: 1.,
    };
    app.send(Action::Viewport(MmPoint::new(0., 0.), bounds, 20.));
    let viewport = app.viewport_task.clone().unwrap();
    app.send(Action::NewWorkspace);
    assert_eq!(viewport.cancel_token.state(), TaskState::CancelRequested);
    let mut m = Model::default();
    for _ in 0..2 {
        let (_, _, action, context) = rx.recv().unwrap();
        m.run_task(context, action);
    }
    assert!(m.view.info.is_some());
    assert!(m.view.error.is_none());
    let receipt = m.view.task_receipt.as_ref().unwrap();
    assert_eq!(receipt.task_id, 2);
    assert_eq!(receipt.result_version.generation, 1);
    assert_eq!(
        app.pending_task.as_ref().unwrap().cancel_token.state(),
        TaskState::Completed
    );
}
#[test]
fn scene_builder_obeys_cancel_without_publishing_partial_scene() {
    let m = model();
    let cancel = editor_service::task::CancellationToken::default();
    cancel.start().unwrap();
    cancel.cancel();
    let before = m.view.scene.clone().unwrap();
    let result = crate::display::Scene::build_cached_with_cancel(
        m.view.snap_snapshot.as_ref().unwrap(),
        &m.view.layers,
        before.anchor,
        before.ppm,
        999,
        Some(&before),
        &mut Default::default(),
        Some(&cancel),
    );
    assert_eq!(result.err().unwrap(), "CANCELLED");
    assert!(std::sync::Arc::ptr_eq(
        &before,
        m.view.scene.as_ref().unwrap()
    ));
}

#[test]
fn exact_selection_cancels_inside_loop_without_partial_results() {
    let m = model();
    let snapshot = m.view.snap_snapshot.as_ref().unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s4c3/blocks.gbr");
    let mut doc = gerber_io::parse_s1(&std::fs::read(path).unwrap(), "cancel-test")
        .unwrap()
        .document;
    doc.layers = snapshot.layers.clone();
    doc.apertures = snapshot.apertures.clone();
    doc.block_definitions = snapshot.block_definitions.clone();
    let mut calls = 0;
    let result = doc.select_rect_cancellable(
        &doc.layers[0].id,
        m.view.bounds.unwrap(),
        editor_core::hit_test::SelectRectMode::Crossing,
        || {
            calls += 1;
            calls == 3
        },
    );
    assert_eq!(calls, 3);
    assert_eq!(
        result.unwrap_err(),
        editor_core::hit_test::HitTestError::Cancelled
    );
}

#[test]
fn result_publication_checks_input_output_and_independent_policy_generations() {
    let mut m = model();
    let current = m.view.clone();
    let context = task(&m, 8);
    m.run_task(context.clone(), Action::CreateEmptyLayer(None));
    assert!(crate::task_reply_matches(&context, &current, &m.view));
    let mut changed = current.clone();
    changed.rule_revision += 1;
    assert!(!crate::task_reply_matches(&context, &changed, &m.view));
    let mut result = m.view.clone();
    result.task_receipt.as_mut().unwrap().task_id += 1;
    assert!(!crate::task_reply_matches(&context, &current, &result));
    let mut result = m.view.clone();
    result.info.as_mut().unwrap().revision = "99999".into();
    assert!(!crate::task_reply_matches(&context, &current, &result));
}
#[test]
fn empty_workspace_import_failure_leaves_no_document_and_can_retry() {
    let mut m = Model::default();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s4c3/blocks.gbr");
    m.run_task(task(&m, 1), Action::ImportGerbers(vec![root.clone()]));
    assert!(m.view.error.is_none());
    let info = m.view.info.clone().unwrap();
    assert_eq!(info.undo_entries, 1);
    m.run_task(task(&m, 2), Action::Close(true));
    assert!(m.view.info.is_none());
    assert!(m.view.snap_snapshot.is_none());
    assert!(m.view.snap_index.entries().is_empty());
    let t = task(&m, 3);
    t.cancel_token.cancel();
    m.run_task(t, Action::ImportGerbers(vec![root]));
    assert_eq!(m.view.error.as_ref().unwrap().code, "CANCELLED");
    assert!(m.view.info.is_none());
}

/// Opt-in release audit over explicitly supplied local files; never uploads inputs.
#[test]
#[ignore = "release timing audit; requires RCAM_A2_FIXTURES JSON paths"]
fn a2_measure_cancellation_windows() {
    use std::time::Instant;
    let paths: Vec<String> =
        serde_json::from_slice(&std::fs::read(std::env::var("RCAM_A2_FIXTURES").unwrap()).unwrap())
            .unwrap();
    let diagnostics_dir =
        std::env::temp_dir().join(format!("rcam-a2-timings-{}", std::process::id()));
    let diagnostics =
        rcam_diagnostics::Runtime::start(diagnostics_dir, "A2 audit", "audit").unwrap();
    assert!(diagnostics.install_sink());
    for path in paths {
        let at = Instant::now();
        let bytes = std::fs::read(&path).unwrap();
        let read_ms = at.elapsed().as_secs_f64() * 1000.;
        let sha = editor_core::hash::sha256_hex(&bytes);
        let at = Instant::now();
        gerber_io::export_timings::take();
        let strict = gerber_io::parse_s1(&bytes, "audit");
        let strict_ms = at.elapsed().as_secs_f64() * 1000.;
        let strict_parts = gerber_io::export_timings::take();
        let at = Instant::now();
        let scene = strict.or_else(|_| gerber_io::parse_s1_compat(&bytes, "audit"));
        let fallback_ms = at.elapsed().as_secs_f64() * 1000.;
        let fallback_parts = gerber_io::export_timings::take();
        let scene = match scene {
            Ok(scene) => scene,
            Err(error) => {
                println!(
                    "A2_TIMING {}",
                    serde_json::json!({"sha256":sha,"bytes":bytes.len(),"read_ms":read_ms,"strict_ms":strict_ms,"fallback_ms":fallback_ms,"error":error.to_string()})
                );
                panic!("selected audit fixture could not be parsed");
            }
        };
        let mut last_checkpoint = Instant::now();
        let mut max_parse_gap_ms = 0_f64;
        let start = Instant::now();
        let checked = gerber_io::parse_s1_cancellable(&bytes, "audit", true, || {
            max_parse_gap_ms =
                max_parse_gap_ms.max(last_checkpoint.elapsed().as_secs_f64() * 1000.);
            last_checkpoint = Instant::now();
            false
        })
        .unwrap();
        max_parse_gap_ms = max_parse_gap_ms.max(last_checkpoint.elapsed().as_secs_f64() * 1000.);
        let checked_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(checked.document, scene.document);
        assert_eq!(
            serde_json::to_vec(&checked.document).unwrap(),
            serde_json::to_vec(&scene.document).unwrap()
        );
        let mut max_edges_for_ms = 0_f64;
        let mut boundary_errors = 0;
        for layer in &scene.document.layers {
            for object in &layer.objects {
                let start = Instant::now();
                // Public wrapper calls exactly the selection edges_for routine.
                // Aperture lookup construction is included (conservative bound).
                let result = editor_core::hit_test::display_boundary_edges(
                    &object.geometry,
                    &scene.document.apertures,
                );
                max_edges_for_ms = max_edges_for_ms.max(start.elapsed().as_secs_f64() * 1000.);
                boundary_errors += usize::from(result.is_err());
            }
        }
        let mut largest_object_ms = 0_f64;
        let mut selection_ms = 0.;
        let mut selection_error = None;
        for layer in &scene.document.layers {
            let start = Instant::now();
            let mut checkpoint = Instant::now();
            let selected = scene.document.select_rect_cancellable(
                &layer.id,
                editor_core::BoundsMm {
                    min_x_mm: -1e6,
                    min_y_mm: -1e6,
                    max_x_mm: 1e6,
                    max_y_mm: 1e6,
                },
                editor_core::hit_test::SelectRectMode::Crossing,
                || {
                    largest_object_ms =
                        largest_object_ms.max(checkpoint.elapsed().as_secs_f64() * 1000.);
                    checkpoint = Instant::now();
                    false
                },
            );
            largest_object_ms = largest_object_ms.max(checkpoint.elapsed().as_secs_f64() * 1000.);
            selection_ms += start.elapsed().as_secs_f64() * 1000.;
            if let Err(e) = selected {
                selection_error = Some(format!("{e:?}"));
            }
        }
        let at = Instant::now();
        let mut service = editor_service::ApplicationService::new();
        service
            .grant_file_access(std::path::Path::new(&path), false)
            .unwrap();
        let info = service.open(&path).unwrap();
        let service_open_ms = at.elapsed().as_secs_f64() * 1000.;
        let snapshot = service.render_snapshot(&info.document_id).unwrap();
        let layers = service.layers_list(&info.document_id).unwrap();
        let at = Instant::now();
        let world = crate::world_index::WorldIndex::build(&snapshot).unwrap();
        let world_index_ms = at.elapsed().as_secs_f64() * 1000.;
        let at = Instant::now();
        let display = crate::display::Scene::build_cached(
            &snapshot,
            &layers,
            MmPoint::new(0., 0.),
            1.,
            1,
            None,
            &mut Default::default(),
        );
        let scene_ms = at.elapsed().as_secs_f64() * 1000.;
        assert!(diagnostics.runtime().flush());
        let raw_timings =
            std::fs::read_to_string(diagnostics.runtime().directory().join("rcam.log")).unwrap();
        let accelerate_us = raw_timings
            .lines()
            .rev()
            .map(|line| serde_json::from_str::<rcam_diagnostics::Event>(line).unwrap())
            .find(|e| e.command_id == "scene.accelerate_polygons")
            .and_then(|e| e.metrics.get("duration_us").copied())
            .unwrap();
        let display = display.unwrap();
        let at = Instant::now();
        let _index =
            crate::render_index::RenderIndex::build(&display.objects, &[], [0.; 2]).unwrap();
        let render_index_ms = at.elapsed().as_secs_f64() * 1000.;
        let at = Instant::now();
        let mut metrics_errors = 0;
        let mut max_metrics_pass_ms = 0_f64;
        let mut max_metrics_rejection_ms = 0_f64;
        let mut metrics_passes = 0;
        let mut metrics_objects = 0;
        for layer in &snapshot.layers {
            let mut batches: std::collections::VecDeque<_> = layer.objects.chunks(10_000).collect();
            while let Some(objects) = batches.pop_front() {
                let pass = Instant::now();
                let result = service.objects_metrics(
                    &info.document_id,
                    editor_service::MetricsParams {
                        layer_id: layer.id.clone(),
                        object_ids: objects.iter().map(|o| o.object_id.clone()).collect(),
                    },
                );
                let elapsed = pass.elapsed().as_secs_f64() * 1000.;
                match result {
                    Ok(_) => {
                        max_metrics_pass_ms = max_metrics_pass_ms.max(elapsed);
                        metrics_passes += 1;
                        metrics_objects += objects.len();
                    }
                    Err(error) => {
                        assert_eq!(error.code, "RESOURCE_LIMIT");
                        assert!(objects.len() > 1, "single object exceeds metrics budget");
                        metrics_errors += 1;
                        max_metrics_rejection_ms = max_metrics_rejection_ms.max(elapsed);
                        let (left, right) = objects.split_at(objects.len() / 2);
                        batches.push_back(left);
                        batches.push_back(right);
                    }
                }
            }
        }
        assert_eq!(
            metrics_objects,
            snapshot
                .layers
                .iter()
                .map(|l| l.objects.len())
                .sum::<usize>()
        );
        let metrics_ms = at.elapsed().as_secs_f64() * 1000.;
        println!(
            "A2_TIMING {}",
            serde_json::json!({"sha256":sha,"bytes":bytes.len(),"objects":world.entries().len(),"read_ms":read_ms,"max_parse_checkpoint_gap_ms":max_parse_gap_ms,"checked_parse_ms":checked_ms,"strict_parts_us":strict_parts,"fallback_parts_us":fallback_parts,"strict_parse_semantic_ms":strict_ms,"fallback_parse_semantic_ms":fallback_ms,"service_open_ms":service_open_ms,"world_index_ms":world_index_ms,"scene_ms":scene_ms,"accelerate_polygons_us":accelerate_us,"max_edges_for_ms":max_edges_for_ms,"boundary_errors":boundary_errors,"render_index_ms":render_index_ms,"metrics_ms":metrics_ms,"max_metrics_pass_ms":max_metrics_pass_ms,"metrics_passes":metrics_passes,"metrics_objects":metrics_objects,"max_metrics_rejection_ms":max_metrics_rejection_ms,"metrics_errors":metrics_errors,"selection_ms":selection_ms,"largest_selection_checkpoint_gap_ms":largest_object_ms,"selection_error":selection_error})
        );
        assert!(read_ms <= 2000., "read exceeded cancellation window");
        assert!(
            max_parse_gap_ms <= 2000.,
            "parse checkpoint gap exceeded cancellation window"
        );
        assert!(
            largest_object_ms <= 2000.,
            "selection checkpoint gap exceeded cancellation window"
        );
        assert!(
            max_edges_for_ms < 500.,
            "edges_for requires finer checkpoints"
        );
        assert!(
            accelerate_us < 500_000,
            "accelerate_polygons requires finer checkpoints"
        );
        assert!(
            [
                world_index_ms,
                render_index_ms,
                max_metrics_pass_ms,
                max_metrics_rejection_ms
            ]
            .iter()
            .all(|ms| *ms <= 2000.),
            "index/metrics requires finer checkpoints"
        );
    }
}

#[test]
fn scene_cancel_after_real_objects_discards_temporary_scene_and_preserves_old_view() {
    let mut m = model();
    m.run(Action::SelectRect(
        m.view.bounds.unwrap(),
        editor_core::hit_test::SelectRectMode::Crossing,
    ));
    let selection: Vec<String> = m
        .view
        .selected
        .ids()
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(selection.len(), 5);
    let old_scene = m.view.scene.clone().unwrap();
    let before = content(&m);
    let info = m.view.info.clone();
    crate::display::CANCEL_SCENE_AFTER_OBJECTS.set(Some(2));
    let context = task(&m, 101);
    let token = context.cancel_token.clone();
    let action = Action::Viewport(old_scene.anchor, m.view.bounds.unwrap(), old_scene.ppm * 2.);
    m.run_task(context, action);
    assert!(
        crate::display::CANCEL_SCENE_AFTER_OBJECTS.get().is_none(),
        "in-loop hook must be reached"
    );
    assert_eq!(token.state(), TaskState::Cancelled);
    assert_eq!(m.view.error.as_ref().unwrap().code, "CANCELLED");
    assert!(std::sync::Arc::ptr_eq(
        &old_scene,
        m.view.scene.as_ref().unwrap()
    ));
    assert_eq!(content(&m), before);
    assert_eq!(m.view.info, info);
    assert_eq!(
        m.view
            .selected
            .ids()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        selection
    );
    let next = task(&m, 102);
    m.run_task(
        next,
        Action::Viewport(old_scene.anchor, m.view.bounds.unwrap(), old_scene.ppm * 2.),
    );
    assert!(m.view.error.is_none());
    assert_eq!(
        m.view.task_receipt.as_ref().unwrap().state,
        TaskState::Completed
    );
}

#[test]
fn publication_rejects_all_six_independent_current_and_result_dimensions() {
    let mut m = model();
    let current = m.view.clone();
    let context = task(&m, 201);
    m.run_task(context.clone(), Action::CreateEmptyLayer(None));
    let mutate = |view: &mut crate::state::View, field| match field {
        0 => view
            .info
            .as_mut()
            .unwrap()
            .document_id
            .push_str("-replaced"),
        1 => view.info.as_mut().unwrap().revision = "999".into(),
        2 => view.info.as_mut().unwrap().workspace_revision = "999".into(),
        3 => view.task_generation += 1,
        4 => view.rule_revision += 1,
        _ => {
            view.info
                .as_mut()
                .unwrap()
                .manufacturing_precision
                .resolution_mm *= 2.
        }
    };
    assert!(crate::task_reply_matches(&context, &current, &m.view));
    for field in 0..6 {
        let mut changed = current.clone();
        mutate(&mut changed, field);
        assert!(!crate::task_reply_matches(&context, &changed, &m.view));
        let mut result = m.view.clone();
        mutate(&mut result, field);
        assert!(!crate::task_reply_matches(&context, &current, &result));
    }
}
