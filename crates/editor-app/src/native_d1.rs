//! Fixed synthetic native evidence workflow. Compiled only with internal-evidence.
//! Uses the real EditorApp worker/service/renderer; never accepts user project paths.
use crate::{EditorApp, block_ui::Context, components_ui::PreviewRequest, state::Action};
use editor_core::{MmPoint, hash::sha256_hex};
use editor_service::*;
use eframe::egui;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
pub fn directory() -> Option<PathBuf> {
    let dir = std::fs::canonicalize(std::env::var_os("RCAM_S4D1_NATIVE_DIR")?).ok()?;
    (dir.parent() == Some(std::path::Path::new("/private/tmp"))
        && dir.file_name()?.to_str()?.starts_with("rcam-s4d1-native-"))
    .then_some(dir)
}
pub struct Run {
    dir: PathBuf,
    phase: usize,
    last: Instant,
    records: Vec<Value>,
    original: Option<Value>,
    registered: Option<Value>,
    failed: bool,
}
impl Run {
    pub fn from_env() -> Option<Self> {
        let dir = directory()?;
        if dir.join("native-observations.json").exists() {
            return None;
        }
        for (name, data) in [
            (
                "board.gbr",
                include_bytes!("../../../fixtures/synthetic/s4d1/board.gbr").as_slice(),
            ),
            (
                "pnp.csv",
                include_bytes!("../../../fixtures/synthetic/s4d1/pnp.csv").as_slice(),
            ),
            (
                "pnp-invalid.csv",
                include_bytes!("../../../fixtures/synthetic/s4d1/pnp-invalid.csv").as_slice(),
            ),
        ] {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(name))
                .ok()?
                .write_all(data)
                .ok()?;
        }
        Some(Self {
            dir,
            phase: 0,
            last: Instant::now(),
            records: vec![],
            original: None,
            registered: None,
            failed: false,
        })
    }
    fn observation(&self, app: &EditorApp, label: &str) -> Value {
        let info = app.view.info.as_ref();
        let snapshot = app.view.snap_snapshot.as_ref();
        json!({"label":label,"input_kind":"controlled instrumented native EditorApp Action path","commit":option_env!("RCAM_BUILD_COMMIT"),"adapter":app.adapter,"component_count":app.view.board.as_ref().map_or(0,|b|b.components.len()),"board":app.view.board.as_ref().map(|b|b.as_ref()),"revision":info.map(|i|&i.revision),"dirty":info.map(|i|i.dirty),"project_dirty":info.map(|i|i.project_dirty),"undo":info.map(|i|i.undo_entries),"redo":info.map(|i|i.redo_entries),"geometry_sha256":snapshot.map(|s|sha256_hex(&serde_json::to_vec(&(&s.apertures,&s.block_definitions,&s.layers)).unwrap())),"object_count":snapshot.map(|s|s.layers.iter().map(|l|l.objects.len()).sum::<usize>()),"diagnostics":app.view.pnp_preview.as_ref().map(|r|&r.result.preview.diagnostics),"preview_valid":app.view.pnp_preview.as_ref().map(|r|r.result.preview.valid()),"mapping":app.components.mapping,"units_confirmed":app.components.units_confirmed,"units_selected":app.components.units_selected,"columns_confirmed":app.components.columns_confirmed,"sample_rows":app.view.pnp_preview.as_ref().map(|r|r.result.preview.sample_rows.len()),"convention_confirmed":app.components.convention_confirmed,"query_results":app.view.component_indices.len(),"focus":app.components.focused,"camera_center":app.camera.center,"camera_scale":app.camera.scale,"overlay":app.components.overlay,"display_unit":app.display_unit.suffix(),"pixels_per_point":app.reported_ppp,"error_code":app.view.error.as_ref().map(|e|&e.code)})
    }
    fn record(&mut self, app: &EditorApp, label: &str) {
        let v = self.observation(app, label);
        self.records.push(v);
    }
    fn fail(&mut self, app: &EditorApp, why: &str) {
        self.record(app, why);
        self.failed = true;
        self.finish(app);
    }
    fn finish(&mut self, app: &EditorApp) {
        let binary_sha256 = std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| sha256_hex(&b));
        let report = json!({"binary_sha256":binary_sha256,"schema_version":2,"stage":"S4-D1","status":if self.failed{"FAIL"}else{"PASS"},"evidence_kind":"instrumented native Apple Silicon / Metal, real EditorApp worker and ApplicationService; no physical-input claim","commit":option_env!("RCAM_BUILD_COMMIT"),"adapter":app.adapter,"records":self.records});
        let _ = std::fs::write(
            self.dir.join("native-observations.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        );
        self.phase = 100;
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(100));
        if self.phase >= 100 || app.busy || self.last.elapsed() < Duration::from_millis(700) {
            return;
        }
        if !app.view.layers.is_empty()
            && (app.view.layers.len() != 1
                || app.view.layers[0].provenance.as_ref().is_none_or(|p| {
                    p.imported_sha256
                        != sha256_hex(include_bytes!("../../../fixtures/synthetic/s4d1/board.gbr"))
                }))
        {
            self.failed = true;
            self.records
                .push(json!({"label":"synthetic_fixture_gate_rejected"}));
            self.finish(app);
            return;
        }
        if app.view.board.as_ref().is_some_and(|b| {
            b.provenance.sha256
                != sha256_hex(include_bytes!("../../../fixtures/synthetic/s4d1/pnp.csv"))
        }) {
            self.failed = true;
            self.records
                .push(json!({"label":"synthetic_pnp_gate_rejected"}));
            self.finish(app);
            return;
        }
        let expected_unregistered_candidate = self.phase == 8
            && app
                .view
                .error
                .as_ref()
                .is_some_and(|e| e.code == "REGISTRATION_REQUIRED");
        if expected_unregistered_candidate {
            let current = self.observation(app, "expected-unregistered-candidate-zero-mutation");
            let baseline = self.records.last().unwrap();
            if baseline["label"] != "uncalibrated-focus-warning"
                || [
                    "revision",
                    "dirty",
                    "project_dirty",
                    "undo",
                    "redo",
                    "geometry_sha256",
                    "component_count",
                ]
                .iter()
                .any(|key| current[*key] != baseline[*key])
                || !app.view.selected.ordered.is_empty()
                || app
                    .view
                    .board
                    .as_ref()
                    .is_none_or(|b| b.registration.is_some())
                || app
                    .components
                    .focused
                    .as_ref()
                    .is_none_or(|f| f.world_position.is_some())
            {
                self.fail(app, "unregistered-candidate-query-mutated-or-wrong-state");
                return;
            }
            self.record(app, "expected-unregistered-candidate-zero-mutation");
        }
        if app.view.error.is_some()
            && !matches!(self.phase, 5 | 6)
            && !expected_unregistered_candidate
        {
            self.fail(app, "unexpected-worker-error");
            return;
        }
        let Some(context) = Context::capture(&app.view) else {
            return;
        };
        match self.phase {
            0 => {
                app.recovery_candidate = None;
                app.send(Action::ImportGerbers(vec![self.dir.join("board.gbr")]));
            }
            1 => {
                self.record(app, "gerber-import");
                app.send(Action::SaveProject(
                    Some(self.dir.join("baseline.rcam")),
                    false,
                    None,
                ));
            }
            2 => {
                self.original = Some(self.observation(app, "baseline"));
                self.record(app, "baseline");
                app.send(Action::Save(
                    self.dir.join("before.gbr"),
                    app.view.layers[0].layer_id.clone(),
                    None,
                ));
            }
            3 => {
                app.open_pnp(self.dir.join("pnp-invalid.csv"));
                app.components.mapping = editor_core::pnp::PnpMapping {
                    source: app.components.mapping.source.clone(),
                    ..crate::components_ui::UiState::default().mapping
                };
                let req = PreviewRequest {
                    context,
                    path: self.dir.join("pnp-invalid.csv"),
                    mapping: app.components.mapping.clone(),
                };
                app.send(Action::PnpPreview(req));
            }
            4 => {
                // Opening now schedules raw-table discovery. Respect the real
                // serialized worker before submitting the explicit fixture map.
                if app
                    .view
                    .pnp_preview
                    .as_ref()
                    .is_some_and(|r| r.request.mapping != app.components.mapping)
                {
                    self.record(app, "raw-table-discovery-before-column-selection");
                    app.send(Action::PnpPreview(PreviewRequest {
                        context,
                        path: app.components.path.clone().unwrap(),
                        mapping: app.components.mapping.clone(),
                    }));
                    self.last = Instant::now();
                    return;
                }
                self.record(app, "invalid-preview");
                let r = app.view.pnp_preview.as_ref().unwrap();
                if r.result.preview.valid() || r.result.preview.diagnostic_count != 3 {
                    self.fail(app, "invalid-preview-accepted");
                    return;
                }
                app.send(Action::PnpImport(
                    context,
                    ImportPnpParams {
                        path: self.dir.join("pnp-invalid.csv").to_string_lossy().into(),
                        mapping: app.components.mapping.clone(),
                        preview_sha256: r.result.sha256.clone(),
                        allow_replace: false,
                    },
                ));
            }
            5 => {
                self.record(app, "invalid-import-zero-mutation");
                let baseline = self.original.as_ref().unwrap();
                let after = self.observation(app, "rejected");
                if after["revision"] != baseline["revision"]
                    || after["geometry_sha256"] != baseline["geometry_sha256"]
                    || after["project_dirty"] != baseline["project_dirty"]
                    || after["undo"] != baseline["undo"]
                    || after["component_count"] != 0
                {
                    self.fail(app, "failed-import-mutated");
                    return;
                }
                app.view.error = None;
                app.cancel_modal();
                app.open_pnp(self.dir.join("pnp.csv"));
                app.components.mapping = editor_core::pnp::PnpMapping {
                    source: app.components.mapping.source.clone(),
                    ..crate::components_ui::UiState::default().mapping
                };
                app.components.units_selected = true;
                app.components.columns_confirmed = true;
                app.components.units_confirmed = true;
                app.components.convention_confirmed = true;
                app.send(Action::PnpPreview(PreviewRequest {
                    context,
                    path: self.dir.join("pnp.csv"),
                    mapping: app.components.mapping.clone(),
                }));
            }
            6 => {
                // Opening now schedules raw-table discovery. Respect the real
                // serialized worker before submitting the explicit fixture map.
                if app
                    .view
                    .pnp_preview
                    .as_ref()
                    .is_some_and(|r| r.request.mapping != app.components.mapping)
                {
                    self.record(app, "raw-table-discovery-before-column-selection");
                    app.send(Action::PnpPreview(PreviewRequest {
                        context,
                        path: app.components.path.clone().unwrap(),
                        mapping: app.components.mapping.clone(),
                    }));
                    self.last = Instant::now();
                    return;
                }
                self.record(app, "valid-mapping-preview");
                let r = app.view.pnp_preview.as_ref().unwrap();
                if !r.result.preview.valid() {
                    self.fail(app, "valid-preview-failed");
                    return;
                }
                app.send(Action::PnpImport(
                    context,
                    ImportPnpParams {
                        path: self.dir.join("pnp.csv").to_string_lossy().into(),
                        mapping: app.components.mapping.clone(),
                        preview_sha256: r.result.sha256.clone(),
                        allow_replace: false,
                    },
                ));
                app.cancel_modal();
                app.components.open = true;
            }
            7 => {
                self.record(app, "import-uncalibrated");
                let Some(board) = app.view.board.as_ref() else {
                    self.fail(app, "missing-components");
                    return;
                };
                let focus = component_info(&board.components[0], board);
                app.focus_component(focus);
                self.record(app, "uncalibrated-focus-warning");
                if app
                    .components
                    .focused
                    .as_ref()
                    .unwrap()
                    .world_position
                    .is_some()
                {
                    self.fail(app, "uncalibrated-world-claimed");
                    return;
                }
            }
            8 => {
                app.components.manual = false;
                app.components.board_points = [[0., 0.], [10., 0.]];
                app.components.reflect = false;
                let mut world = [MmPoint::new(0., 0.); 2];
                for (i, x) in [0., 10.].into_iter().enumerate() {
                    let raw = MmPoint::new(x + 0.01, 0.01);
                    let mut settings = app.object_snap.clone();
                    settings.enabled = true;
                    let resolution = app.object_snap_runtime.resolve(
                        raw,
                        &settings,
                        app.grid,
                        app.camera,
                        app.reported_ppp,
                        app.view.snap_snapshot.as_deref(),
                        &app.view.snap_index,
                        &app.view.layers,
                        None,
                        false,
                    );
                    let Ok(resolution) = resolution else {
                        self.fail(app, "snap-failed");
                        return;
                    };
                    world[i] = resolution.point;
                    app.components.world_points[i] = [world[i].x_mm, world[i].y_mm];
                    if resolution.candidate.is_none()
                        || world[i].distance_mm(MmPoint::new(x, 0.)) > 1e-8
                    {
                        self.fail(app, "snap-center-mismatch");
                        return;
                    }
                }
                app.components.registration_confirmed = true;
                self.record(app, "two-point-object-snap-preview");
                app.send(Action::BoardRegistration(
                    context,
                    app.components.registration_input(),
                ));
            }
            9 => {
                self.record(app, "registered");
                self.registered = Some(self.observation(app, "registered"));
                app.send(Action::ComponentSearch(
                    context.clone(),
                    ComponentQuery {
                        revision: context.revision,
                        query: "C15".into(),
                        mode: RefdesMatch::Exact,
                        side: Some(editor_core::board::BoardSide::Bottom),
                        footprint: Some("0603".into()),
                        offset: 0,
                        limit: 500,
                    },
                ));
            }
            10 => {
                let board = app.view.board.as_ref().unwrap();
                if app.view.component_indices.len() != 1 {
                    self.fail(app, "search-count-mismatch");
                    return;
                }
                let c = &board.components[app.view.component_indices[0]];
                let focus = component_info(c, board);
                app.focus_component(focus);
                self.record(app, "search-bottom-focus-overlay");
            }
            11 => {
                app.display_unit = crate::tools::DisplayUnit::Inch;
                app.components.side = Some(editor_core::board::BoardSide::Top);
                app.components.overlay = false;
                self.record(app, "view-unit-overlay-pure");
                app.components.overlay = true;
                app.send(Action::ComponentSearch(
                    context.clone(),
                    ComponentQuery {
                        revision: context.revision,
                        query: String::new(),
                        mode: RefdesMatch::Prefix,
                        side: Some(editor_core::board::BoardSide::Top),
                        footprint: None,
                        offset: 0,
                        limit: 500,
                    },
                ));
            }
            12 => {
                self.record(app, "search-top-two-results");
                if app.view.component_indices.len() != 2 {
                    self.fail(app, "top-search-count-mismatch");
                    return;
                }
                app.send(Action::History(false));
            }
            13 => {
                self.record(app, "undo-registration");
                if app.view.board.as_ref().unwrap().registration.is_some() {
                    self.fail(app, "undo-registration-failed");
                    return;
                }
                app.send(Action::History(true));
            }
            14 => {
                self.record(app, "redo-registration");
                if self.observation(app, "redo")["board"]
                    != self.registered.as_ref().unwrap()["board"]
                {
                    self.fail(app, "redo-board-mismatch");
                    return;
                }
                app.send(Action::SaveProject(
                    Some(self.dir.join("pnp.rcam")),
                    false,
                    None,
                ));
            }
            15 => {
                self.record(app, "saved");
                app.send(Action::OpenProject(self.dir.join("pnp.rcam"), false));
            }
            16 => {
                self.record(app, "opened");
                if self.observation(app, "open")["board"]
                    != self.registered.as_ref().unwrap()["board"]
                {
                    self.fail(app, "open-board-mismatch");
                    return;
                }
                app.components.manual = true;
                app.components.angle = 37.;
                app.components.translation = [12., -8.];
                app.components.reflect = true;
                app.send(Action::BoardRegistration(
                    context,
                    app.components.registration_input(),
                ));
            }
            17 => {
                self.record(app, "dirty-registration-for-recovery");
                app.send(Action::RecoveryWrite(self.dir.join("state/recovery")));
            }
            18 => {
                self.record(app, "recovery-written");
                let found = crate::recovery::discover(&self.dir.join("state/recovery"));
                let Some(metadata) = found.first() else {
                    self.fail(app, "recovery-file-missing");
                    return;
                };
                let bytes = crate::recovery::load(&self.dir.join("state/recovery"), metadata);
                match bytes {
                    Ok(bytes) => {
                        std::fs::write(self.dir.join("recovery.rcam"), &bytes).unwrap();
                        app.send(Action::RestoreProject(bytes));
                    }
                    Err(_) => {
                        self.fail(app, "recovery-read-failed");
                        return;
                    }
                }
            }
            19 => {
                self.record(app, "recovery-restored");
                app.send(Action::Save(
                    self.dir.join("after.gbr"),
                    app.view.layers[0].layer_id.clone(),
                    None,
                ));
            }
            20 => {
                self.record(app, "export-zero-component-side-effects");
                if std::fs::read(self.dir.join("before.gbr")).unwrap()
                    != std::fs::read(self.dir.join("after.gbr")).unwrap()
                {
                    self.fail(app, "gerber-bytes-changed");
                    return;
                }
                if let Some(runtime) = rcam_diagnostics::global() {
                    let context = editor_service::diagnostic_context(
                        app.view.info.as_ref().unwrap(),
                        &app.view.layers,
                        app.view.snap_snapshot.as_deref(),
                        app.display_unit,
                    );
                    if runtime
                        .export_with_context(&self.dir.join("diagnostics.zip"), context)
                        .is_err()
                    {
                        self.fail(app, "diagnostic-export-failed");
                        return;
                    }
                } else {
                    self.fail(app, "no-diagnostic-runtime");
                    return;
                }
                self.record(app, "diagnostics-exported");
                if let Some(board) = &app.view.board {
                    let focus = component_info(&board.components[1], board);
                    app.camera.center = focus.world_position.unwrap();
                    app.components.focus_revision =
                        app.view.info.as_ref().unwrap().revision.clone();
                    app.components.focused = Some(focus);
                    app.fit = false;
                }
                app.view.message = "S4-D1 synthetic native workflow complete".into();
                self.finish(app);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    "d1-final",
                )));
            }
            _ => {}
        }
        if self.phase < 100 {
            self.phase += 1;
        }
        self.last = Instant::now();
    }
}
