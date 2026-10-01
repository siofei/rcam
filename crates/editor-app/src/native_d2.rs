//! Synthetic-only controlled native workflow. Default public builds exclude it.
use crate::{EditorApp, block_ui::Context, components_ui::PreviewRequest, state::Action};
use editor_core::{
    MmPoint, board::CoordinateTransform2D, hash::sha256_hex, pnp::RegistrationInput,
};
use editor_service::*;
use eframe::egui;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub fn directory() -> Option<PathBuf> {
    let dir = std::fs::canonicalize(std::env::var_os("RCAM_S4D2_NATIVE_DIR")?).ok()?;
    (dir.parent() == Some(Path::new("/private/tmp"))
        && dir.file_name()?.to_str()?.starts_with("rcam-s4d2-native-"))
    .then_some(dir)
}
pub struct Run {
    dir: PathBuf,
    phase: usize,
    last: Instant,
    records: Vec<Value>,
    pnp_hash: String,
    failed: bool,
    baseline_saved: bool,
    baseline_exported: bool,
    searched: bool,
    frame_ticks: u64,
    last_frame: Instant,
    max_frame_gap_ms: f64,
}
impl Run {
    pub fn from_env() -> Option<Self> {
        let dir = directory()?;
        if dir.join("native-observations.json").exists() {
            return None;
        }
        let mut csv = String::from(
            "RefDes,X,Y,Rotation,Side,Footprint,Value\nC15,0,0,37,Top,Synthetic,none\nC16,0,0,37,Bottom,Synthetic,none\n",
        );
        for i in 2..100000 {
            csv.push_str(&format!("R{i},100,100,0,Top,Synthetic,none\n"));
        }
        for (name, data) in [
            (
                "board.gbr",
                include_bytes!("../../../fixtures/synthetic/s4d2/board.gbr").as_slice(),
            ),
            ("pnp.csv", csv.as_bytes()),
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
            pnp_hash: sha256_hex(csv.as_bytes()),
            failed: false,
            baseline_saved: false,
            baseline_exported: false,
            searched: false,
            frame_ticks: 0,
            last_frame: Instant::now(),
            max_frame_gap_ms: 0.,
        })
    }
    fn record(&mut self, app: &EditorApp, label: &str) {
        let info = app.view.info.as_ref();
        let reply = app.view.candidate_reply.as_ref();
        self.records.push(json!({"label":label,"revision":info.map(|i|&i.revision),"workspace_revision":info.map(|i|&i.workspace_revision),"dirty":info.map(|i|i.dirty),"project_dirty":info.map(|i|i.project_dirty),"undo":info.map(|i|i.undo_entries),"redo":info.map(|i|i.redo_entries),"component_count":app.view.board.as_ref().map(|b|b.components.len()),"registration":app.view.board.as_ref().and_then(|b|b.registration.as_ref()),"focused_side":app.components.focused.as_ref().map(|c|c.component.side),"world_position":app.components.focused.as_ref().and_then(|c|c.world_position),"world_rotation_deg":app.components.focused.as_ref().and_then(|c|c.world_rotation_deg),"candidate_page":reply.map(|r|&r.page),"selected_count":app.view.selected.ordered.len(),"camera_center":app.camera.center,"camera_scale":app.camera.scale,"display_unit":app.display_unit.suffix(),"pixels_per_point":app.reported_ppp,"geometry_sha256":app.view.snap_snapshot.as_ref().map(|s|sha256_hex(&serde_json::to_vec(&(&s.apertures,&s.block_definitions,&s.layers)).unwrap())),"error_code":app.view.error.as_ref().map(|e|&e.code),"ui_error":app.ui_error,"busy":app.busy}));
        std::fs::write(
            self.dir.join("native-progress.json"),
            serde_json::to_vec_pretty(&self.records).unwrap(),
        )
        .unwrap();
    }
    fn finish(&mut self, app: &EditorApp, ctx: &egui::Context) {
        let hash = std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| sha256_hex(&b));
        let report = json!({"schema_version":2,"stage":"S4-D2","status":if self.failed{"FAIL"}else{"PASS"},"commit":option_env!("RCAM_BUILD_COMMIT"),"binary_sha256":hash,"adapter":app.adapter,"evidence_kind":"controlled synthetic native EditorApp worker/ApplicationService/Metal; physical human input not claimed","records":self.records,"frame_ticks":self.frame_ticks,"max_frame_gap_ms":self.max_frame_gap_ms});
        std::fs::write(
            self.dir.join("native-observations.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        self.phase = 100;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        self.frame_ticks += 1;
        self.max_frame_gap_ms = self
            .max_frame_gap_ms
            .max(self.last_frame.elapsed().as_secs_f64() * 1000.);
        self.last_frame = Instant::now();
        ctx.request_repaint_after(Duration::from_millis(100));
        // Optional synthetic-only pause for observing the live native overlay.
        if self.phase == 7
            && self.dir.join("native-capture.request").exists()
            && !self.dir.join("native-capture.resume").exists()
        {
            return;
        }
        if self.phase >= 100 || app.busy || self.last.elapsed() < Duration::from_millis(700) {
            return;
        }
        if app.view.layers.iter().any(|l| {
            l.provenance.as_ref().is_none_or(|p| {
                p.imported_sha256
                    != sha256_hex(include_bytes!("../../../fixtures/synthetic/s4d2/board.gbr"))
            })
        }) || app
            .view
            .board
            .as_ref()
            .is_some_and(|b| b.provenance.sha256 != self.pnp_hash)
        {
            self.failed = true;
            self.record(app, "synthetic-fixture-gate-rejected");
            self.finish(app, ctx);
            return;
        }
        if app.view.error.is_some() && self.phase != 4 {
            self.failed = true;
            self.record(app, "unexpected-worker-error");
            self.finish(app, ctx);
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
                if !self.baseline_saved {
                    self.baseline_saved = true;
                    app.send(Action::SaveProject(
                        Some(self.dir.join("baseline.rcam")),
                        false,
                        None,
                    ));
                    self.last = Instant::now();
                    return;
                }
                self.record(app, "gerber-import");
                app.send(Action::PnpPreview(PreviewRequest {
                    context,
                    path: self.dir.join("pnp.csv"),
                    mapping: crate::components_ui::UiState::default().mapping,
                }));
            }
            2 => {
                if !self.baseline_exported {
                    self.baseline_exported = true;
                    app.send(Action::Save(
                        self.dir.join("before.gbr"),
                        app.view.layers[0].layer_id.clone(),
                        None,
                    ));
                    self.last = Instant::now();
                    return;
                }
                self.record(app, "pnp-100k-preview");
                let preview = app.view.pnp_preview.as_ref().unwrap();
                app.send(Action::PnpImport(
                    context,
                    ImportPnpParams {
                        path: self.dir.join("pnp.csv").to_string_lossy().into(),
                        mapping: crate::components_ui::UiState::default().mapping,
                        preview_sha256: preview.result.sha256.clone(),
                        allow_replace: false,
                    },
                ));
            }
            3 => {
                self.record(app, "unregistered-before");
                app.components.open = true;
                let b = app.view.board.as_ref().unwrap().clone();
                app.focus_component(component_info(&b.components[0], &b));
            }
            4 => {
                self.record(app, "unregistered-rejected");
                app.send(Action::BoardRegistration(
                    context,
                    RegistrationInput::Manual {
                        transform: CoordinateTransform2D {
                            reflect_x: false,
                            rotation_deg: 37.,
                            translation: MmPoint::new(0., 0.),
                        },
                    },
                ));
            }
            5 => {
                if !self.searched {
                    self.searched = true;
                    app.components.query = "C15".into();
                    self.record(app, "before-refdes-search");
                    app.send(Action::ComponentSearch(
                        context.clone(),
                        ComponentQuery {
                            revision: context.revision,
                            query: "C15".into(),
                            mode: RefdesMatch::Exact,
                            side: None,
                            footprint: None,
                            offset: 0,
                            limit: 500,
                        },
                    ));
                    self.last = Instant::now();
                    return;
                }
                self.record(app, "refdes-search-list-only");
                assert_eq!(app.view.component_indices.len(), 1);
                let b = app.view.board.as_ref().unwrap().clone();
                app.focus_component(component_info(
                    &b.components[app.view.component_indices[0]],
                    &b,
                ));
            }
            6 => {
                let bounds = app
                    .view
                    .candidate_reply
                    .as_ref()
                    .unwrap()
                    .page
                    .window
                    .bounds();
                app.camera.fit(Some(bounds), app.canvas_rect);
                app.fit = false;
                self.record(app, "registered-top-candidates");
                let r = app.view.candidate_reply.as_ref().unwrap();
                app.send(Action::CandidateSelect(r.request.clone(), false));
            }
            7 => {
                self.record(app, "replace-selection");
                app.send(Action::Move("1".into(), "0".into()));
            }
            8 => {
                self.record(app, "move-after-selection");
                app.send(Action::History(false));
            }
            9 => {
                self.record(app, "undo-move");
                let b = app.view.board.as_ref().unwrap().clone();
                app.focus_component(component_info(&b.components[1], &b));
            }
            10 => {
                self.record(app, "registered-bottom-candidates");
                let r = app.view.candidate_reply.as_ref().unwrap();
                app.send(Action::CandidateSelect(r.request.clone(), true));
            }
            11 => {
                self.record(app, "add-selection");
                app.display_unit = crate::tools::DisplayUnit::Micrometer;
                app.camera.scale *= 2.;
            }
            12 => {
                self.record(app, "units-zoom-overlay");
                app.send(Action::Save(
                    self.dir.join("after.gbr"),
                    app.view.layers[0].layer_id.clone(),
                    None,
                ));
            }
            13 => {
                self.record(app, "gerber-export");
                app.send(Action::History(false));
            }
            14 => {
                self.record(app, "registration-undo-cache-cleared");
                app.send(Action::History(true));
            }
            15 => {
                self.record(app, "registration-redo-cache-cleared");
                app.send(Action::BoardRegistration(
                    context,
                    RegistrationInput::Manual {
                        transform: CoordinateTransform2D {
                            reflect_x: true,
                            rotation_deg: 37.,
                            translation: MmPoint::new(0., 0.),
                        },
                    },
                ));
            }
            16 => {
                let b = app.view.board.as_ref().unwrap().clone();
                app.focus_component(component_info(&b.components[1], &b));
            }
            17 => {
                self.record(app, "reflected-bottom-candidates");
                app.send(Action::SaveProject(
                    Some(self.dir.join("candidates.rcam")),
                    false,
                    None,
                ));
            }
            18 => {
                self.record(app, "project-save");
                app.send(Action::OpenProject(self.dir.join("candidates.rcam"), false));
            }
            19 => {
                self.record(app, "project-open-no-candidate-state");
                let b = app.view.board.as_ref().unwrap().clone();
                app.focus_component(component_info(&b.components[0], &b));
            }
            20 => {
                self.record(app, "requery-after-open");
                if let Some(runtime) = rcam_diagnostics::global() {
                    runtime.export(&self.dir.join("diagnostics.zip")).unwrap();
                }
            }
            21 => {
                self.record(app, "diagnostics-export");
                self.finish(app, ctx);
                return;
            }
            _ => return,
        }
        self.phase += 1;
        self.last = Instant::now();
    }
}
