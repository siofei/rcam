use crate::{
    point_input::{self, Context, Draft, Point, Source},
    point_transform::{Mode, Session},
    state::{Action, Model},
};
use editor_core::{BoundsMm, MmPoint, SemanticGeometry};
use editor_service::{SelectionCentersParams, SelectionMaterialSemantics};
fn model() -> Model {
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/s5i2b");
    m.run(Action::ImportGerbers(
        ["layer_a.gbr", "layer_b.gbr", "layer_c.gbr"]
            .map(|p| root.join(p))
            .to_vec(),
    ));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: -2.,
            min_y_mm: -2.,
            max_x_mm: 22.,
            max_y_mm: 6.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert_eq!(m.view.selected.ordered.len(), 5);
    assert_eq!(m.view.selected.groups().len(), 3);
    let identity = crate::state::selection_geometry_identity(&m.view);
    let groups = m.view.selected.groups();
    m.run(Action::SelectionCenters(
        identity,
        SelectionCentersParams {
            groups,
            semantics: SelectionMaterialSemantics::SelectedLayerComposite,
        },
    ));
    assert!(m.view.error.is_none());
    m
}
fn p(x: f64, y: f64) -> Point {
    Point {
        world_mm: MmPoint::new(x, y),
        source: Source::Numeric,
    }
}

#[test]
fn frame_cancel_beats_enter_repeat_and_mouse_apply_in_all_selection_tools() {
    use eframe::egui;
    for mode in [
        Mode::Move,
        Mode::Copy,
        Mode::Rotate,
        Mode::HorizontalMirror,
        Mode::VerticalMirror,
    ] {
        for conflict in 0..5 {
            let mut m = model();
            let before = m.view.info.clone();
            let selected = m.view.selected.clone();
            let mut app = crate::modal::tests::app();
            let (tx, rx) = std::sync::mpsc::sync_channel(8);
            app.tx = tx;
            app.view = m.view.clone();
            app.routing.bind_fixture(&app.view);
            app.camera.scale = 32.;
            let mut s = Session::new(&m.view, mode, app.display_unit);
            s.target.set(p(8., -3.), app.display_unit);
            let r = s.request(&m.view, app.display_unit, 32.).unwrap();
            m.point_preview(r.clone()).unwrap();
            app.view.point_preview = m.view.point_preview.clone();
            s.requested = Some(r);
            app.point_transform = Some(s);
            app.modal = Some(match mode {
                Mode::Rotate => crate::modal::ActiveModal::Rotate,
                Mode::HorizontalMirror | Mode::VerticalMirror => crate::modal::ActiveModal::Mirror,
                _ => crate::modal::ActiveModal::Move,
            });
            let ctx = egui::Context::default();
            let raw = egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 900.),
                )),
                ..Default::default()
            };
            let _ = ctx.run(raw.clone(), |c| app.parameter_modal(c));
            let out = ctx.run(raw.clone(), |c| app.parameter_modal(c));
            fn button(shape: &egui::epaint::Shape) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(t) if t.galley.job.text == "应用基点变换" => {
                        Some(t.pos + t.galley.size() / 2.)
                    }
                    egui::epaint::Shape::Vec(v) => v.iter().find_map(button),
                    _ => None,
                }
            }
            let pos = out.shapes.iter().find_map(|s| button(&s.shape)).unwrap();
            let mouse = conflict >= 2;
            if mouse {
                let mut press = raw.clone();
                press.events = vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ];
                let _ = ctx.run(press, |c| app.parameter_modal(c));
            }
            let mut cancel = raw.clone();
            if mouse {
                cancel.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                });
            }
            if conflict <= 2 {
                for key in [egui::Key::Escape, egui::Key::Enter] {
                    cancel.events.push(egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: conflict == 1,
                        modifiers: Default::default(),
                    });
                }
            } else if conflict == 3 {
                cancel.focused = false;
                cancel.events.push(egui::Event::WindowFocused(false));
            } else {
                cancel.events.push(egui::Event::PointerGone);
            }
            let _ = ctx.run(cancel, |c| {
                app.parameter_modal(c);
                // A later same-frame handler cannot resurrect a commit either.
                app.send(Action::Move("1".into(), "1".into()));
            });
            while let Ok((_, _, action, task, _)) = rx.try_recv() {
                m.run_task(task, action);
            }
            assert_eq!(m.view.info, before, "{mode:?} conflict {conflict}");
            assert_eq!(m.view.selected, selected);
            assert!(app.point_input_cancelled);
            assert!(app.modal.is_none());
        }
    }
}
#[test]
#[ignore = "release-only B contour CPU observations, not visible-input latency"]
fn b_p100k_contour_cpu() {
    assert!(!cfg!(debug_assertions));
    let mut m = Model::default();
    m.open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr"),
    )
    .unwrap();
    let info = m.view.info.clone();
    let settings = crate::object_snap::Settings::default().contour();
    let mut runtime = crate::object_snap::Runtime::default();
    let mut times = Vec::new();
    for n in 0..40 {
        let camera = crate::camera::Camera {
            center: MmPoint::new(200., 125.),
            scale: [40., 200., 1000.][n % 3],
        };
        let radius = 0.25 + 1. / (camera.scale * if n % 2 == 0 { 1. } else { 2. });
        let raw = MmPoint::new(
            (n % 20 + 1) as f64 + radius * 0.53f64.cos(),
            1. + radius * 0.53f64.sin(),
        );
        let start = std::time::Instant::now();
        let value = runtime
            .resolve(
                raw,
                &settings,
                crate::tools::GridSettings {
                    snap_enabled: false,
                    ..Default::default()
                },
                camera,
                if n % 2 == 0 { 1. } else { 2. },
                m.view.snap_snapshot.as_deref(),
                &m.view.snap_index,
                &m.view.layers,
                None,
                false,
            )
            .unwrap();
        assert!(value.point.is_valid_geometry());
        assert_eq!(
            value.kind,
            Some(editor_core::snap::SnapKind::Nearest),
            "measure actual contour resolution, not empty queries"
        );
        if n >= 10 {
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
    }
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let p95 = sorted[28];
    eprintln!(
        "I2_B_CONTOUR_CPU {}",
        serde_json::json!({"raw_ms":times,"p95_ms":p95,"samples":30,"dpi":[1,2],"zoom":[40,200,1000],"scope":"Runtime.resolve nearby manufacturing contour CPU; no visible-input/GPU latency"})
    );
    assert!(p95 <= 20., "P100K nearby contour p95 CPU budget");
    assert_eq!(m.view.info, info);
}
#[test]
#[ignore = "release-only longest pre-existing point-preview segment measurement"]
fn b_preview_long_segment_observation() {
    assert!(!cfg!(debug_assertions));
    let mut m = model();
    let prototype = m.view.selected.ordered[0].clone();
    m.view.selected.ordered = (0..10_000)
        .map(|n| {
            let mut o = prototype.clone();
            o.object.object_id = format!("stress-{n}");
            o
        })
        .collect();
    let aperture = m.view.apertures[0].clone();
    m.view.apertures.extend((0..10_000).map(|n| {
        let mut a = aperture.clone();
        a.id = format!("unused-{n}");
        a
    }));
    let s = Session::new(
        &m.view,
        Mode::Rotate,
        editor_core::units::DisplayUnit::Millimeter,
    );
    let request = s
        .request(&m.view, editor_core::units::DisplayUnit::Millimeter, 100.)
        .unwrap();
    let info = m.view.info.clone();
    let start = std::time::Instant::now();
    let result = m.point_preview(request);
    let seconds = start.elapsed().as_secs_f64();
    eprintln!(
        "I2_B_LONG_SEGMENT {}",
        serde_json::json!({"seconds":seconds,"selected":10000,"apertures":m.view.apertures.len(),"result":result.as_ref().map(|_|"ok").map_err(|e|e.code.clone()),"scope":"synthetic read-only worker View; compare preserved pre-fix measurement separately"})
    );
    assert_eq!(m.view.info, info);
    assert!(result.is_ok());
    assert!(seconds <= 2., "controlled total preview budget");
}
fn centers(m: &Model) -> Vec<MmPoint> {
    m.view
        .selected
        .ordered
        .iter()
        .map(|o| match o.object.geometry {
            SemanticGeometry::Flash { center, .. } => center,
            _ => panic!(),
        })
        .collect()
}
#[test]
fn common_world_mirror_preview_and_commit_reverse_arc_direction_keep_radius() {
    let mut m = Model::default();
    m.open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s5i2b/arc.gbr"),
    )
    .unwrap();
    let layer = m.view.layers[0].layer_id.clone();
    let snapshot = m.view.snap_snapshot.as_ref().unwrap();
    let id = snapshot.layers[0].objects[0].object_id.clone();
    let doc = m.view.info.as_ref().unwrap().document_id.clone();
    *m.view.selected.ordered = vec![
        m.service
            .objects_get(
                &doc,
                editor_service::ObjectParams {
                    layer_id: layer,
                    object_id: id,
                },
            )
            .unwrap(),
    ];
    let original = m.view.selected.ordered[0].object.geometry.clone();
    let mut s = Session::new(&m.view, Mode::HorizontalMirror, Default::default());
    s.base.set(p(0., 0.), Default::default());
    let request = s.request(&m.view, Default::default(), 100.).unwrap();
    m.point_preview(request.clone()).unwrap();
    m.point_apply(request).unwrap();
    let SemanticGeometry::Arc { path, width_mm } = &m.view.selected.ordered[0].object.geometry
    else {
        panic!()
    };
    assert_eq!(path.direction, editor_core::ArcDirection::Clockwise);
    assert_eq!(path.start, MmPoint::new(1., 0.));
    assert_eq!(path.end, MmPoint::new(0., -1.));
    assert_eq!(path.radius(), 1.);
    assert_eq!(*width_mm, 0.2);
    m.run(Action::History(false));
    assert_eq!(m.view.selected.ordered[0].object.geometry, original);
}
#[test]
fn numeric_grip_keeps_opposite_edge_and_commits_its_shown_preview_once() {
    let mut m = model();
    *m.view.selected.ordered = vec![m.view.selected.ordered.last().unwrap().clone()];
    m.run(Action::SetActiveLayer(Some(
        m.view.selected.ordered[0].layer_id.clone(),
    )));
    let mut app = crate::modal::tests::app();
    app.view = m.view.clone();
    app.routing.bind_fixture(&app.view);
    let before = app.view.info.clone();
    app.open_point_adapter(
        crate::point_adapter::Adapter::Grip(editor_core::grip::GripFeatureId::Right),
        MmPoint::new(22., 4.),
    );
    let mut adapter = app.point_adapter.take().unwrap();
    app.preview_adapter_point(&mut adapter, MmPoint::new(22., 4.))
        .unwrap();
    let grip = adapter.grip_preview.unwrap();
    let preview = grip.preview.as_ref().unwrap().clone();
    assert_eq!(app.view.info, before);
    match preview.geometry {
        SemanticGeometry::Flash { center, .. } => assert_eq!(center, MmPoint::new(20.75, 4.)),
        _ => panic!(),
    }
    match preview.aperture_shape.as_ref().unwrap() {
        editor_core::ApertureShape::Rectangle {
            width_mm,
            height_mm,
            ..
        } => {
            assert_eq!(*width_mm, 2.5);
            assert_eq!(*height_mm, 1.);
        }
        _ => panic!(),
    }
    m.run(grip.release().unwrap());
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(centers(&m), vec![MmPoint::new(20.75, 4.)]);
    let info = m.view.info.as_ref().unwrap();
    assert_eq!(info.undo_entries, before.as_ref().unwrap().undo_entries + 1);
    let object = &m.view.selected.ordered[0].object;
    let SemanticGeometry::Flash { aperture_id, .. } = &object.geometry else {
        panic!()
    };
    assert_eq!(
        m.view
            .apertures
            .iter()
            .find(|a| a.id == *aperture_id)
            .unwrap()
            .shape,
        preview.aperture_shape.unwrap()
    );
    m.run(Action::History(false));
    assert_eq!(centers(&m), vec![MmPoint::new(20., 4.)]);
}
#[test]
fn preview_budget_rejects_large_source_before_cloning_and_changes_no_state() {
    let mut m = model();
    m.view.selected.ordered.truncate(1);
    m.view.selected.ordered[0].object.geometry = SemanticGeometry::Region {
        contours: vec![editor_core::RegionContour {
            role: editor_core::RegionRole::Solid,
            edges: vec![
                editor_core::RegionEdge::Line {
                    start: MmPoint::new(0., 0.),
                    end: MmPoint::new(1., 0.)
                };
                131_073
            ],
        }],
    };
    let before = m.view.info.clone();
    let request = Session::new(&m.view, Mode::Rotate, Default::default())
        .request(&m.view, Default::default(), 100.)
        .unwrap();
    let start = std::time::Instant::now();
    let error = m.point_preview(request).unwrap_err();
    assert_eq!(error.code, "RESOURCE_LIMIT");
    assert!(start.elapsed().as_secs_f64() < 0.5);
    assert_eq!(m.view.info, before);
    assert!(m.view.point_preview.is_none());
}
#[test]
fn five_objects_three_layers_distinct_centers_keep_order_primary() {
    let m = model();
    let before = m.view.selected.clone();
    let bounds = point_input::center(&m.view, false).unwrap().world_mm;
    assert_eq!(bounds, MmPoint::new(10.25, 1.75));
    let area = point_input::center(&m.view, true).unwrap();
    let a = std::f64::consts::PI * (1. - 0.25f64.powi(2));
    let total = 9. + 2. * a;
    let expected = MmPoint::new(
        (4. * 1. + 4. * 5. + a * 10. + a * 12. + 20.) / total,
        (4. * 1. + 4. * 1. + a * 0. + a * 2. + 4.) / total,
    );
    assert!(area.world_mm.distance_mm(expected) < 1e-9);
    assert_ne!(area.world_mm, bounds);
    assert_eq!(m.view.selected, before);
}
#[test]
fn all_four_point_sources_resolve_one_preview_and_atomic_three_layer_transform() {
    for mode in [
        Mode::Move,
        Mode::Copy,
        Mode::Rotate,
        Mode::HorizontalMirror,
        Mode::VerticalMirror,
    ] {
        for source in 0..4 {
            let mut m = model();
            let original = centers(&m);
            let info = m.view.info.clone().unwrap();
            let count = m
                .view
                .snap_snapshot
                .as_ref()
                .unwrap()
                .layers
                .iter()
                .map(|l| l.objects.len())
                .sum::<usize>();
            let unit = editor_core::units::DisplayUnit::Millimeter;
            let mut s = Session::new(&m.view, mode, unit);
            let base = match source {
                0 => p(3., -2.),
                1 => point_input::center(&m.view, false).unwrap(),
                2 => point_input::center(&m.view, true).unwrap(),
                _ => Point {
                    world_mm: MmPoint::new(2., 1.),
                    source: Source::Feature(editor_core::snap::SnapKind::Nearest),
                },
            };
            let b = base.world_mm;
            s.base.set(base, unit);
            s.target.set(p(b.x_mm + 3., b.y_mm - 2.), unit);
            s.angle = "90".into();
            let request = s.request(&m.view, unit, 32.).unwrap();
            m.run(Action::PointPreview(Box::new(request.clone())));
            assert!(m.view.error.is_none(), "{:?}", m.view.error);
            assert_eq!(m.view.info, Some(info.clone()));
            assert_eq!(m.view.point_preview.as_ref().unwrap().request, request);
            m.run(Action::PointApply(Box::new(request)));
            assert!(m.view.error.is_none(), "{:?}", m.view.error);
            let changed = centers(&m);
            for (before, after) in original.iter().zip(&changed) {
                let expected = match mode {
                    Mode::Move | Mode::Copy => MmPoint::new(before.x_mm + 3., before.y_mm - 2.),
                    Mode::Rotate => MmPoint::new(
                        b.x_mm - (before.y_mm - b.y_mm),
                        b.y_mm + (before.x_mm - b.x_mm),
                    ),
                    Mode::HorizontalMirror => MmPoint::new(before.x_mm, 2. * b.y_mm - before.y_mm),
                    Mode::VerticalMirror => MmPoint::new(2. * b.x_mm - before.x_mm, before.y_mm),
                };
                assert!(
                    after.distance_mm(expected) < 1e-9,
                    "{mode:?} {source} {after:?} {expected:?}"
                );
            }
            let after = m.view.info.clone().unwrap();
            assert_eq!(after.undo_entries, info.undo_entries + 1);
            assert_eq!(
                after.revision.parse::<u64>().unwrap(),
                info.revision.parse::<u64>().unwrap() + 1
            );
            assert_eq!(
                m.view
                    .snap_snapshot
                    .as_ref()
                    .unwrap()
                    .layers
                    .iter()
                    .map(|l| l.objects.len())
                    .sum::<usize>(),
                count + if mode == Mode::Copy { 5 } else { 0 }
            );
            m.run(Action::History(false));
            assert!(m.view.error.is_none());
            let snapshot = m.view.snap_snapshot.as_ref().unwrap();
            let original_ids: Vec<_> = m.view.layers.iter().map(|l| l.layer_id.clone()).collect();
            assert_eq!(snapshot.layers.len(), 3);
            assert_eq!(original_ids.len(), 3);
            assert_eq!(
                m.view
                    .snap_snapshot
                    .as_ref()
                    .unwrap()
                    .layers
                    .iter()
                    .map(|l| l.objects.len())
                    .sum::<usize>(),
                count
            );
            m.run(Action::History(true));
            assert!(m.view.error.is_none());
        }
    }
}
#[test]
fn full_context_stale_matrix_and_bad_numeric_never_mutate() {
    let mut m = model();
    let mut s = Session::new(&m.view, Mode::Move, Default::default());
    s.base.set(p(0., 0.), Default::default());
    s.target.set(p(3., 4.), Default::default());
    let request = s.request(&m.view, Default::default(), 32.).unwrap();
    for field in 0..8 {
        let mut v = m.view.clone();
        match field {
            0 => v.info.as_mut().unwrap().document_id.push_str("reopen"),
            1 => v.info.as_mut().unwrap().revision = "99".into(),
            2 => v.info.as_mut().unwrap().workspace_revision = "99".into(),
            3 => v.task_generation += 1,
            4 => v.rule_revision += 1,
            5 => v.selection_epoch += 1,
            6 => {
                v.info
                    .as_mut()
                    .unwrap()
                    .manufacturing_precision
                    .resolution_mm = 1e-6
            }
            _ => v.info.as_mut().unwrap().document_id = String::new(),
        };
        assert!(!request.context.valid(&v));
    }
    let original = m.view.info.clone();
    let mut bad = request.clone();
    bad.context.selection_epoch += 1;
    m.run(Action::PointApply(Box::new(bad)));
    assert!(m.view.error.is_some());
    assert_eq!(m.view.info, original);
    s.target.x = "NaN".into();
    assert!(s.request(&m.view, Default::default(), 32.).is_err());
    assert_eq!(m.view.info, original);
}
#[test]
fn display_units_never_round_a_resolved_center_and_zero_area_never_falls_back() {
    let mut m = model();
    let value = point_input::center(&m.view, true).unwrap();
    for unit in editor_core::units::DisplayUnit::ALL {
        let mut d = Draft::default();
        d.set(value.clone(), unit);
        assert_eq!(d.resolve(unit).unwrap(), value);
    }
    let mut result = (**m.view.selection_geometry.as_ref().unwrap()).clone();
    result.material = editor_service::SelectionMaterialResult::Computed {
        value: editor_service::CompositeMaterial::ZeroArea,
    };
    m.view.selection_geometry = Some(std::sync::Arc::new(result));
    assert!(point_input::center(&m.view, true).is_err());
    assert!(point_input::center(&m.view, false).is_ok());
    assert!(Context::capture(&m.view).valid(&m.view));
}
#[test]
fn analytic_hole_pick_dpi_zoom_self_exclusion_alt_grid_and_preferences() {
    let m = model();
    let original = m.view.selected.clone();
    let settings = crate::object_snap::Settings::default();
    assert!(!settings.enabled);
    let contour = settings.contour();
    let mut runtime = crate::object_snap::Runtime::default();
    for ppp in [1., 2.] {
        for scale in [40., 200., 1000.] {
            let camera = crate::camera::Camera {
                scale,
                ..Default::default()
            };
            let hole_point = MmPoint::new(
                10. + 0.25 * (std::f64::consts::PI / 5.).cos(),
                0.25 * (std::f64::consts::PI / 5.).sin(),
            );
            let resolution = runtime
                .resolve(
                    hole_point,
                    &contour,
                    Default::default(),
                    camera,
                    ppp,
                    m.view.snap_snapshot.as_deref(),
                    &m.view.snap_index,
                    &m.view.layers,
                    None,
                    false,
                )
                .unwrap();
            assert!(resolution.point.distance_mm(hole_point) < 1e-12);
            assert_eq!(resolution.kind, Some(editor_core::snap::SnapKind::Nearest));
            let excluded = m
                .view
                .selected
                .ids()
                .into_iter()
                .map(str::to_owned)
                .collect();
            runtime.reset();
            let value = runtime
                .resolve(
                    MmPoint::new(10.24, 0.),
                    &contour,
                    Default::default(),
                    camera,
                    ppp,
                    m.view.snap_snapshot.as_deref(),
                    &m.view.snap_index,
                    &m.view.layers,
                    Some(&excluded),
                    false,
                )
                .unwrap();
            assert_eq!(value.point, MmPoint::new(10.24, 0.));
            assert!(value.kind.is_none());
            let grid = crate::tools::GridSettings {
                snap_enabled: true,
                spacing_mm: 0.1,
                ..Default::default()
            };
            let value = runtime
                .resolve(
                    MmPoint::new(10.24, 0.),
                    &contour,
                    grid,
                    camera,
                    ppp,
                    m.view.snap_snapshot.as_deref(),
                    &m.view.snap_index,
                    &m.view.layers,
                    Some(&excluded),
                    false,
                )
                .unwrap();
            assert!(value.point.distance_mm(MmPoint::new(10.2, 0.)) < 1e-12);
            assert!(value.from_grid);
            let value = runtime
                .resolve(
                    MmPoint::new(10.24, 0.),
                    &contour,
                    grid,
                    camera,
                    ppp,
                    m.view.snap_snapshot.as_deref(),
                    &m.view.snap_index,
                    &m.view.layers,
                    None,
                    true,
                )
                .unwrap();
            assert_eq!(value.point, MmPoint::new(10.24, 0.));
            assert!(value.kind.is_none());
        }
    }
    assert!(!settings.enabled);
    assert!(
        !settings
            .enabled_kinds
            .contains(&editor_core::snap::SnapKind::Nearest)
    );
    assert_eq!(m.view.selected, original);
}
#[test]
fn child_pick_back_keeps_base_and_selection_and_adapter_cancel_restores_parent() {
    let m = model();
    let mut app = crate::modal::tests::app();
    app.view = m.view.clone();
    app.routing.bind_fixture(&app.view);
    let original = app.view.selected.clone();
    let mut session = Session::new(&app.view, Mode::Rotate, app.display_unit);
    session.base.set(p(1.23456789, 2.), app.display_unit);
    let saved = session.base.clone();
    app.point_transform = Some(session);
    app.point_pick = Some(crate::point_transform::Pick {
        context: Context::capture(&app.view),
        field: crate::point_transform::PickField::Base,
        saved: saved.clone(),
        resume: crate::modal::ActiveModal::Rotate,
    });
    app.finish_point_pick(None);
    assert_eq!(app.modal, Some(crate::modal::ActiveModal::Rotate));
    assert_eq!(
        app.point_transform
            .as_ref()
            .unwrap()
            .base
            .resolve(app.display_unit)
            .unwrap(),
        saved.resolve(app.display_unit).unwrap()
    );
    assert_eq!(app.view.selected, original);
    app.modal = Some(crate::modal::ActiveModal::Array);
    app.open_point_adapter(
        crate::point_adapter::Adapter::ArrayBase,
        MmPoint::new(7., 8.),
    );
    app.cancel_modal();
    assert_eq!(app.modal, Some(crate::modal::ActiveModal::Array));
    assert!(app.array_point_base.is_none());
    assert_eq!(app.view.info, m.view.info);
}
#[test]
fn common_readonly_adapters_keep_text_layout_board_and_array_original_cell() {
    let m = model();
    let mut app = crate::modal::tests::app();
    app.view = m.view.clone();
    app.routing.bind_fixture(&app.view);
    let before = app.view.info.clone();
    let selection = app.view.selected.clone();
    app.apply_adapter_point(
        &crate::point_adapter::Adapter::TextReference,
        MmPoint::new(1.7, 0.9),
    )
    .unwrap();
    assert!(app.text.has_reference);
    assert_eq!(app.text.rx, "1.7");
    app.apply_adapter_point(
        &crate::point_adapter::Adapter::BoardWorld(1),
        MmPoint::new(3., 4.),
    )
    .unwrap();
    assert_eq!(app.components.world_points[1], [3., 4.]);
    assert!(!app.components.registration_confirmed);
    app.apply_adapter_point(
        &crate::point_adapter::Adapter::ArrayBase,
        MmPoint::new(8., 9.),
    )
    .unwrap();
    app.apply_adapter_point(
        &crate::point_adapter::Adapter::ArrayTarget,
        MmPoint::new(5., 13.),
    )
    .unwrap();
    assert_eq!(app.array.pitch_x, "-3");
    assert_eq!(app.array.pitch_y, "4");
    app.apply_adapter_point(
        &crate::point_adapter::Adapter::Measure,
        MmPoint::new(0., 0.),
    )
    .unwrap();
    app.apply_adapter_point(
        &crate::point_adapter::Adapter::Measure,
        MmPoint::new(3., 4.),
    )
    .unwrap();
    assert_eq!(app.measure.values(), Some((3., 4., 5.)));
    assert_eq!(app.view.info, before);
    assert_eq!(app.view.selected, selection);
}
#[test]
fn local_block_centers_pick_and_placement_reference_never_use_world_selection() {
    let mut m = model();
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: -1.,
            min_y_mm: -1.,
            max_x_mm: 7.,
            max_y_mm: 3.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert_eq!(m.view.selected.ordered.len(), 2);
    let context = crate::block_ui::Context::capture(&m.view).unwrap();
    let groups = m.view.selected.groups();
    m.run(Action::BlockEdit(Box::new(crate::block_ui::Request {
        context,
        edit: crate::block_ui::Edit::Create(editor_service::CreateBlockDefinitionParams {
            layer_id: groups[0].layer_id.clone(),
            object_ids: groups[0].object_ids.clone(),
            local_origin_mm: editor_service::PivotMm { x_mm: 1., y_mm: 1. },
            name: "local-reference".into(),
        }),
    })));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let d = m.view.block_definitions[0].clone();
    let info = m.view.info.clone().unwrap();
    let snapshot = m.service.render_snapshot(&info.document_id).unwrap();
    let local = m
        .service
        .geometry_block_definition_centers_cancellable(
            &info.document_id,
            &info.revision,
            &d.id.0,
            || false,
        )
        .unwrap();
    assert_eq!(local.bounding_center_mm, Some(MmPoint::new(2., 0.)));
    assert_eq!(
        point_input::from_centers(&local, true).unwrap().world_mm,
        MmPoint::new(2., 0.)
    );
    assert_eq!(
        m.service.render_snapshot(&info.document_id).unwrap(),
        snapshot
    );
    let stale = m
        .service
        .geometry_block_definition_centers_cancellable(&info.document_id, "0", &d.id.0, || false)
        .unwrap_err();
    assert_eq!(stale.code, "REVISION_CONFLICT");
    let cancel = m
        .service
        .geometry_block_definition_centers_cancellable(
            &info.document_id,
            &info.revision,
            &d.id.0,
            || true,
        )
        .unwrap_err();
    assert_eq!(cancel.code, "CANCELLED");
    let mut runtime = crate::object_snap::Runtime::default();
    let value = runtime
        .resolve_definition(
            MmPoint::new(1., 0.3),
            &d,
            &m.view.apertures,
            crate::camera::Camera {
                scale: 100.,
                ..Default::default()
            },
            2.,
            Default::default(),
            false,
        )
        .unwrap();
    assert!(value.point.distance_mm(MmPoint::new(1., 0.3)) < 1e-12);
    assert_eq!(value.kind, Some(editor_core::snap::SnapKind::Nearest));
    let mut app = crate::modal::tests::app();
    app.view = m.view.clone();
    app.routing.bind_fixture(&app.view);
    app.apply_adapter_point(
        &crate::point_adapter::Adapter::BlockLocal(d.id.0.clone()),
        MmPoint::new(2., 0.),
    )
    .unwrap();
    assert_eq!(app.block_point_reference, MmPoint::new(2., 0.));
    assert_eq!(app.view.info, Some(info));
}
#[test]
fn locked_layer_or_cancelled_request_rejects_entire_base_transform() {
    let mut m = model();
    let mut s = Session::new(&m.view, Mode::Copy, Default::default());
    s.base.set(p(0., 0.), Default::default());
    s.target.set(p(3., 4.), Default::default());
    let request = s.request(&m.view, Default::default(), 32.).unwrap();
    let task = editor_service::task::TaskContext::new(771, m.task_version().unwrap());
    task.cancel_token.cancel();
    let info = m.view.info.clone();
    let selection = m.view.selected.clone();
    m.run_task(task, Action::PointApply(Box::new(request.clone())));
    assert_eq!(m.view.error.as_ref().unwrap().code, "CANCELLED");
    assert_eq!(m.view.info, info);
    assert_eq!(m.view.selected, selection);
    let layer = m.view.layers[1].layer_id.clone();
    m.run(Action::Layer(editor_service::LayerUpdateParams {
        layer_id: layer,
        expected_workspace_revision: m.view.info.as_ref().unwrap().workspace_revision.clone(),
        locked: Some(true),
        ..Default::default()
    }));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let before = m.view.info.clone();
    let mut request = request;
    request.context = Context::capture(&m.view);
    request.groups = std::sync::Arc::new(m.view.selected.groups());
    m.run(Action::PointApply(Box::new(request)));
    assert!(m.view.error.is_some());
    assert_eq!(m.view.info, before);
}

#[test]
fn adapter_cancel_and_ime_win_over_keyboard_and_mouse_confirmation() {
    use eframe::egui;
    for block in [false, true] {
        for conflict in 0..6 {
            let mut m = model();
            m.run(Action::SetActiveLayer(Some(
                m.view.layers.last().unwrap().layer_id.clone(),
            )));
            m.run(Action::Select(
                MmPoint::new(20., 4.),
                0.01,
                crate::selection::SelectionMode::Replace,
            ));
            let mut app = crate::modal::tests::app();
            if block {
                let context = crate::block_ui::Context::capture(&m.view).unwrap();
                let group = m.view.selected.groups().remove(0);
                m.run(Action::BlockEdit(Box::new(crate::block_ui::Request {
                    context,
                    edit: crate::block_ui::Edit::Create(
                        editor_service::CreateBlockDefinitionParams {
                            layer_id: group.layer_id,
                            object_ids: group.object_ids,
                            local_origin_mm: editor_service::PivotMm {
                                x_mm: 20.,
                                y_mm: 4.,
                            },
                            name: "cancel-target".into(),
                        },
                    ),
                })));
                assert!(m.view.error.is_none());
                let context = crate::block_ui::Context::capture(&m.view).unwrap();
                let definition = m.view.block_definitions[0].id.0.clone();
                m.run(Action::BlockPreview(
                    context.clone(),
                    definition.clone(),
                    32.,
                ));
                app.block.session = Some(crate::block_ui::Session {
                    context,
                    layer: m.view.layers.last().unwrap().layer_id.clone(),
                    kind: crate::block_ui::SessionKind::Place { definition },
                    point: None,
                    preview: m.view.block_preview.clone(),
                });
                app.tool = crate::tools::ActiveTool::Block;
            }
            app.view = m.view.clone();
            app.routing.bind_fixture(&app.view);
            let before = m.view.info.clone();
            let snapshot = m
                .service
                .render_snapshot(&before.as_ref().unwrap().document_id)
                .unwrap();
            let (tx, rx) = std::sync::mpsc::sync_channel(8);
            app.tx = tx;
            app.open_point_adapter(
                if block {
                    crate::point_adapter::Adapter::BlockTarget
                } else {
                    crate::point_adapter::Adapter::Grip(editor_core::grip::GripFeatureId::Right)
                },
                if block {
                    MmPoint::new(15., 5.)
                } else {
                    MmPoint::new(22., 4.)
                },
            );
            let ctx = egui::Context::default();
            let raw = egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 900.),
                )),
                ..Default::default()
            };
            let _ = ctx.run(raw.clone(), |c| app.parameter_modal(c));
            let out = ctx.run(raw.clone(), |c| app.parameter_modal(c));
            fn button(s: &egui::epaint::Shape) -> Option<egui::Pos2> {
                match s {
                    egui::epaint::Shape::Text(t) if t.galley.job.text == "使用此点" => {
                        Some(t.pos + t.galley.size() / 2.)
                    }
                    egui::epaint::Shape::Vec(v) => v.iter().find_map(button),
                    _ => None,
                }
            }
            let pos = out.shapes.iter().find_map(|s| button(&s.shape)).unwrap();
            if conflict >= 2 {
                let mut press = raw.clone();
                press.events = vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ];
                let _ = ctx.run(press, |c| app.parameter_modal(c));
            }
            let mut cancel = raw.clone();
            if conflict >= 2 {
                cancel.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                });
            }
            if conflict <= 2 {
                for key in [egui::Key::Escape, egui::Key::Enter] {
                    cancel.events.push(egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: conflict == 1,
                        modifiers: Default::default(),
                    });
                }
            } else if conflict == 3 {
                cancel.focused = false;
                cancel.events.push(egui::Event::WindowFocused(false));
            } else if conflict == 4 {
                cancel.events.push(egui::Event::PointerGone);
            } else {
                cancel
                    .events
                    .push(egui::Event::Ime(egui::ImeEvent::Preedit("输入中".into())));
                cancel.events.push(egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                });
            }
            let _ = ctx.run(cancel, |c| app.parameter_modal(c));
            while let Ok((_, _, action, task, _)) = rx.try_recv() {
                m.run_task(task, action);
            }
            assert_eq!(m.view.info, before, "block={block} conflict={conflict}");
            assert_eq!(
                m.service
                    .render_snapshot(&before.as_ref().unwrap().document_id)
                    .unwrap(),
                snapshot
            );
            assert!(app.point_commit_blocked);
        }
    }
}

#[test]
fn production_adapter_and_transform_buttons_keep_rect_for_invalid_and_preview_status() {
    use eframe::egui;
    for viewport in [egui::vec2(980., 760.), egui::vec2(320., 420.)] {
        let mut m = model();
        for adapter in [true, false] {
            let ctx = egui::Context::default();
            let mut app = crate::modal::tests::app();
            app.view = m.view.clone();
            app.routing.bind_fixture(&app.view);
            app.camera.scale = 32.;
            if adapter {
                app.open_point_adapter(
                    crate::point_adapter::Adapter::ArrayBase,
                    MmPoint::new(0., 0.),
                );
            } else {
                app.modal = Some(crate::modal::ActiveModal::Move);
                let mut session = Session::new(&app.view, Mode::Move, app.display_unit);
                session.target.set(p(2., 3.), app.display_unit);
                let request = session
                    .request(&app.view, app.display_unit, app.camera.scale)
                    .unwrap();
                m.point_preview(request.clone()).unwrap();
                app.view.point_preview = m.view.point_preview.clone();
                session.requested = Some(request);
                app.point_transform = Some(session);
            }
            let mut baseline = None;
            for (value, simplified) in [
                ("0".to_owned(), false),
                ("".to_owned(), false),
                ("无效".repeat(100), false),
                ("0".to_owned(), true),
            ] {
                if adapter {
                    app.point_adapter.as_mut().unwrap().draft.x = value;
                } else {
                    app.point_transform.as_mut().unwrap().base.x = value;
                    if let Some(preview) = &mut app.view.point_preview {
                        std::sync::Arc::make_mut(preview).simplified = simplified;
                    }
                }
                for _ in 0..3 {
                    let _ = ctx.run(
                        egui::RawInput {
                            focused: true,
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                viewport,
                            )),
                            ..Default::default()
                        },
                        |ctx| app.parameter_modal(ctx),
                    );
                }
                let id = if adapter {
                    "adapter-apply-rect"
                } else {
                    "transform-apply-rect"
                };
                let rect = ctx
                    .data(|data| data.get_temp::<egui::Rect>(egui::Id::new(id)))
                    .unwrap();
                if let Some(previous) = baseline {
                    assert_eq!(rect, previous, "adapter={adapter} viewport={viewport:?}");
                } else {
                    baseline = Some(rect);
                }
            }
        }
    }
}

#[test]
fn eighty_thousand_numeric_move_rebinds_complete_result_order_and_exact_history() {
    use std::fmt::Write;
    let dir = std::env::temp_dir().join(format!("rcam-point-bulk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("synthetic.gbr");
    let mut source = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\n");
    for i in 0..80_000 {
        writeln!(source, "X{}Y0D02*X{}Y1000000D01*", i * 100, i * 100).unwrap();
    }
    source.push_str("M02*\n");
    std::fs::write(&path, source).unwrap();
    let mut m = Model::default();
    m.run(Action::Open(path));
    assert!(m.view.error.is_none());
    let before = m.view.snap_snapshot.clone().unwrap();
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: -1.,
            min_y_mm: -1.,
            max_x_mm: 10.,
            max_y_mm: 2.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert!(m.view.error.is_none());
    assert_eq!(m.view.selected.ordered.len(), 80_000);
    m.view.selected.ordered.reverse();
    let request = crate::point_transform::Request {
        context: crate::point_input::Context::capture(&m.view),
        groups: std::sync::Arc::new(m.view.selected.groups()),
        operation: editor_service::SelectionEdit::Move {
            dx_mm: 1.,
            dy_mm: 2.,
        },
        ppm: 20.,
    };
    m.point_preview(request.clone()).unwrap();
    assert!(m.view.point_preview.as_ref().unwrap().simplified);
    m.point_apply(request).unwrap();
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    let expected_ids: Vec<_> = before.layers[0]
        .objects
        .iter()
        .map(|o| o.object_id.as_str())
        .collect();
    assert_eq!(m.view.selected.ids(), expected_ids);
    let after = m.view.snap_snapshot.clone().unwrap();
    for (a, b) in before.layers[0]
        .objects
        .iter()
        .zip(&after.layers[0].objects)
    {
        let editor_core::SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } = a.geometry
        else {
            panic!()
        };
        assert_eq!(
            b.geometry,
            editor_core::SemanticGeometry::Line {
                start: MmPoint::new(start.x_mm + 1., start.y_mm + 2.),
                end: MmPoint::new(end.x_mm + 1., end.y_mm + 2.),
                width_mm
            }
        );
    }
    m.run(Action::History(false));
    assert!(m.view.error.is_none());
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        before.layers[0].objects
    );
    m.run(Action::History(true));
    assert!(m.view.error.is_none());
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        after.layers[0].objects
    );
    std::fs::remove_dir_all(dir).unwrap();
}
