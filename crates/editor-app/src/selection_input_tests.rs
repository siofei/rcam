//! Ordinary synthetic manufacturing/egui regressions; no native input evidence.
use crate::{
    selection::SelectionMode::{self, Add, Remove, Replace},
    state::{Action, Model},
};
use editor_core::{
    BoundsMm,
    command::{Platform, Resolution, Shortcut, ShortcutContext, ShortcutResolver, ids},
};
use editor_service::{ClassStyleUpdate, LayerUpdateParams};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture {
    model: Model,
    dir: PathBuf,
}
impl Fixture {
    fn new(count: usize, layers: usize) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rcam-selection-input-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut source = String::from("%FSLAX46Y46*%%MOMM*%%ADD10C,0.2*%%ADD11R,0.2X0.3*%D10*");
        for n in 0..count {
            source.push_str(&format!(
                "X{}Y{}D03*",
                (n % 400) * 1_000_000,
                (n / 400) * 1_000_000
            ));
        }
        source.push_str("D11*X0Y2000000D03*M02*");
        let paths = (0..layers)
            .map(|n| {
                let p = dir.join(format!("layer{n}.gbr"));
                std::fs::write(&p, &source).unwrap();
                p
            })
            .collect();
        let mut model = Model::default();
        model.run(Action::ImportGerbers(paths));
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        Self { model, dir }
    }
    fn patch(
        &mut self,
        layer: &str,
        visible: Option<bool>,
        selectable: Option<bool>,
        classes: Vec<ClassStyleUpdate>,
    ) {
        let revision = self
            .model
            .view
            .info
            .as_ref()
            .unwrap()
            .workspace_revision
            .clone();
        self.model.run(Action::Layer(LayerUpdateParams {
            layer_id: layer.into(),
            expected_workspace_revision: revision,
            visible,
            selectable,
            classes,
            ..Default::default()
        }));
        assert!(self.model.view.error.is_none());
    }
    fn run(&mut self, action: Action) {
        self.model.run(action);
        assert!(
            self.model.view.error.is_none(),
            "{:?}",
            self.model.view.error
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn rect() -> BoundsMm {
    BoundsMm {
        min_x_mm: -0.5,
        min_y_mm: -0.5,
        max_x_mm: 1.5,
        max_y_mm: 0.5,
    }
}
#[test]
fn select_all_uses_whole_snapshot_stable_order_and_every_selection_filter() {
    let mut f = Fixture::new(3, 2);
    let info = f.model.view.info.clone().unwrap();
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 8);
    let snapshot = f.model.service.render_snapshot(&info.document_id).unwrap();
    let expected: Vec<_> = f
        .model
        .view
        .layers
        .iter()
        .flat_map(|l| {
            snapshot
                .layers
                .iter()
                .find(|s| s.id == l.layer_id)
                .unwrap()
                .objects
                .iter()
                .map(move |o| (l.layer_id.clone(), o.object_id.clone()))
        })
        .collect();
    let actual: Vec<_> = f
        .model
        .view
        .selected
        .ordered
        .iter()
        .map(|o| (o.layer_id.clone(), o.object.object_id.clone()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(f.model.view.info, Some(info.clone()));
    let original = f.model.view.selected.clone();
    let epoch = f.model.view.selection_epoch;
    f.run(Action::SelectAll);
    assert!(
        original
            .ordered
            .shares_storage(&f.model.view.selected.ordered)
    );
    assert_eq!(epoch, f.model.view.selection_epoch);
    let layer = f.model.view.layers[0].layer_id.clone();
    f.patch(&layer, Some(false), None, vec![]);
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 4);
    f.patch(&layer, Some(true), Some(false), vec![]);
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 4);
    f.patch(
        &layer,
        None,
        Some(true),
        vec![ClassStyleUpdate {
            class: Some(editor_core::workspace::DisplayClass::FlashCircle),
            visible: Some(false),
            ..Default::default()
        }],
    );
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 5);
    f.patch(
        &layer,
        None,
        None,
        vec![ClassStyleUpdate {
            class: Some(editor_core::workspace::DisplayClass::FlashCircle),
            visible: Some(true),
            selectable: Some(false),
            ..Default::default()
        }],
    );
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 5);
    // Existing policy: locks allow inspection, while the service refuses edits.
    let revision = f
        .model
        .view
        .info
        .as_ref()
        .unwrap()
        .workspace_revision
        .clone();
    f.run(Action::Layer(LayerUpdateParams {
        layer_id: layer.clone(),
        expected_workspace_revision: revision,
        locked: Some(true),
        reset_classes: true,
        ..Default::default()
    }));
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 8);
    assert!(!crate::drag::editable_selection(&f.model.view));
    assert_eq!(f.model.view.info.as_ref().unwrap().revision, info.revision);
    assert_eq!(
        f.model.view.info.as_ref().unwrap().undo_entries,
        info.undo_entries
    );
}
#[test]
fn select_all_respects_solo_without_changing_manufacturing_or_viewport() {
    let mut f = Fixture::new(3, 2);
    let layer = f.model.view.layers[0].layer_id.clone();
    f.run(Action::SetSoloLayer(Some(layer.clone())));
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 4);
    assert!(
        f.model
            .view
            .selected
            .ordered
            .iter()
            .all(|o| o.layer_id == layer)
    );
    // Deliberately tiny render coverage does not restrict full manufacturing selection.
    f.run(Action::SetSoloLayer(None));
    f.model.view.render_viewport = Some(rect());
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 8);
}
#[test]
fn rectangle_add_remove_preserves_old_order_primary_and_noop_snapshot() {
    let mut f = Fixture::new(3, 1);
    f.run(Action::SelectAll);
    let all = f.model.view.selected.clone();
    f.run(Action::CanvasSelectRect(
        rect(),
        editor_core::hit_test::SelectRectMode::Window,
        Remove,
    ));
    assert_eq!(f.model.view.selected.ordered.len(), 2);
    assert_eq!(f.model.view.selected.ordered.as_slice(), &all.ordered[2..]);
    let retained = f.model.view.selected.clone();
    let epoch = f.model.view.selection_epoch;
    f.run(Action::CanvasSelectRect(
        rect(),
        editor_core::hit_test::SelectRectMode::Window,
        Remove,
    ));
    assert!(
        retained
            .ordered
            .shares_storage(&f.model.view.selected.ordered)
    );
    assert_eq!(epoch, f.model.view.selection_epoch);
    f.run(Action::CanvasSelectRect(
        rect(),
        editor_core::hit_test::SelectRectMode::Crossing,
        Add,
    ));
    let reordered = f.model.view.selected.clone();
    assert_eq!(
        reordered.ordered.as_slice(),
        [
            all.ordered[2].clone(),
            all.ordered[3].clone(),
            all.ordered[0].clone(),
            all.ordered[1].clone()
        ]
        .as_slice()
    );
    f.run(Action::CanvasSelectRect(
        rect(),
        editor_core::hit_test::SelectRectMode::Window,
        Add,
    ));
    assert!(
        reordered
            .ordered
            .shares_storage(&f.model.view.selected.ordered)
    );
    f.run(Action::CanvasSelectRect(
        rect(),
        editor_core::hit_test::SelectRectMode::Window,
        Replace,
    ));
    assert_eq!(f.model.view.selected.ordered.as_slice(), &all.ordered[..2]);
    assert_eq!(all.ordered.len(), 4); // Published old Arc never mutated.
}
#[test]
fn modifier_box_from_selected_hit_never_arms_move_and_keeps_press_mode() {
    let mut f = Fixture::new(3, 1);
    f.run(Action::SelectAll);
    let c = crate::camera::Camera::default();
    let r = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400., 400.));
    let p = c.screen(editor_core::MmPoint::new(0., 0.), r);
    for mode in [Add, Remove] {
        let mut g = crate::drag::Gesture::arm(&f.model.view, p, c, r, 1., mode);
        f.run(Action::ProbeDrag(
            editor_core::MmPoint::new(0., 0.),
            c.tolerance(1.),
        ));
        assert!(f.model.view.press_hit.is_some());
        g.confirm(&f.model.view);
        g.update(p + egui::vec2(40., 30.));
        assert!(!g.movement_armed());
        assert!(g.preview_rect().is_some());
        assert_eq!(g.delta, editor_core::MmPoint::new(0., 0.));
        assert!(matches!(g.release(),Some(Action::CanvasSelectRect(_,_,m)) if m==mode));
    }
}
#[test]
fn physical_control_defaults_and_shift_priority_are_exact_on_mac_and_windows() {
    use editor_core::command::Modifiers;
    for platform in [Platform::MacOs, Platform::Windows] {
        let map = crate::shortcut_config::validate(
            crate::shortcut_config::Config::defaults(platform),
            platform,
        )
        .unwrap()
        .keymap;
        let control = egui::Modifiers {
            ctrl: true,
            command: platform == Platform::Windows,
            ..Default::default()
        };
        let logical = crate::shortcut_settings::logical(control, platform);
        assert_eq!(
            logical,
            if platform == Platform::MacOs {
                Modifiers {
                    secondary: true,
                    ..Modifiers::NONE
                }
            } else {
                Modifiers::PRIMARY
            }
        );
        assert_eq!(
            ShortcutResolver::resolve(
                &map,
                &[ShortcutContext::Canvas],
                Shortcut::new(logical, editor_core::command::Key::Char('a'))
            ),
            Resolution::Command(ids::EDIT_SELECT_ALL)
        );
        assert_eq!(SelectionMode::from_modifiers(control), Add);
        assert_eq!(
            SelectionMode::from_modifiers(egui::Modifiers {
                shift: true,
                ..control
            }),
            Remove
        );
    }
    let cmd = egui::Modifiers {
        mac_cmd: true,
        command: true,
        ..Default::default()
    };
    assert_eq!(SelectionMode::from_modifiers(cmd), Replace);
    let map = crate::shortcut_config::validate(
        crate::shortcut_config::Config::defaults(Platform::MacOs),
        Platform::MacOs,
    )
    .unwrap()
    .keymap;
    assert_eq!(
        ShortcutResolver::resolve(
            &map,
            &[ShortcutContext::Canvas],
            Shortcut::new(
                crate::shortcut_settings::logical(cmd, Platform::MacOs),
                editor_core::command::Key::Char('a')
            )
        ),
        Resolution::Unbound
    );
}
#[test]
fn large_complete_selection_and_set_algebra_reuse_noops_without_extra_history() {
    let mut f = Fixture::new(80_000, 1);
    let info = f.model.view.info.clone();
    f.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 80_001);
    let original = f.model.view.selected.clone();
    let epoch = f.model.view.selection_epoch;
    f.run(Action::SelectAll);
    assert!(
        original
            .ordered
            .shares_storage(&f.model.view.selected.ordered)
    );
    assert_eq!(epoch, f.model.view.selection_epoch);
    f.run(Action::CanvasSelectRect(
        rect(),
        editor_core::hit_test::SelectRectMode::Window,
        Add,
    ));
    assert!(
        original
            .ordered
            .shares_storage(&f.model.view.selected.ordered)
    );
    f.run(Action::CanvasSelectRect(
        rect(),
        editor_core::hit_test::SelectRectMode::Window,
        Remove,
    ));
    assert_eq!(f.model.view.selected.ordered.len(), 79_999);
    assert_eq!(original.ordered.len(), 80_001);
    assert_eq!(f.model.view.info, info);
}
#[test]
fn set_algebra_deduplicates_layer_and_object_pairs_and_keeps_primary_stable() {
    let mut f = Fixture::new(2, 1);
    f.run(Action::SelectAll);
    let original = f.model.view.selected.clone();
    let o = &original.ordered[0];
    let mut s = crate::selection::SelectionSet::default();
    s.apply(
        [("a", &o.object), ("a", &o.object), ("b", &o.object)],
        Replace,
    );
    assert_eq!(s.ordered.len(), 2);
    assert_eq!(s.primary().unwrap().layer_id, "b");
    let before = s.clone();
    s.apply([("a", &o.object)], Add);
    assert!(s.ordered.shares_storage(&before.ordered));
    s.apply([("a", &o.object), ("a", &o.object)], Remove);
    assert_eq!(s.ordered.len(), 1);
    assert_eq!(s.primary().unwrap().layer_id, "b");
}
#[test]
fn cancellation_during_set_copy_never_installs_partial_selection() {
    let mut f = Fixture::new(600, 1);
    f.run(Action::SelectAll);
    let original = f.model.view.selected.clone();
    for mode in [Replace, Add, Remove] {
        let mut selected = if mode == Replace {
            crate::selection::SelectionSet::default()
        } else {
            original.clone()
        };
        let before = selected.clone();
        let mut calls = 0;
        // Remove reaches its copy phase; Replace reaches the first copy phase.
        let abort = if mode == Remove { 7 } else { 5 };
        let items = original
            .ordered
            .iter()
            .map(|o| (o.layer_id.as_str(), &o.object));
        let result = selected.apply_checked(items, mode, || {
            calls += 1;
            if calls == abort {
                Err(editor_service::ServiceError {
                    code: "CANCELLED".into(),
                    message: "synthetic checkpoint".into(),
                    details: serde_json::json!({}),
                })
            } else {
                Ok(())
            }
        });
        assert!(result.is_err(), "{mode:?} {calls}");
        assert!(selected.ordered.shares_storage(&before.ordered));
        assert_eq!(selected, before);
    }
}
#[test]
fn focused_text_ime_modal_and_pending_selection_do_not_dispatch_control_all() {
    use editor_core::command::CommandDispatcher;
    let f = Fixture::new(3, 1);
    for blocked in ["text", "ime", "ime-event", "modal"] {
        let mut app = crate::modal::tests::app();
        app.view = f.model.view.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        app.tx = tx;
        let control = egui::Modifiers {
            ctrl: true,
            command: Platform::current() == Platform::Windows,
            ..Default::default()
        };
        app.shortcuts.presses = vec![(egui::Key::A, control)];
        app.ime_active = blocked == "ime";
        app.ime_event = blocked == "ime-event";
        let ctx = egui::Context::default();
        let mut text = "unchanged text".to_owned();
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ctx| {
                if blocked == "text" {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.text_edit_singleline(&mut text).request_focus();
                    });
                    assert!(ctx.wants_keyboard_input());
                }
                app.route_shortcuts(ctx, ctx.wants_keyboard_input(), blocked == "modal");
            },
        );
        assert!(rx.try_recv().is_err(), "{blocked}");
        assert_eq!(text, "unchanged text");
        assert!(app.view.selected.ordered.is_empty());
    }
    let mut app = crate::modal::tests::app();
    app.view = f.model.view.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(4);
    app.tx = tx;
    assert!(app.dispatch(ids::EDIT_SELECT_ALL));
    assert!(app.selection_read_pending());
    assert!(!app.busy);
    assert!(matches!(rx.try_recv().unwrap().2, Action::SelectAll));
    assert!(!app.dispatch(ids::EDIT_SELECT_ALL));
    assert!(rx.try_recv().is_err());
}

#[test]
fn shortcut_migration_preserves_custom_conflicts_and_physical_default_roundtrip() {
    use editor_core::command::{Key, Modifiers};
    for platform in [Platform::MacOs, Platform::Windows] {
        let defaults = crate::shortcut_config::validate(
            crate::shortcut_config::Config::defaults(platform),
            platform,
        )
        .unwrap()
        .config;
        let control = defaults.entry(ids::EDIT_SELECT_ALL).shortcuts.clone();
        let mut legacy = defaults.clone();
        legacy
            .bindings
            .retain(|e| e.command_id != ids::EDIT_SELECT_ALL.0);
        legacy
            .bindings
            .iter_mut()
            .find(|e| e.command_id == ids::VIEW_FIT.0)
            .unwrap()
            .shortcuts = control.clone();
        let migrated = crate::shortcut_config::validate(legacy, platform).unwrap();
        assert_eq!(migrated.missing, [ids::EDIT_SELECT_ALL.0]);
        assert_eq!(migrated.default_conflicts, [ids::EDIT_SELECT_ALL.0]);
        assert!(
            migrated
                .config
                .entry(ids::EDIT_SELECT_ALL)
                .shortcuts
                .is_empty()
        );
        assert_eq!(migrated.config.entry(ids::VIEW_FIT).shortcuts, control);
        let other = if platform == Platform::MacOs {
            Platform::Windows
        } else {
            Platform::MacOs
        };
        let transferred = crate::shortcut_config::validate(defaults.clone(), other).unwrap();
        assert!(transferred.cross_platform);
        assert_eq!(
            transferred.config.source_platform,
            crate::shortcut_config::SourcePlatform::for_platform(platform)
        );
        let roundtrip = crate::shortcut_config::validate(transferred.config, platform).unwrap();
        assert!(!roundtrip.cross_platform);
        assert_eq!(roundtrip.config, defaults);
        let mut custom = defaults.clone();
        custom
            .bindings
            .iter_mut()
            .find(|e| e.command_id == ids::EDIT_SELECT_ALL.0)
            .unwrap()
            .shortcuts = vec![Shortcut::new(Modifiers::NONE, Key::F(8))];
        let transferred = crate::shortcut_config::validate(custom, other).unwrap();
        assert_eq!(
            transferred.config.entry(ids::EDIT_SELECT_ALL).shortcuts,
            [Shortcut::new(Modifiers::NONE, Key::F(8))]
        );
    }
    // A remapped explicit default may collide with another explicit binding:
    // reject the whole import rather than replacing either user choice.
    let mut mac = crate::shortcut_config::Config::defaults(Platform::MacOs);
    mac.bindings
        .iter_mut()
        .find(|e| e.command_id == ids::VIEW_FIT.0)
        .unwrap()
        .shortcuts = vec![Shortcut::new(Modifiers::PRIMARY, Key::Char('a'))];
    assert_eq!(
        crate::shortcut_config::validate(mac, Platform::Windows)
            .unwrap_err()
            .code,
        "Conflict"
    );
}

#[test]
fn custom_mac_command_all_preserves_raw_identity_through_windows_roundtrip() {
    use editor_core::command::{Key, Modifiers};
    let mut mac = crate::shortcut_config::Config::defaults(Platform::MacOs);
    mac.bindings
        .iter_mut()
        .find(|e| e.command_id == ids::EDIT_SELECT_ALL.0)
        .unwrap()
        .shortcuts = vec![Shortcut::new(Modifiers::PRIMARY, Key::Char('a'))];
    let mac = crate::shortcut_config::validate(mac, Platform::MacOs)
        .unwrap()
        .config;
    let win = crate::shortcut_config::validate(mac.clone(), Platform::Windows).unwrap();
    assert_eq!(win.config, mac);
    let restored = crate::shortcut_config::validate(win.config, Platform::MacOs).unwrap();
    assert_eq!(restored.config, mac);
    assert_eq!(
        restored
            .config
            .effective_shortcuts(restored.config.entry(ids::EDIT_SELECT_ALL), Platform::MacOs),
        [Shortcut::new(Modifiers::PRIMARY, Key::Char('a'))]
    );
    let mut illegal = crate::shortcut_config::Config::defaults(Platform::Windows);
    illegal
        .bindings
        .iter_mut()
        .find(|e| e.command_id == ids::VIEW_FIT.0)
        .unwrap()
        .shortcuts = vec![Shortcut::new(Modifiers::PRIMARY, Key::Char('a'))];
    assert_eq!(
        crate::shortcut_config::validate(illegal, Platform::MacOs)
            .unwrap_err()
            .code,
        "Conflict"
    );
}
