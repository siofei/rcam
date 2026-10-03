//! Opt-in release-only synthetic native input; no production automation surface.
use crate::{
    EditorApp,
    state::{Action, MirrorDirection, PivotInput},
};
use editor_core::{MmPoint, hash::sha256_hex};
use eframe::egui::{self, Pos2};
use egui_wgpu::wgpu;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
// Independent producers: raw-input hooks, UI updates, production actions and GPU paint.
// The ledger is written separately from the per-frame report, never derived from it.
static ACTIVE: AtomicBool = AtomicBool::new(false);
static TRACE: OnceLock<Mutex<Value>> = OnceLock::new();
fn trace() -> &'static Mutex<Value> {
    TRACE.get_or_init(|| {
        Mutex::new(json!({"input_ids":[],"update_ids":[],"paint_ids":[],"actions":[],"frame":0}))
    })
}
pub fn paint(id: u64) {
    if ACTIVE.load(Ordering::Relaxed) {
        trace().lock().unwrap()["paint_ids"]
            .as_array_mut()
            .unwrap()
            .push(json!(id));
    }
}
pub fn action_detail(action: &Action) -> Option<Value> {
    if !ACTIVE.load(Ordering::Relaxed) {
        return None;
    }
    let detail = match action {
        Action::ProbeDrag(p, tolerance) => {
            json!({"kind":"probe","world":[p.x_mm,p.y_mm],"tolerance":tolerance})
        }
        Action::CanvasSelect(c, mode) => {
            json!({"kind":"select","world":[c.world.x_mm,c.world.y_mm],"point":[c.point.x,c.point.y],"mode":format!("{mode:?}"),"camera":c.camera,"canvas":[c.rect.min.x,c.rect.min.y,c.rect.max.x,c.rect.max.y],"ppp":c.ppp,"navigation_epoch":c.navigation_epoch})
        }
        Action::DragMove(d) => json!({"kind":"move","delta":[d.delta.x_mm,d.delta.y_mm]}),
        Action::SelectRect(b, mode) => {
            json!({"kind":"box","bounds":[b.min_x_mm,b.min_y_mm,b.max_x_mm,b.max_y_mm],"mode":format!("{mode:?}")})
        }
        _ => json!({"kind":"other","name":crate::native_probe::action_text(action)}),
    };
    Some(detail)
}
pub fn accepted_action(detail: Option<Value>, sequence: u64) {
    let Some(detail) = detail else {
        return;
    };
    let mut t = trace().lock().unwrap();
    let frame = t["frame"].clone();
    t["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"frame_id":frame,"sequence":sequence,"detail":detail}));
}
fn modifiers(m: egui::Modifiers) -> Value {
    json!({"alt":m.alt,"ctrl":m.ctrl,"shift":m.shift,"mac_cmd":m.mac_cmd,"command":m.command})
}
fn events(events: &[egui::Event]) -> Vec<Value> {
    events.iter().filter_map(|e| Some(match e {
        egui::Event::Screenshot{..} => return None,
        egui::Event::PointerMoved(p) => json!({"kind":"move","position":[p.x,p.y]}),
        egui::Event::PointerButton{pos,button,pressed,modifiers:m} => json!({"kind":"button","position":[pos.x,pos.y],"button":format!("{button:?}"),"pressed":pressed,"modifiers":modifiers(*m)}),
        egui::Event::PointerGone => json!({"kind":"gone"}),
        egui::Event::Key{key,pressed,modifiers:m,..} => json!({"kind":"key","key":format!("{key:?}"),"pressed":pressed,"modifiers":modifiers(*m)}),
        egui::Event::WindowFocused(f) => json!({"kind":"focus","focused":f}),
        _ => json!({"kind":"other","debug":format!("{e:?}")}),
    })).collect()
}
pub fn directory() -> Option<PathBuf> {
    let p = std::fs::canonicalize(std::env::var_os("RCAM_I1_NATIVE_DIR")?).ok()?;
    (p.parent() == Some(Path::new("/private/tmp"))
        && p.file_name()?.to_str()?.starts_with("rcam-i1-native-"))
    .then_some(p)
}
pub struct Run {
    dir: PathBuf,
    request: Value,
    step: usize,
    phase: u8,
    since: Instant,
    started: Instant,
    device: wgpu::Device,
    pub painted: Arc<AtomicU64>,
    pub frame_id: u64,
    previous: Option<(u64, Value)>,
    complete: Option<Value>,
    frames: Vec<Value>,
    records: Vec<Value>,
    finished: bool,
    input_before: Value,
    diagnostic_injected: bool,
    held_frames: u64,
}
fn state(app: &EditorApp) -> Value {
    let cycle = app.view.click_cycle.as_ref().map(|c| json!({"index":c.index,"candidates":c.candidates,"document":c.document,"revision":c.revision,"workspace":c.workspace,"point":[c.context.point.x,c.context.point.y],"world":[c.context.world.x_mm,c.context.world.y_mm],"camera":c.context.camera,"canvas":[c.context.rect.min.x,c.context.rect.min.y,c.context.rect.max.x,c.context.rect.max.y],"ppp":c.context.ppp,"navigation_epoch":c.context.navigation_epoch}));
    json!({"info":app.view.info,"selected":app.view.selected.ordered,"click_cycle":cycle,"task_receipt":app.view.task_receipt,"navigation_epoch":app.click_navigation.evidence_epoch(),"error":app.view.error,"scene_serial":app.view.scene.as_ref().map(|s|s.serial),"layers":app.view.layers,"camera":[app.camera.center.x_mm,app.camera.center.y_mm,app.camera.scale],"canvas":[app.canvas_rect.min.x,app.canvas_rect.min.y,app.canvas_rect.max.x,app.canvas_rect.max.y],"ppp":app.reported_ppp,"busy":app.busy,"display_pending":app.display_pending})
}
impl Run {
    pub fn from_env(device: wgpu::Device) -> Option<Self> {
        let dir = directory()?;
        assert!(!cfg!(debug_assertions));
        let request: Value =
            serde_json::from_slice(&std::fs::read(dir.join("request.json")).ok()?).ok()?;
        assert!(request["steps"].as_array()?.len() <= 200);
        ACTIVE.store(true, Ordering::Relaxed);
        Some(Self {
            dir,
            request,
            step: 0,
            phase: 0,
            since: Instant::now(),
            started: Instant::now(),
            device,
            painted: Arc::new(AtomicU64::new(0)),
            frame_id: 0,
            previous: None,
            complete: None,
            frames: vec![],
            records: vec![],
            finished: false,
            input_before: Value::Null,
            diagnostic_injected: false,
            held_frames: 0,
        })
    }
    fn position(app: &EditorApp, value: &Value, key: &str) -> Pos2 {
        app.camera.screen(
            MmPoint::new(
                value[key][0].as_f64().unwrap(),
                value[key][1].as_f64().unwrap(),
            ),
            app.canvas_rect,
        )
    }
    fn pointer(
        raw: &mut egui::RawInput,
        p: Pos2,
        pressed: Option<bool>,
        modifiers: egui::Modifiers,
    ) {
        raw.modifiers = modifiers;
        raw.events.push(egui::Event::PointerMoved(p));
        if let Some(pressed) = pressed {
            raw.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            });
        }
    }
    fn enter(&mut self, phase: u8) {
        self.phase = phase;
        self.held_frames = 0;
        self.since = Instant::now();
    }
    pub fn input(&mut self, app: &mut EditorApp, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.finished {
            return;
        }
        if self.phase == 1 {
            self.held_frames += 1;
        }
        if self.step == 6 && self.phase == 1 && !self.diagnostic_injected {
            match self.request["diagnostic"].as_str() {
                Some("foreign_move") => {
                    let p =
                        Self::position(app, &self.request["steps"][6], "from") + egui::vec2(4., 0.);
                    raw.events.push(egui::Event::PointerMoved(p));
                    self.diagnostic_injected = true;
                }
                Some("pointer_gone") => {
                    raw.events.push(egui::Event::PointerGone);
                    self.diagnostic_injected = true;
                }
                _ => {}
            }
        }
        self.input_before = json!({"gesture":app.drag.as_ref().map(crate::drag::Gesture::evidence_state),"focused":raw.focused,"events":events(&raw.events),"modifiers":modifiers(raw.modifiers),"sequence":app.sequence,"grip":app.grip.is_some(),"state":state(app)});
        for event in &raw.events {
            if let egui::Event::Screenshot {
                image, user_data, ..
            } = event
                && let Some(label) = user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<String>())
            {
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                std::fs::write(self.dir.join(format!("{label}.ppm")), bytes).unwrap();
            }
        }
        self.complete = None;
        if let Some((id, mut record)) = self.previous.take() {
            let painted = self.painted.load(Ordering::Acquire) == id;
            record["painted"] = json!(painted);
            record["gpu_completed"] = json!(
                painted
                    && self
                        .device
                        .poll(wgpu::PollType::Wait {
                            submission_index: None,
                            timeout: Some(Duration::from_secs(5))
                        })
                        .is_ok()
            );
            record["completed_ns"] = json!(self.started.elapsed().as_nanos() as u64);
            if record["gpu_completed"] == true {
                self.complete = Some(record.clone());
            }
            self.frames.push(record);
        }
        self.frame_id += 1;
        {
            let mut t = trace().lock().unwrap();
            t["frame"] = json!(self.frame_id);
            t["input_ids"]
                .as_array_mut()
                .unwrap()
                .push(json!(self.frame_id));
        }
        raw.focused = true;
        if self.step >= self.request["steps"].as_array().unwrap().len() {
            return;
        }
        let step = self.request["steps"][self.step].clone();
        let kind = step["kind"].as_str().unwrap();
        let modifiers = egui::Modifiers {
            ctrl: step["ctrl"] == true,
            shift: step["shift"] == true,
            ..Default::default()
        };
        if self.phase == 0
            && self.started.elapsed() > Duration::from_secs(5)
            && !app.busy
            && !app.display_pending
        {
            app.ui_error = None;
            app.recovery_candidate = None;
            if self.request["stress"] == true && self.step == 112 {
                // Reproduce the observed same-frame idle-recovery/press race.
                let now = Instant::now();
                let info = app.view.info.as_ref().unwrap();
                app.last_dirty_identity = format!(
                    "{}:{}:{}",
                    info.project_id, info.revision, info.workspace_revision
                );
                app.last_recovered_identity.clear();
                app.dirty_since = now - Duration::from_secs(31);
                app.last_recovery_at = now - Duration::from_secs(61);
                self.input_before["recovery_collision_due"] = json!(true);
            }
            match kind {
                "new" => app.send(Action::NewWorkspace),
                "import" => app.send(Action::ImportGerbers(
                    ["lower.gbr", "upper.gbr"]
                        .map(|n| {
                            Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("../../fixtures/synthetic/s5i1")
                                .join(n)
                        })
                        .to_vec(),
                )),
                "reimport_exports" => app.send(Action::ImportGerbers(
                    ["lower.gbr", "upper.gbr"]
                        .map(|n| self.dir.join(n))
                        .to_vec(),
                )),
                "camera" => {
                    app.camera = crate::camera::Camera {
                        center: MmPoint::new(3., 3.),
                        scale: 35.,
                    };
                    app.fit = false;
                    app.grid.snap_enabled = false;
                    app.object_snap.enabled = false;
                }
                "click" | "drag" => {
                    Self::pointer(
                        raw,
                        Self::position(app, &step, "from"),
                        Some(true),
                        modifiers,
                    );
                    self.enter(1);
                    return;
                }
                "zoom" => {
                    raw.events.push(egui::Event::PointerMoved(Self::position(
                        app, &step, "from",
                    )));
                    raw.events
                        .push(egui::Event::Zoom(step["factor"].as_f64().unwrap() as f32));
                }
                "pan" => {
                    raw.events
                        .push(egui::Event::PointerMoved(app.canvas_rect.center()));
                    raw.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(30., 0.),
                        modifiers: Default::default(),
                    });
                }
                "move" => app.send(Action::Move(step["dx"].to_string(), step["dy"].to_string())),
                "recovery" => app.send(Action::RecoveryWrite(
                    self.dir.join(format!("idle-recovery-step-{}", self.step)),
                )),
                "rotate" => app.send(Action::Rotate("90".into(), PivotInput::WorldOrigin)),
                "mirror" => app.send(Action::Mirror(MirrorDirection::Vertical)),
                "duplicate" => app.send(Action::Duplicate),
                "delete" => app.send(Action::Delete),
                "undo" | "redo" => {
                    let modifiers = egui::Modifiers {
                        command: true,
                        mac_cmd: true,
                        shift: kind == "redo",
                        ..Default::default()
                    };
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::Key {
                            key: egui::Key::Z,
                            physical_key: Some(egui::Key::Z),
                            pressed,
                            repeat: false,
                            modifiers,
                        });
                    }
                }
                "locked" | "visible" => {
                    let layer = app
                        .view
                        .layers
                        .iter()
                        .find(|l| l.display_name.contains(step["layer"].as_str().unwrap()))
                        .unwrap();
                    let info = app.view.info.as_ref().unwrap();
                    app.send(Action::Layer(editor_service::LayerUpdateParams {
                        layer_id: layer.layer_id.clone(),
                        expected_workspace_revision: info.workspace_revision.clone(),
                        locked: (kind == "locked").then_some(step["value"] == true),
                        visible: (kind == "visible").then_some(step["value"] == true),
                        ..Default::default()
                    }));
                }
                "save" => app.send(Action::SaveProject(
                    Some(self.dir.join("workflow.rcam")),
                    false,
                    None,
                )),
                "reopen" => app.send(Action::OpenProject(self.dir.join("workflow.rcam"), true)),
                "export" => {
                    let l = app
                        .view
                        .layers
                        .iter()
                        .find(|l| l.display_name.contains(step["layer"].as_str().unwrap()))
                        .unwrap();
                    app.send(Action::Save(
                        self.dir
                            .join(format!("{}.gbr", step["layer"].as_str().unwrap())),
                        l.layer_id.clone(),
                        None,
                    ));
                }
                _ => panic!("unknown I1 test step {kind}"),
            }
            self.enter(4);
        } else if self.phase == 1
            && self.held_frames > step["hold_frames"].as_u64().unwrap_or(0)
            && ((!app.busy
                && (app.drag.as_ref().is_some_and(|g| g.confirmed) || app.grip.is_some()))
                || (self.step == 6 && self.request["diagnostic"] == "early_release")
                || (self.diagnostic_injected && self.held_frames >= 2))
        {
            if kind == "drag" {
                Self::pointer(raw, Self::position(app, &step, "to"), None, modifiers);
                self.enter(2);
            } else {
                Self::pointer(
                    raw,
                    Self::position(app, &step, "from"),
                    Some(false),
                    modifiers,
                );
                self.enter(4);
            }
        } else if self.phase == 2 && self.since.elapsed() > Duration::from_millis(250) {
            if step["escape"] == true {
                raw.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: Some(egui::Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                });
            }
            Self::pointer(
                raw,
                Self::position(app, &step, "to"),
                Some(false),
                modifiers,
            );
            self.enter(4);
        }
        ctx.request_repaint();
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        if self.finished {
            return;
        }
        {
            trace().lock().unwrap()["update_ids"]
                .as_array_mut()
                .unwrap()
                .push(json!(self.frame_id));
        }
        let current = state(app);
        self.previous = Some((
            self.frame_id,
            json!({"frame_id":self.frame_id,"step":self.step,"phase":self.phase,"state":current,"input_ns":self.started.elapsed().as_nanos() as u64,"pass_index":ctx.current_pass_index(),"input_before":self.input_before,"grip_after":app.grip.is_some(),"sequence":app.sequence,"gesture_after":app.drag.as_ref().map(crate::drag::Gesture::evidence_state),"input":ctx.input(|i|json!({"events":events(&i.events),"position":i.pointer.interact_pos().map(|p|[p.x,p.y]),"down":i.pointer.primary_down(),"released":i.pointer.primary_released(),"focused":i.focused,"modifiers":modifiers(i.modifiers)}))}),
        ));
        if self.started.elapsed() > Duration::from_secs(180) {
            self.finish(app, ctx, Some("native timeout"));
            return;
        }
        if self.step == self.request["steps"].as_array().unwrap().len() {
            ctx.request_repaint_after(Duration::from_millis(100));
            if self.since.elapsed() > Duration::from_millis(600)
                && self.started.elapsed()
                    > Duration::from_secs(if self.request["stress"] == true {
                        96
                    } else {
                        76
                    })
            {
                self.finish(app, ctx, None);
            }
            return;
        }
        let no_document = app.view.info.is_none() || app.view.layers.is_empty();
        let completed = self
            .complete
            .as_ref()
            .filter(|frame| frame["state"] == current);
        if self.phase == 4
            && !app.busy
            && !app.display_pending
            && app.drag.is_none()
            && app.grip.is_none()
            && self.since.elapsed() > Duration::from_millis(400)
            && (no_document || completed.is_some())
        {
            let label = format!("step-{:02}", self.step);
            let snapshot = app
                .view
                .snap_snapshot
                .as_ref()
                .map(|s| serde_json::to_value(s.as_ref()).unwrap());
            std::fs::write(self.dir.join(format!("{label}.json")),serde_json::to_vec_pretty(&json!({"input":self.request["steps"][self.step],"state":current,"snapshot":snapshot,"completed_frame":completed})).unwrap()).unwrap();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                label.clone(),
            )));
            self.records.push(json!({"step":self.step,"path":format!("{label}.json"),"elapsed_ms":self.since.elapsed().as_secs_f64()*1000.}));
            self.step += 1;
            self.enter(0);
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
    fn finish(&mut self, app: &mut EditorApp, ctx: &egui::Context, error: Option<&str>) {
        let binary = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let mut ledger = trace().lock().unwrap().clone();
        ledger["run_id"] = self.request["run_id"].clone();
        ledger["capture_start"] = json!(1);
        ledger["capture_end"] = json!(self.frame_id - 1);
        ledger["terminal_pending_frame"] = json!(self.frame_id);
        ledger["paint_count"] = json!(ledger["paint_ids"].as_array().unwrap().len());
        std::fs::write(
            self.dir.join("capture-ledger.json"),
            serde_json::to_vec_pretty(&ledger).unwrap(),
        )
        .unwrap();
        std::fs::write(self.dir.join("observations.json"),serde_json::to_vec_pretty(&json!({"schema_version":2,"stage":"S5-I1","request":self.request,"records":self.records,"frames":self.frames,"error":error,"adapter":app.adapter,"profile":"release","commit":option_env!("RCAM_BUILD_COMMIT"),"build_source":option_env!("RCAM_BUILD_SOURCE"),"binary_sha256":sha256_hex(&binary),"measurement_scope":"synthetic egui input, real worker and production Metal completion fence; functional evidence, no physical latency or PMIX claim"})).unwrap()).unwrap();
        self.finished = true;
        app.allow_quit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
