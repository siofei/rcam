use super::*;
use crate::{object_snap, state::Model, tools, world_index::WorldIndex};
use editor_core::snap::SnapKind;
use editor_service::ObjectInfo;
use eframe::egui::Vec2;
use std::{
    collections::HashSet,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\nD10*\nX10000000Y20000000D03*\nX20000000Y20000000D03*\nM02*\n";

fn fixture() -> Model {
    let path = std::env::temp_dir().join(format!(
        "grip-app-{}-{}.gbr",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, SOURCE).unwrap();
    let mut model = Model::default();
    model.open(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let info = model.view.info.as_ref().unwrap();
    let snapshot = model.service.render_snapshot(&info.document_id).unwrap();
    *model.view.selected.ordered = vec![ObjectInfo {
        layer_id: snapshot.layers[0].id.clone(),
        object: snapshot.layers[0].objects[0].clone(),
    }];
    model
}
fn second(model: &Model) -> ObjectInfo {
    let info = model.view.info.as_ref().unwrap();
    let snapshot = model.service.render_snapshot(&info.document_id).unwrap();
    ObjectInfo {
        layer_id: snapshot.layers[0].id.clone(),
        object: snapshot.layers[0].objects[1].clone(),
    }
}

#[test]
fn grips_need_one_visible_selectable_unlocked_active_selection() {
    let model = fixture();
    assert_eq!(features(&model.view).unwrap().len(), 1);
    assert!(Session::arm(&model.view, GripFeatureId::Radius).is_some());
    let mut view = model.view.clone();
    view.selected.ordered.push(second(&model));
    assert!(features(&view).unwrap().is_empty());
    assert!(Session::arm(&view, GripFeatureId::Radius).is_none());
    for altered in [
        {
            let mut v = model.view.clone();
            v.layers[0].visible = false;
            v
        },
        {
            let mut v = model.view.clone();
            v.layers[0].effective_visible = false;
            v
        },
        {
            let mut v = model.view.clone();
            v.layers[0].selectable = false;
            v
        },
        {
            let mut v = model.view.clone();
            v.layers[0].locked = true;
            v
        },
        {
            let mut v = model.view.clone();
            v.layers[0].is_active = false;
            v
        },
        {
            let mut v = model.view.clone();
            v.blocked = Some("busy".into());
            v
        },
    ] {
        assert!(features(&altered).unwrap().is_empty());
        assert!(Session::arm(&altered, GripFeatureId::Radius).is_none());
    }
}

#[test]
fn hit_radius_uses_physical_pixels_on_retina() {
    let camera = Camera::default();
    let rect = Rect::from_min_size(Pos2::new(0., 0.), Vec2::new(200., 200.));
    let grips = [GripFeature {
        id: GripFeatureId::Radius,
        position_mm: MmPoint::new(0., 0.),
    }];
    let center = camera.screen(MmPoint::new(0., 0.), rect);
    assert_eq!(
        hit(&grips, center + Vec2::new(4.9, 0.), camera, rect, 2.),
        Some(GripFeatureId::Radius)
    );
    assert_eq!(
        hit(&grips, center + Vec2::new(5.1, 0.), camera, rect, 2.),
        None
    );
    assert_eq!(
        hit(&grips, center + Vec2::new(9.9, 0.), camera, rect, 1.),
        Some(GripFeatureId::Radius)
    );
}

#[test]
fn preview_is_transient_and_valid_release_is_one_service_edit() {
    let mut model = fixture();
    let before = model.view.info.clone().unwrap();
    let aperture_before = model.view.apertures.clone();
    let selected_before = model.view.selected.clone();
    let snapshot_before = model.service.render_snapshot(&before.document_id).unwrap();
    let mut session = Session::arm(&model.view, GripFeatureId::Radius).unwrap();
    assert_eq!(
        session.excluded,
        HashSet::from([session.object.object_id.clone()])
    );
    assert!(session.clone().release().is_none());
    session.update(MmPoint::new(12., 20.));
    assert!(session.preview.is_ok());
    assert!(session.valid(&model.view));
    assert_eq!(model.view.info.as_ref().unwrap(), &before);
    assert_eq!(model.view.apertures, aperture_before);
    assert_eq!(model.view.selected, selected_before);
    assert_eq!(
        model.service.render_snapshot(&before.document_id).unwrap(),
        snapshot_before
    );
    let action = session.release().unwrap();
    let Action::GripEdit(ref captured) = action else {
        panic!("release must submit GripEdit")
    };
    assert_eq!(captured.target, MmPoint::new(12., 20.));
    model.run(action);
    assert!(model.view.error.is_none(), "{:?}", model.view.error);
    let after = model.view.info.as_ref().unwrap();
    assert_ne!(after.revision, before.revision);
    assert_eq!(after.undo_entries, before.undo_entries + 1);
    assert!(after.dirty);
}

#[test]
fn invalid_release_and_changed_selection_revision_or_layer_cancel_session() {
    let model = fixture();
    let mut invalid = Session::arm(&model.view, GripFeatureId::Radius).unwrap();
    invalid.update(MmPoint::new(10., 20.));
    assert!(invalid.preview.is_err());
    assert!(invalid.release().is_none());
    let session = Session::arm(&model.view, GripFeatureId::Radius).unwrap();
    let mut changed = model.view.clone();
    changed.selected.ordered.push(second(&model));
    assert!(!session.valid(&changed));
    changed = model.view.clone();
    changed.info.as_mut().unwrap().revision.push('x');
    assert!(!session.valid(&changed));
    changed = model.view.clone();
    changed.selected.ordered[0].layer_id = "other".into();
    assert!(!session.valid(&changed));
    changed = model.view.clone();
    changed.layers[0].locked = true;
    assert!(!session.valid(&changed));
}

#[test]
fn snap_reuses_object_grid_alt_and_excludes_dragged_object() {
    let model = fixture();
    let session = Session::arm(&model.view, GripFeatureId::Radius).unwrap();
    let snapshot = model.service.render_snapshot(&session.document).unwrap();
    let index = WorldIndex::build(&snapshot).unwrap();
    let settings = object_snap::Settings {
        enabled: true,
        enabled_kinds: vec![SnapKind::Center],
        ..Default::default()
    };
    let grid = tools::GridSettings {
        snap_enabled: true,
        spacing_mm: 1.,
        ..Default::default()
    };
    let raw = MmPoint::new(10.02, 20.01);
    let resolve = |excluded: Option<&HashSet<String>>, disabled| {
        object_snap::Runtime::default()
            .resolve(
                raw,
                &settings,
                grid,
                Camera {
                    scale: 100.,
                    ..Default::default()
                },
                2.,
                Some(&snapshot),
                &index,
                &model.view.layers,
                excluded,
                disabled,
            )
            .unwrap()
    };
    assert_eq!(resolve(None, false).kind, Some(SnapKind::Center));
    let excluded = resolve(Some(&session.excluded), false);
    assert!(excluded.from_grid);
    assert_eq!(excluded.point, MmPoint::new(10., 20.));
    let alt = resolve(Some(&session.excluded), true);
    assert_eq!(alt.kind, None);
    assert_eq!(alt.point, raw);
}

#[test]
fn escape_blur_pointer_loss_win_over_release_without_a_transaction() {
    let model = fixture();
    let before = model.view.info.as_ref().unwrap().clone();
    let snapshot = model.service.render_snapshot(&before.document_id).unwrap();
    for (escape, focused, gone) in [
        (true, true, false),
        (false, false, false),
        (false, true, true),
    ] {
        let mut session = Session::arm(&model.view, GripFeatureId::Radius).unwrap();
        session.update(MmPoint::new(12., 20.));
        assert!(session.preview.is_ok());
        assert!(crate::drag::cancelled(escape, focused, gone, false, true));
        drop(session);
        assert_eq!(model.view.info.as_ref().unwrap(), &before);
        assert_eq!(
            model.service.render_snapshot(&before.document_id).unwrap(),
            snapshot
        );
    }
}
