use crate::{
    camera::Camera,
    drag::Gesture,
    interaction::{Cursor, Preferences},
    selection::SelectionMode,
    state::{Action, Model},
    status_bar,
};
use editor_core::{BoundsMm, MmPoint, grip::GripFeatureId};
use editor_service::{SelectionCentersParams, SelectionMaterialSemantics};
use eframe::egui::{self, Pos2, Rect, Vec2};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rcam-i2-c-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}
fn model() -> Model {
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/s5i2b");
    m.run(Action::ImportGerbers(
        ["layer_a.gbr", "layer_b.gbr", "layer_c.gbr"]
            .map(|p| root.join(p))
            .to_vec(),
    ));
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: -2.,
            min_y_mm: -2.,
            max_x_mm: 22.,
            max_y_mm: 6.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert!(m.view.error.is_none());
    assert_eq!(m.view.selected.ordered.len(), 5);
    centers(&mut m);
    m
}
fn centers(m: &mut Model) {
    let identity = crate::state::selection_geometry_identity(&m.view);
    let groups = m.view.selected.groups();
    m.run(Action::SelectionCenters(
        identity,
        SelectionCentersParams {
            groups,
            semantics: SelectionMaterialSemantics::SelectedLayerComposite,
        },
    ));
}
fn drag(m: &Model, enabled: bool) -> Gesture {
    let mut v = m.view.clone();
    v.drag_hit = true;
    v.press_hit = v.selected.primary().cloned();
    let c = Camera {
        scale: 20.,
        ..Default::default()
    };
    let r = Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.));
    let p = c.screen(MmPoint::new(20., 4.), r);
    let mut g =
        Gesture::arm(&v, p, c, r, 2., SelectionMode::Replace).with_movement_enabled(enabled);
    g.confirm(&v);
    g.update(p + Vec2::new(20., 10.));
    g
}
#[cfg(feature = "internal-evidence")]
#[test]
fn c_native_block_helper_requires_committed_definition_through_undo_redo() {
    use crate::block_ui::{Context, Edit, Request, create_targets};
    use crate::native_i1::block_place_definition;
    let mut m = model();
    m.run(Action::Select(
        MmPoint::new(20., 4.),
        0.01,
        SelectionMode::Replace,
    ));
    assert_eq!(m.view.selected.ordered.len(), 1);
    let before = m.view.snap_snapshot.clone();
    let info = m.view.info.clone();
    assert!(block_place_definition(&m.view).is_err());
    assert_eq!(m.view.snap_snapshot, before);
    assert_eq!(m.view.info, info);
    let (layer_id, object_ids) = create_targets(&m.view).unwrap();
    m.run(Action::BlockEdit(Box::new(Request {
        context: Context::capture(&m.view).unwrap(),
        edit: Edit::Create(editor_service::CreateBlockDefinitionParams {
            layer_id,
            object_ids,
            local_origin_mm: editor_service::PivotMm {
                x_mm: 20.,
                y_mm: 4.,
            },
            name: "helper prerequisite".into(),
        }),
    })));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let committed = block_place_definition(&m.view).unwrap();
    assert_eq!(m.view.block_definitions.len(), 1);
    let created = m.view.snap_snapshot.clone();
    m.run(Action::History(false));
    assert!(m.view.block_definitions.is_empty());
    let undone_info = m.view.info.clone();
    assert!(block_place_definition(&m.view).is_err());
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers,
        before.as_ref().unwrap().layers
    );
    assert_eq!(m.view.info, undone_info);
    m.run(Action::History(true));
    assert_eq!(block_place_definition(&m.view).unwrap(), committed);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers,
        created.as_ref().unwrap().layers
    );
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().block_definitions,
        created.as_ref().unwrap().block_definitions
    );
}

#[cfg(feature = "internal-evidence")]
#[test]
fn c_native_widget_click_requires_release_inside_original_button() {
    fn click(foreign_move: bool) -> bool {
        let ctx = egui::Context::default();
        let frame = |events| {
            let mut rect = Rect::NOTHING;
            let mut clicked = false;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640., 480.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = ui.button("以此基点创建 Block");
                        rect = response.rect;
                        clicked = response.clicked();
                    });
                },
            );
            (rect, clicked)
        };
        let (rect, _) = frame(vec![]);
        let start = rect.center();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        assert!(!frame(vec![egui::Event::PointerMoved(start), button(start, true)]).1);
        let release = if foreign_move {
            start + Vec2::new(250., 150.)
        } else {
            start
        };
        frame(vec![
            egui::Event::PointerMoved(release),
            button(release, false),
        ])
        .1
    }
    assert!(click(false));
    assert!(!click(true));
}

#[test]
fn c_four_preference_combinations_preserve_selection_and_explicit_service_edits() {
    for movement in [false, true] {
        for grip in [false, true] {
            let mut m = model();
            let before = m.view.snap_snapshot.clone();
            let info = m.view.info.clone();
            let selected = m.view.selected.clone();
            let g = drag(&m, movement);
            let action = g.release();
            assert_eq!(matches!(action, Some(Action::DragMove(_))), movement);
            if let Some(action) = action {
                m.run(action);
                assert!(m.view.error.is_none());
                assert_eq!(
                    m.view.info.as_ref().unwrap().undo_entries,
                    info.as_ref().unwrap().undo_entries + 1
                );
                m.run(Action::History(false));
                assert_eq!(
                    m.view.snap_snapshot.as_ref().unwrap().layers,
                    before.as_ref().unwrap().layers
                );
            } else {
                assert_eq!(m.view.info, info);
                assert_eq!(m.view.selected, selected);
            }
            let mut app = crate::modal::tests::app();
            app.prefs.interaction = Preferences {
                drag_move: movement,
                grip_edit: grip,
                cursor: Cursor::Normal,
            };
            app.view = m.view.clone();
            // Turning mouse gestures off never gates explicit commands.
            let before = m.view.info.as_ref().unwrap().undo_entries;
            m.run(Action::Move("1".into(), "2".into()));
            assert!(m.view.error.is_none());
            assert_eq!(m.view.info.as_ref().unwrap().undo_entries, before + 1);
            let mut view = m.view.clone();
            view.drag_hit = true;
            view.press_hit = view.selected.primary().cloned();
            let c = Camera::default();
            let r = Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.));
            let p = Pos2::new(200., 200.);
            let mut click = Gesture::arm(&view, p, c, r, 1., SelectionMode::Replace)
                .with_movement_enabled(movement);
            click.confirm(&view);
            assert!(matches!(click.release(), Some(Action::CanvasSelect(..))));
            view.drag_hit = false;
            view.press_hit = None;
            let mut box_select = Gesture::arm(&view, p, c, r, 1., SelectionMode::Replace)
                .with_movement_enabled(movement);
            box_select.confirm(&view);
            box_select.update(p + Vec2::splat(30.));
            assert!(matches!(box_select.release(), Some(Action::SelectRect(..))));
        }
    }
}
#[test]
fn c_live_off_discards_ready_drag_before_release_and_never_rearms_old_press() {
    let m = model();
    let mut app = crate::modal::tests::app();
    app.view = m.view.clone();
    let before = app.view.info.clone();
    let selection = app.view.selected.clone();
    app.drag = Some(drag(&m, true));
    assert!(app.drag.as_ref().unwrap().movement_armed());
    app.prefs.interaction.grip_edit = false;
    app.fence_mouse_preferences();
    assert!(app.drag.is_some(), "independent Grip switch");
    app.prefs.interaction.drag_move = false;
    app.fence_mouse_preferences();
    assert!(app.drag.is_none());
    assert_eq!(app.view.info, before);
    assert_eq!(app.view.selected, selection);
    let g = drag(&m, false);
    app.drag = Some(g);
    app.prefs.interaction.drag_move = true;
    app.fence_mouse_preferences();
    assert!(!app.drag.as_ref().unwrap().movement_armed());
    assert!(app.drag.take().unwrap().release().is_none());
}
#[test]
fn c_live_grip_off_discards_actual_changed_manufacturing_preview() {
    let mut m = model();
    m.view.selected.ordered.truncate(1);
    m.view.layers[0].is_active = true;
    let mut app = crate::modal::tests::app();
    app.view = m.view.clone();
    let before = m.view.snap_snapshot.clone();
    let selected = app.view.selected.clone();
    let info = app.view.info.clone();
    app.grip = crate::grip::Session::arm(&app.view, GripFeatureId::Right);
    let g = app.grip.as_mut().expect("rectangle right edge Grip");
    g.update(MmPoint::new(3., 1.));
    assert!(g.moved);
    assert_ne!(g.preview.as_ref().unwrap().geometry, g.object.geometry);
    app.prefs.interaction.drag_move = false;
    app.fence_mouse_preferences();
    assert!(app.grip.is_some());
    app.prefs.interaction.grip_edit = false;
    app.fence_mouse_preferences();
    assert!(app.grip.is_none());
    assert_eq!(app.view.info, info);
    assert_eq!(app.view.selected, selected);
    assert_eq!(m.view.snap_snapshot, before);
}
#[test]
fn c_preferences_old_defaults_and_all_choices_survive_explicit_disk_roundtrip() {
    let path = path("preferences.json");
    std::fs::write(&path, b"{\"panel_width\":300}").unwrap();
    let old = crate::preferences::AppPreferences::load(&path);
    assert_eq!(old.interaction, Preferences::default());
    assert_eq!(old.panel_width, Some(300.));
    for drag in [false, true] {
        for grip in [false, true] {
            for cursor in [Cursor::Normal, Cursor::SmallCross, Cursor::LargeCross] {
                let mut p = old.clone();
                p.interaction = Preferences {
                    drag_move: drag,
                    grip_edit: grip,
                    cursor,
                };
                p.save(&path).unwrap();
                let restored = crate::preferences::AppPreferences::load(&path);
                assert_eq!(restored.interaction, p.interaction);
                assert_eq!(restored.panel_width, p.panel_width);
            }
        }
    }
    std::fs::write(&path, b"{\"interaction\":{\"cursor\":\"unknown\"}}").unwrap();
    assert_eq!(
        crate::preferences::AppPreferences::load(&path).interaction,
        Preferences::default()
    );
}
#[test]
fn c_status_uses_material_composition_not_object_sums_and_keeps_centers_consistent() {
    let m = model();
    let r = crate::point_input::current_centers(&m.view).unwrap();
    let editor_service::SelectionMaterialResult::Computed {
        value:
            editor_service::CompositeMaterial::Ready {
                area_mm2,
                perimeter_mm,
                ..
            },
    } = r.material
    else {
        panic!()
    };
    assert!((area_mm2 - (9. + 1.875 * std::f64::consts::PI)).abs() < 1e-9);
    assert!((perimeter_mm - (20. + 5. * std::f64::consts::PI)).abs() < 1e-9);
    let fields = status_bar::fields(&m.view, crate::tools::DisplayUnit::Millimeter, 0.0001);
    assert_eq!(fields.state, "ready");
    assert_eq!(fields.selection, "选中 5 个图形");
    assert!(fields.tooltip.contains("误差界"));
    assert!(
        !fields
            .selection
            .contains(&m.view.selected.primary().unwrap().object.object_id)
    );
    assert!(crate::point_input::center(&m.view, true).is_ok());
    for unit in crate::tools::DisplayUnit::ALL {
        let f = status_bar::fields(&m.view, unit, 0.0001);
        assert_eq!(f.state, "ready");
        assert!(f.tooltip.contains(&format!("{area_mm2:.12}")));
    }
}
#[test]
fn c_status_overlap_clear_refill_cross_layer_and_zero_have_independent_truth() {
    let a = path("ordered.gbr");
    let b = path("layer.gbr");
    std::fs::write(&a,"%FSLAX46Y46*%\n%MOMM*%\n%ADD10R,4X4*%\n%ADD11R,2X2*%\n%ADD12R,1X1*%\nD10*\nX0Y0D03*\nX2000000Y0D03*\n%LPC*%\nD11*\nX1000000Y0D03*\n%LPD*%\nD12*\nX1000000Y0D03*\nM02*\n").unwrap();
    std::fs::write(
        &b,
        "%FSLAX46Y46*%\n%MOMM*%\n%ADD10R,4X4*%\nD10*\nX0Y0D03*\nM02*\n",
    )
    .unwrap();
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    m.run(Action::ImportGerbers(vec![a, b]));
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: -3.,
            min_y_mm: -3.,
            max_x_mm: 5.,
            max_y_mm: 3.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert_eq!(m.view.selected.ordered.len(), 5);
    centers(&mut m);
    let r = crate::point_input::current_centers(&m.view).unwrap();
    assert!(
        matches!(r.material,editor_service::SelectionMaterialResult::Computed{value:editor_service::CompositeMaterial::Ready{area_mm2,perimeter_mm,..}} if (area_mm2-37.).abs()<1e-9 && (perimeter_mm-48.).abs()<1e-9)
    );
    m.view
        .selected
        .ordered
        .retain(|o| o.object.exposure == editor_core::Exposure::Clear);
    m.view.selection_epoch += 1;
    centers(&mut m);
    // Selection normalization fences the first manual-context query.
    centers(&mut m);
    let f = status_bar::fields(&m.view, crate::tools::DisplayUnit::Millimeter, 0.0001);
    assert_eq!(f.state, "zero");
    assert!(crate::point_input::center(&m.view, true).is_err());
    assert!(f.tooltip.contains("面积为零"));
}
#[test]
fn c_status_stale_matrix_never_reuses_previous_material_values() {
    let m = model();
    for kind in 0..8 {
        let mut v = m.view.clone();
        match kind {
            0 => v.selection_epoch += 1,
            1 => v.task_generation += 1,
            2 => v.rule_revision += 1,
            3 => v.info.as_mut().unwrap().revision = "999".into(),
            4 => v.info.as_mut().unwrap().workspace_revision = "999".into(),
            5 => v.info.as_mut().unwrap().document_id = "new-document".into(),
            6 => {
                v.info
                    .as_mut()
                    .unwrap()
                    .manufacturing_precision
                    .resolution_mm = 0.001
            }
            _ => {
                v.selected.ordered.clear();
            }
        }
        let f = status_bar::fields(&v, crate::tools::DisplayUnit::Millimeter, 0.0001);
        assert_ne!(f.state, "ready", "matrix {kind}");
        assert!(!f.area.contains("14.890"));
        assert!(f.area.is_empty() && f.perimeter.is_empty(), "matrix {kind}");
    }
    let mut v = m.view.clone();
    v.selection_geometry = None;
    v.selection_geometry_error = Some("resource limit".into());
    assert_eq!(
        status_bar::fields(&v, crate::tools::DisplayUnit::Millimeter, 0.0001).state,
        "unavailable"
    );
}

#[test]
fn c_status_only_current_data_is_visible_and_zero_is_a_value() {
    use editor_service::{CompositeMaterial, SelectionMaterialResult};
    let mut m = model();
    let unit = crate::tools::DisplayUnit::Millimeter;
    let ready = status_bar::fields(&m.view, unit, 0.0001);
    assert!(!ready.selection.is_empty() && !ready.area.is_empty() && !ready.perimeter.is_empty());
    let result = std::sync::Arc::make_mut(m.view.selection_geometry.as_mut().unwrap());
    result.material = SelectionMaterialResult::Computed {
        value: CompositeMaterial::ZeroArea,
    };
    let zero = status_bar::fields(&m.view, unit, 0.0001);
    assert_eq!(zero.state, "zero");
    assert!(zero.area.contains('0') && zero.perimeter.contains('0'));
    assert!(zero.notice().is_none());
    m.view.selection_geometry = None;
    let pending = status_bar::fields(&m.view, unit, 0.0001);
    assert_eq!(pending.state, "pending");
    assert!(!pending.selection.is_empty());
    assert!(pending.area.is_empty() && pending.perimeter.is_empty());
    assert!(pending.notice().unwrap().contains("计算中"));
    m.view.selection_geometry_error = Some("resource limit".into());
    let failed = status_bar::fields(&m.view, unit, 0.0001);
    assert_eq!(failed.state, "unavailable");
    assert!(failed.area.is_empty() && failed.perimeter.is_empty());
    assert!(failed.notice().unwrap().contains("不可用"));
    assert!(failed.tooltip.contains("resource limit"));
    m.view.selected.ordered.clear();
    let empty = status_bar::fields(&m.view, unit, 0.0001);
    assert!(empty.selection.is_empty() && empty.area.is_empty() && empty.perimeter.is_empty());
    assert!(empty.notice().is_none());
}

#[test]
fn c_status_hides_only_invalid_numeric_fields_and_keeps_the_warning() {
    use editor_service::{CompositeMaterial, SelectionMaterialResult};
    for (bad_area, bad_perimeter) in [(true, false), (false, true), (true, true)] {
        let mut m = model();
        let result = std::sync::Arc::make_mut(m.view.selection_geometry.as_mut().unwrap());
        let SelectionMaterialResult::Computed {
            value:
                CompositeMaterial::Ready {
                    area_mm2,
                    perimeter_mm,
                    ..
                },
        } = &mut result.material
        else {
            panic!("expected ready material")
        };
        if bad_area {
            *area_mm2 = f64::NAN;
        }
        if bad_perimeter {
            *perimeter_mm = f64::INFINITY;
        }
        let fields = status_bar::fields(&m.view, crate::tools::DisplayUnit::Millimeter, 0.0001);
        assert_eq!(fields.area.is_empty(), bad_area);
        assert_eq!(fields.perimeter.is_empty(), bad_perimeter);
        assert!(!fields.selection.is_empty());
        assert_eq!(fields.state, "unavailable");
        assert!(fields.notice().unwrap().contains("不可用"));
    }
}

#[test]
fn c_status_single_aperture_and_block_descriptions_are_preserved() {
    use editor_core::{
        SemanticGeometry,
        block::{BlockDefinition, BlockDefinitionId, BlockTransform},
    };
    let mut m = model();
    let flash = m
        .view
        .selected
        .ordered
        .iter()
        .find(|item| matches!(item.object.geometry, SemanticGeometry::Flash { .. }))
        .unwrap()
        .clone();
    *m.view.selected.ordered = vec![flash];
    let aperture = status_bar::fields(&m.view, crate::tools::DisplayUnit::Millimeter, 0.0001);
    assert!(aperture.selection.starts_with("光圈 "));
    assert!(!aperture.selection.contains("选中"));
    let id = BlockDefinitionId("presentation-only-test".into());
    m.view.block_definitions.push(BlockDefinition {
        id: id.clone(),
        name: "中文 Block name".into(),
        local_origin: MmPoint::new(0., 0.),
        objects: vec![],
        revision: 1,
    });
    m.view.selected.ordered[0].object.geometry = SemanticGeometry::BlockInstance {
        definition_id: id,
        transform: BlockTransform::IDENTITY,
    };
    let block = status_bar::fields(&m.view, crate::tools::DisplayUnit::Millimeter, 0.0001);
    assert_eq!(block.selection, "Block 中文 Block name");
}
#[test]
fn c_status_rects_are_content_independent_nonoverlapping_and_coords_right() {
    for width in [280., 420., 600., 850., 1200.] {
        let rect = Rect::from_min_size(Pos2::new(5., 9.), Vec2::new(width, 20.));
        let f = status_bar::Fields {
            selection: "selection".into(),
            area: "area".into(),
            perimeter: "perimeter".into(),
            tooltip: "full".into(),
            state: "ready",
        };
        let layout = status_bar::layout(rect, &f, 0.);
        let rect = Rect::from_min_size(rect.min, Vec2::new(width, layout.height));
        let slots = layout.fields;
        assert_eq!(slots[3].unwrap().right(), rect.right());
        for a in slots.iter().flatten() {
            assert!(rect.contains_rect(*a));
            for b in slots.iter().flatten() {
                if a != b {
                    assert!(!a.intersects(*b));
                }
            }
        }
        let ctx = egui::Context::default();
        let mut footprints = vec![];
        for len in [1, 1000] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 100.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let f = status_bar::Fields {
                            selection: "汉".repeat(len),
                            area: "9".repeat(len),
                            perimeter: "9".repeat(len),
                            tooltip: "full".into(),
                            state: "ready",
                        };
                        footprints
                            .push(status_bar::paint(ui, &f, &"-999999".repeat(len), 0.).fields);
                    });
                },
            );
        }
        assert_eq!(footprints[0], footprints[1]);
    }
}
#[test]
fn c_cursor_physical_size_clipping_and_popup_ownership() {
    for ppp in [1., 2.] {
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::new(200., 100.));
        let point = Pos2::new(50., 50.);
        let lines = crate::interaction::segments(Cursor::SmallCross, point, r, ppp);
        assert_eq!(lines[0][0].distance(lines[0][1]) * ppp, 12.);
        for style in [Cursor::SmallCross, Cursor::LargeCross] {
            for point in [r.left_top(), r.center(), r.right_bottom()] {
                for line in crate::interaction::segments(style, point, r, ppp) {
                    assert!(r.contains(line[0]) && r.contains(line[1]));
                }
            }
        }
        assert!(crate::interaction::segments(Cursor::Normal, point, r, ppp).is_empty());
    }
    let ctx = egui::Context::default();
    let mut results = vec![];
    for modal in [false, false, true] {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.))),
                events: vec![egui::Event::PointerMoved(Pos2::new(100., 100.))],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (r, p) =
                        ui.allocate_painter(Vec2::splat(300.), egui::Sense::click_and_drag());
                    results.push(crate::interaction::paint_cursor(
                        ctx,
                        &r,
                        &p,
                        Cursor::SmallCross,
                        modal,
                    ));
                });
            },
        );
        if modal {
            assert_ne!(out.platform_output.cursor_icon, egui::CursorIcon::None);
        }
    }
    assert!(results[1]);
    assert!(!results[2]);
}

#[test]
fn c_untrusted_multiline_labels_cannot_grow_the_status_line() {
    let value = "Block 长名字\n第二行\r\t\u{2028}第三行";
    assert!(
        !crate::status_bar::display_line(value)
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    );
    let ctx = egui::Context::default();
    let mut heights = vec![];
    for text in ["短标签".to_owned(), value.repeat(300)] {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 100.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let f = status_bar::Fields {
                        selection: text.clone(),
                        area: "面积 0".into(),
                        perimeter: "周长 0".into(),
                        tooltip: text.clone(),
                        state: "zero",
                    };
                    ui.scope(|ui| {
                        status_bar::paint(ui, &f, "X 0 Y 0", 0.);
                        heights.push(ui.min_rect().height());
                    });
                });
            },
        );
    }
    assert_eq!(heights[0], heights[1]);
    assert_eq!(heights[1], 44.);
}
#[test]
fn c_cursor_is_ordinary_over_an_actual_foreground_menu_area() {
    let ctx = egui::Context::default();
    let mut shown = vec![];
    for _ in 0..3 {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.))),
                events: vec![egui::Event::PointerMoved(Pos2::new(100., 100.))],
                ..Default::default()
            },
            |ctx| {
                egui::Area::new(egui::Id::new("covering-menu"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(Pos2::new(80., 80.))
                    .show(ctx, |ui| {
                        ui.set_min_size(Vec2::splat(80.));
                        ui.label("菜单");
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (r, p) =
                        ui.allocate_painter(Vec2::splat(300.), egui::Sense::click_and_drag());
                    shown.push(crate::interaction::paint_cursor(
                        ctx,
                        &r,
                        &p,
                        Cursor::LargeCross,
                        false,
                    ));
                });
            },
        );
        assert_ne!(output.platform_output.cursor_icon, egui::CursorIcon::None);
    }
    assert_eq!(shown, vec![false, false, false]);
}
