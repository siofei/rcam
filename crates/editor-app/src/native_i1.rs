//! Opt-in release-only synthetic native input; no production automation surface.
use crate::{
    EditorApp,
    state::{Action, MirrorDirection, PivotInput},
};
use editor_core::command::CommandDispatcher;
use editor_core::{MmPoint, hash::sha256_hex};
use eframe::egui::{self, Pos2};
use egui_wgpu::wgpu;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
static CLOCK: OnceLock<Instant> = OnceLock::new();
fn now_ns() -> u64 {
    CLOCK.get().unwrap().elapsed().as_nanos() as u64
}
fn trace() -> &'static Mutex<Value> {
    TRACE.get_or_init(|| {
        Mutex::new(json!({"input_ids":[],"update_ids":[],"paint_ids":[],"paint_records":[],"delivered_inputs":[],"surface_callbacks":[],"actions":[],"frame":0}))
    })
}
pub fn widget(name: &str, response: &egui::Response) {
    if ACTIVE.load(Ordering::Relaxed) {
        let mut t = trace().lock().unwrap();
        if t.get("widgets").is_none() {
            t["widgets"] = json!({});
        }
        t["widgets"][name] = json!({"rect":[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y],"enabled":response.enabled(),"frame":t["frame"]});
    }
}
pub fn status(fields: &crate::status_bar::Fields, slots: [Option<egui::Rect>; 4]) {
    if ACTIVE.load(Ordering::Relaxed) {
        trace().lock().unwrap()["status"] = json!({"selection":fields.selection,"area":fields.area,"perimeter":fields.perimeter,"state":fields.state,"tooltip":fields.tooltip,"slots":slots.map(|s|s.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]))});
    }
}
pub fn cursor(shown: bool) {
    if ACTIVE.load(Ordering::Relaxed) {
        trace().lock().unwrap()["cursor_shown"] = json!(shown);
    }
}
pub fn paint(id: u64) {
    if ACTIVE.load(Ordering::Relaxed) {
        trace().lock().unwrap()["paint_ids"]
            .as_array_mut()
            .unwrap()
            .push(json!(id));
        trace().lock().unwrap()["paint_records"]
            .as_array_mut()
            .unwrap()
            .push(json!({"frame_id":id,"paint_ns":now_ns()}));
    }
}
pub fn action_detail(action: &Action) -> Option<Value> {
    if !ACTIVE.load(Ordering::Relaxed) {
        return None;
    }
    let detail = match action {
        Action::GripEdit(g) => json!({"kind":"grip_edit","target":g.target}),
        Action::BlockEdit(_) => json!({"kind":"block_edit"}),
        Action::Move(..) | Action::Rotate(..) | Action::Mirror(..) => {
            json!({"kind":"legacy_transform"})
        }
        Action::PointApply(r) => {
            json!({"kind":"point_apply","operation":r.operation,"context":format!("{:?}",r.context)})
        }
        Action::PointPreview(r) => json!({"kind":"point_preview","operation":r.operation}),
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
        egui::Event::Key{key,pressed,repeat,modifiers:m,..} => json!({"kind":"key","key":format!("{key:?}"),"pressed":pressed,"repeat":repeat,"modifiers":modifiers(*m)}),
        egui::Event::WindowFocused(f) => json!({"kind":"focus","focused":f}),
        egui::Event::Text(t) => json!({"kind":"text","value":t}),
        egui::Event::Ime(t) => json!({"kind":"ime","debug":format!("{t:?}")}),
        _ => json!({"kind":"other","debug":format!("{e:?}")}),
    })).collect()
}
pub fn directory() -> Option<PathBuf> {
    let p = std::fs::canonicalize(
        std::env::var_os("RCAM_I2_C_NATIVE_DIR")
            .or_else(|| std::env::var_os("RCAM_I2_B_NATIVE_DIR"))
            .or_else(|| std::env::var_os("RCAM_I1_NATIVE_DIR"))?,
    )
    .ok()?;
    let explicit_root = std::env::var_os("RCAM_I2_C_NATIVE_ROOT")
        .or_else(|| std::env::var_os("RCAM_I2_B_NATIVE_ROOT"))
        .and_then(|r| std::fs::canonicalize(r).ok());
    let allowed_root = p.parent() == Some(Path::new("/private/tmp"))
        || ((std::env::var_os("RCAM_I2_C_NATIVE_DIR").is_some()
            || std::env::var_os("RCAM_I2_B_NATIVE_DIR").is_some())
            && explicit_root
                .as_deref()
                .is_some_and(|r| p.parent() == Some(r)));
    (allowed_root
        && (p.file_name()?.to_str()?.starts_with("rcam-i2-c-native-")
            || p.file_name()?.to_str()?.starts_with("rcam-i1-native-")
            || p.file_name()?.to_str()?.starts_with("rcam-i2-b-native-")))
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
    delivered_input: Value,
    feedback_pending: Option<Value>,
    feedback: Vec<Value>,
}
fn state(app: &EditorApp) -> Value {
    let (status, cursor_shown) = {
        let t = trace().lock().unwrap();
        (t["status"].clone(), t["cursor_shown"].clone())
    };
    let cycle = app.view.click_cycle.as_ref().map(|c| json!({"index":c.index,"candidates":c.candidates,"document":c.document,"revision":c.revision,"workspace":c.workspace,"point":[c.context.point.x,c.context.point.y],"world":[c.context.world.x_mm,c.context.world.y_mm],"camera":c.context.camera,"canvas":[c.context.rect.min.x,c.context.rect.min.y,c.context.rect.max.x,c.context.rect.max.y],"ppp":c.context.ppp,"navigation_epoch":c.context.navigation_epoch}));
    let point=app.point_transform.as_ref().map(|s|json!({"mode":format!("{:?}",s.mode),"angle":s.angle,"base":s.base.resolve(app.display_unit).ok().map(|p|json!({"world":[p.world_mm.x_mm,p.world_mm.y_mm],"source":format!("{:?}",p.source)})),"target":s.target.resolve(app.display_unit).ok().map(|p|json!({"world":[p.world_mm.x_mm,p.world_mm.y_mm],"source":format!("{:?}",p.source)})),"operation":s.operation(&app.view,app.display_unit).ok(),"preview":app.view.point_preview.as_ref().map(|p|json!({"operation":p.request.operation,"bounds":p.bounds,"simplified":p.simplified,"path_count":p.paths.len()}))}));
    json!({"mouse_grip":app.grip.as_ref().map(|g|json!({"id":g.id,"moved":g.moved,"target":g.target,"original":g.object,"preview":g.preview.as_ref().ok().map(|p|json!({"geometry":p.geometry,"aperture_shape":p.aperture_shape}))})),"interaction":app.prefs.interaction,"status":status,"cursor_shown":cursor_shown,"info":app.view.info,"selected":app.view.selected.ordered,"point_transform":point,"point_pick":app.point_pick.as_ref().map(|p|format!("{:?}",p.field)),"point_adapter":app.point_adapter.as_ref().map(|s|json!({"target":format!("{:?}",s.target),"world":s.draft.resolve(app.display_unit).ok().map(|p|[p.world_mm.x_mm,p.world_mm.y_mm]),"block_translation":s.block_translation,"grip_target":s.grip_preview.as_ref().map(|g|g.target),"grip_preview":s.grip_preview.as_ref().and_then(|g|g.preview.as_ref().ok()).map(|p|json!({"geometry":p.geometry,"aperture_shape":p.aperture_shape}))})),"modal":app.modal.map(|m|format!("{m:?}")),"tool":match app.tool {crate::tools::ActiveTool::Select=>"Select",crate::tools::ActiveTool::Measure=>"Measure",crate::tools::ActiveTool::Text=>"Text",crate::tools::ActiveTool::Block=>"Block"},"measure":{"a":app.measure.a,"b":app.measure.b,"values":app.measure.values(),"completed":app.measure.completed.len()},"text_reference":{"x":app.text.rx,"y":app.text.ry,"enabled":app.text.has_reference},"array_pitch":[app.array.pitch_x,app.array.pitch_y],"array_base":app.array_point_base,"block_reference":app.block_point_reference,"block_origin":[app.block.x,app.block.y],"board_world":app.components.world_points,"snap_marker":app.object_snap_runtime.current.as_ref().map(|r|json!({"world":[r.point.x_mm,r.point.y_mm],"kind":format!("{:?}",r.kind)})),"click_cycle":cycle,"task_receipt":app.view.task_receipt,"navigation_epoch":app.click_navigation.evidence_epoch(),"error":app.view.error,"ui_error":app.ui_error,"scene_serial":app.view.scene.as_ref().map(|s|s.serial),"layers":app.view.layers,"camera":[app.camera.center.x_mm,app.camera.center.y_mm,app.camera.scale],"canvas":[app.canvas_rect.min.x,app.canvas_rect.min.y,app.canvas_rect.max.x,app.canvas_rect.max.y],"ppp":app.reported_ppp,"busy":app.busy,"display_pending":app.display_pending})
}
pub(crate) fn block_place_definition(view: &crate::state::View) -> Result<String, &'static str> {
    view.block_definitions
        .first()
        .map(|definition| definition.id.0.clone())
        .ok_or(
            "native setup precondition failed: b_block_place requires a committed Block definition",
        )
}
impl Run {
    pub fn record_delivery(&mut self, app: &EditorApp, raw: &egui::RawInput) {
        self.delivered_input = json!({"frame_id":self.frame_id,"step":self.step,"phase":self.phase,"t0_ns":now_ns(),"focused":raw.focused,"events":events(&raw.events),"modifiers":modifiers(raw.modifiers),"camera":[app.camera.center.x_mm,app.camera.center.y_mm,app.camera.scale],"canvas":[app.canvas_rect.min.x,app.canvas_rect.min.y,app.canvas_rect.max.x,app.canvas_rect.max.y],"ppp":app.reported_ppp});
        let step = &self.request["steps"][self.step];
        if let Some(name) = step["name"].as_str() {
            self.delivered_input["widget"] = trace().lock().unwrap()["widgets"][name].clone();
        }
        trace().lock().unwrap()["delivered_inputs"]
            .as_array_mut()
            .unwrap()
            .push(self.delivered_input.clone());
    }
    fn capture_feedback(&mut self, image: &egui::ColorImage, label: &str) {
        let t1 = now_ns();
        let mut sample = self
            .feedback_pending
            .take()
            .expect("unsolicited feedback image");
        assert_eq!(sample["label"], label);
        let x = sample["pixel"][0].as_u64().unwrap() as usize;
        let y = sample["pixel"][1].as_u64().unwrap() as usize;
        assert!(x >= 12 && y >= 12 && x + 12 < image.size[0] && y + 12 < image.size[1]);
        let mut crop = b"P6\n25 25\n255\n".to_vec();
        for row in y - 12..=y + 12 {
            for col in x - 12..=x + 12 {
                let p = image.pixels[row * image.size[0] + col];
                crop.extend([p.r(), p.g(), p.b()]);
            }
        }
        let path = format!("{label}.ppm");
        std::fs::write(self.dir.join(&path), &crop).unwrap();
        sample["t1_ns"] = json!(t1);
        sample["callback_frame_id"] = json!(self.frame_id + 1);
        sample["crop"] = json!(path);
        sample["crop_origin_px"] = json!([x - 12, y - 12]);
        sample["crop_sha256"] = json!(sha256_hex(&crop));
        sample["surface_size_px"] = json!(image.size);
        sample["latency_ms"] = json!((t1 - sample["t0_ns"].as_u64().unwrap()) as f64 / 1e6);
        trace().lock().unwrap()["surface_callbacks"].as_array_mut().unwrap().push(json!({"label":label,"callback_frame_id":self.frame_id+1,"t1_ns":t1,"crop_sha256":sample["crop_sha256"]}));
        self.feedback.push(sample);
    }
    pub fn from_env(device: wgpu::Device) -> Option<Self> {
        let dir = directory()?;
        assert!(!cfg!(debug_assertions));
        let request: Value =
            serde_json::from_slice(&std::fs::read(dir.join("request.json")).ok()?).ok()?;
        assert!(
            request["steps"].as_array()?.len()
                <= if matches!(request["stage"].as_str(), Some("S5-I2-B" | "S5-I2-C")) {
                    400
                } else {
                    200
                }
        );
        if matches!(request["stage"].as_str(), Some("S5-I2-B" | "S5-I2-C")) {
            assert_eq!(
                request["source_manifest_sha256"].as_str().unwrap(),
                sha256_hex(include_bytes!("../../../MANIFEST.sha256"))
            );
            for name in ["layer_a.gbr", "layer_b.gbr", "layer_c.gbr"] {
                let bytes = std::fs::read(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../fixtures/synthetic/s5i2b")
                        .join(name),
                )
                .unwrap();
                assert_eq!(
                    request["fixtures"][name].as_str().unwrap(),
                    sha256_hex(&bytes)
                );
            }
        }
        let started = *CLOCK.get_or_init(Instant::now);
        if request["role"] == "feedback" {
            let bytes = std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr"),
            )
            .unwrap();
            assert_eq!(
                request["performance_fixture_sha256"].as_str().unwrap(),
                sha256_hex(&bytes)
            );
        }
        ACTIVE.store(true, Ordering::Relaxed);
        Some(Self {
            dir,
            request,
            step: 0,
            phase: 0,
            since: Instant::now(),
            started,
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
            delivered_input: Value::Null,
            feedback_pending: None,
            feedback: vec![],
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
                if label.starts_with("feedback-") {
                    self.capture_feedback(image, label);
                    continue;
                }
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                if matches!(self.request["stage"].as_str(), Some("S5-I2-B" | "S5-I2-C")) {
                    // Preserve every P6 byte without duplicating multi-GB
                    // uncompressed windows. Feedback ROI timing bypasses this.
                    let mut child = Command::new("/usr/bin/gzip")
                        .args(["-n", "-c"])
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .spawn()
                        .unwrap();
                    let mut stdin = child.stdin.take().unwrap();
                    let output = std::thread::scope(|scope| {
                        let writer = scope.spawn(move || stdin.write_all(&bytes));
                        let output = child.wait_with_output().unwrap();
                        writer.join().unwrap().unwrap();
                        output
                    });
                    assert!(
                        output.status.success(),
                        "lossless native surface compression failed"
                    );
                    std::fs::write(self.dir.join(format!("{label}.ppm.gz")), output.stdout)
                        .unwrap();
                } else {
                    std::fs::write(self.dir.join(format!("{label}.ppm")), bytes).unwrap();
                }
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
                "c_new" => app.send(Action::DiscardNewWorkspace),
                "c_preferences" => {
                    let preferences = serde_json::from_value(step["value"].clone()).unwrap();
                    app.set_interaction_preferences(preferences);
                }
                "c_reload_preferences" => {
                    let path = crate::preferences::AppPreferences::path().unwrap();
                    app.prefs = crate::preferences::AppPreferences::load(&path);
                    app.fence_mouse_preferences();
                }
                "c_hover" => {
                    Self::pointer(raw, Self::position(app, &step, "from"), None, modifiers);
                }
                "c_hover_menu" => {
                    let t = trace().lock().unwrap();
                    let w = &t["widgets"][step["name"].as_str().unwrap()];
                    let r = &w["rect"];
                    let p = Pos2::new(
                        ((r[0].as_f64().unwrap() + r[2].as_f64().unwrap()) / 2.) as f32,
                        ((r[1].as_f64().unwrap() + r[3].as_f64().unwrap()) / 2.) as f32,
                    );
                    Self::pointer(raw, p, None, modifiers);
                }
                "c_resize" => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        step["size"][0].as_f64().unwrap() as f32,
                        step["size"][1].as_f64().unwrap() as f32,
                    )));
                }
                "c_complex_import" => app.send(Action::ImportGerbers(vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../fixtures/synthetic/s5i2c/material.gbr"),
                ])),
                "c_complex_select" => app.send(Action::SelectRect(
                    editor_core::BoundsMm {
                        min_x_mm: -10.,
                        min_y_mm: -10.,
                        max_x_mm: 30.,
                        max_y_mm: 30.,
                    },
                    editor_core::hit_test::SelectRectMode::Window,
                )),
                "b_tool_select" => {
                    app.dispatch(editor_core::command::ids::TOOL_SELECT);
                }
                "b_active_last" => {
                    app.send(Action::SetActiveLayer(Some(
                        app.view.layers.last().unwrap().layer_id.clone(),
                    )));
                }
                "b_select_one" => {
                    app.send(Action::Select(
                        MmPoint::new(20., 4.),
                        0.01,
                        crate::selection::SelectionMode::Replace,
                    ));
                }
                "b_block_create" => {
                    app.block_command(editor_core::command::ids::BLOCK_CREATE);
                }
                "b_block_place" => {
                    let definition = match block_place_definition(&app.view) {
                        Ok(definition) => definition,
                        Err(error) => {
                            self.finish(app, ctx, Some(error));
                            return;
                        }
                    };
                    app.block.definition = Some(definition);
                    app.block_command(editor_core::command::ids::BLOCK_PLACE);
                }
                "b_import" => app.send(Action::ImportGerbers(
                    ["layer_a.gbr", "layer_b.gbr", "layer_c.gbr"]
                        .map(|n| {
                            Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("../../fixtures/synthetic/s5i2b")
                                .join(n)
                        })
                        .to_vec(),
                )),
                "b_select_all" => app.send(Action::SelectRect(
                    editor_core::BoundsMm {
                        min_x_mm: -2.,
                        min_y_mm: -2.,
                        max_x_mm: 22.,
                        max_y_mm: 6.,
                    },
                    editor_core::hit_test::SelectRectMode::Window,
                )),
                "b_modal" => {
                    app.open_modal(match step["tool"].as_str().unwrap() {
                        "move" => crate::modal::ActiveModal::Move,
                        "rotate" => crate::modal::ActiveModal::Rotate,
                        "mirror" | "vertical" => crate::modal::ActiveModal::Mirror,
                        "copy" => crate::modal::ActiveModal::Move,
                        _ => panic!("invalid point tool"),
                    });
                    if let Some(session) = &mut app.point_transform {
                        if step["tool"] == "copy" {
                            session.mode = crate::point_transform::Mode::Copy;
                        }
                        if step["tool"] == "vertical" {
                            session.mode = crate::point_transform::Mode::VerticalMirror;
                        }
                    }
                }
                "b_adapter" => app.open_point_adapter(
                    match step["tool"].as_str().unwrap() {
                        "measure" => crate::point_adapter::Adapter::Measure,
                        "text" => crate::point_adapter::Adapter::TextReference,
                        "array_base" => crate::point_adapter::Adapter::ArrayBase,
                        "array_target" => crate::point_adapter::Adapter::ArrayTarget,
                        "board" => crate::point_adapter::Adapter::BoardWorld(0),
                        "grip" => crate::point_adapter::Adapter::Grip(
                            editor_core::grip::GripFeatureId::Right,
                        ),
                        _ => panic!("invalid point adapter"),
                    },
                    if step["tool"] == "grip" {
                        MmPoint::new(20.5, 4.)
                    } else {
                        MmPoint::new(0., 0.)
                    },
                ),
                "widget" | "cancel_conflict" if kind == "widget" || step["via"] == "button" => {
                    let t = trace().lock().unwrap();
                    let w = &t["widgets"][step["name"].as_str().unwrap()];
                    assert!(
                        w["frame"]
                            .as_u64()
                            .is_some_and(|f| f + 1 >= self.frame_id && f <= self.frame_id),
                        "requested widget is stale: {step}"
                    );
                    assert_eq!(w["enabled"], true, "requested widget disabled: {step}");
                    let p = Pos2::new(
                        ((w["rect"][0].as_f64().unwrap() + w["rect"][2].as_f64().unwrap()) / 2.)
                            as f32,
                        ((w["rect"][1].as_f64().unwrap() + w["rect"][3].as_f64().unwrap()) / 2.)
                            as f32,
                    );
                    Self::pointer(raw, p, Some(true), modifiers);
                    self.enter(1);
                    return;
                }
                "cancel_conflict" => {
                    for key in [egui::Key::Escape, egui::Key::Enter] {
                        for pressed in [true, false] {
                            raw.events.push(egui::Event::Key {
                                key,
                                physical_key: Some(key),
                                pressed,
                                repeat: step["repeat"] == true,
                                modifiers: Default::default(),
                            });
                        }
                    }
                }
                "b_feedback_import" => app.send(Action::ImportGerbers(vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr"),
                ])),
                "b_feedback_select" => {
                    assert_eq!(
                        app.view
                            .layers
                            .iter()
                            .map(|l| l.object_count)
                            .sum::<usize>(),
                        100000
                    );
                    app.send(Action::Select(
                        MmPoint::new(10., 1.),
                        0.01,
                        crate::selection::SelectionMode::Replace,
                    ));
                }
                "feedback_move" => {
                    Self::pointer(raw, Self::position(app, &step, "from"), None, modifiers);
                }
                "text" => {
                    let primary = egui::Modifiers {
                        command: true,
                        mac_cmd: true,
                        ..Default::default()
                    };
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::Key {
                            key: egui::Key::A,
                            physical_key: Some(egui::Key::A),
                            pressed,
                            repeat: false,
                            modifiers: primary,
                        });
                    }
                    raw.events.push(egui::Event::Text(
                        step["value"].as_str().unwrap().to_owned(),
                    ));
                }
                "ime_end" => raw.events.push(egui::Event::Ime(egui::ImeEvent::Disabled)),
                "escape" => {
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: Some(egui::Key::Escape),
                            pressed,
                            repeat: false,
                            modifiers: Default::default(),
                        });
                    }
                }
                "b_feedback_camera" => {
                    app.display_unit = editor_core::units::DisplayUnit::Millimeter;
                    app.camera = crate::camera::Camera {
                        center: MmPoint::new(10., 1.),
                        scale: 40.,
                    };
                    app.fit = false;
                    app.grid.snap_enabled = false;
                }
                "b_camera" => {
                    app.display_unit = editor_core::units::DisplayUnit::Millimeter;
                    app.camera = crate::camera::Camera {
                        center: MmPoint::new(10., 1.),
                        scale: 30.,
                    };
                    app.fit = false;
                    app.grid.snap_enabled = false;
                }
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
                "click" | "drag" | "point_click" => {
                    if kind == "point_click" {
                        assert_eq!(
                            app.canvas_rect.contains(Self::position(app, &step, "from")),
                            step["outside"] != true,
                            "point input fixture screen/world mismatch: {step}"
                        );
                    }
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
            || self.phase == 1
                && matches!(kind, "widget" | "point_click" | "cancel_conflict")
                && self.held_frames >= 1
        {
            if kind == "drag" {
                Self::pointer(raw, Self::position(app, &step, "to"), None, modifiers);
                self.enter(2);
            } else {
                let pos = if kind == "widget" || kind == "cancel_conflict" {
                    ctx.input(|i| i.pointer.interact_pos()).unwrap()
                } else {
                    Self::position(app, &step, "from")
                };
                Self::pointer(raw, pos, Some(false), modifiers);
                if kind == "cancel_conflict" {
                    match step["cancel"].as_str().unwrap() {
                        "escape" => {
                            for key in [egui::Key::Escape, egui::Key::Enter] {
                                raw.events.push(egui::Event::Key {
                                    key,
                                    physical_key: Some(key),
                                    pressed: true,
                                    repeat: false,
                                    modifiers: Default::default(),
                                });
                            }
                        }
                        "blur" => {
                            raw.focused = false;
                            raw.events.push(egui::Event::WindowFocused(false));
                        }
                        "gone" => raw.events.push(egui::Event::PointerGone),
                        "ime" => {
                            raw.events
                                .push(egui::Event::Ime(egui::ImeEvent::Preedit("输入中".into())));
                            raw.events.push(egui::Event::Key {
                                key: egui::Key::Enter,
                                physical_key: Some(egui::Key::Enter),
                                pressed: true,
                                repeat: false,
                                modifiers: Default::default(),
                            });
                        }
                        _ => panic!("unknown cancellation conflict"),
                    }
                }
                self.enter(4);
            }
        } else if self.phase == 2 && self.since.elapsed() > Duration::from_millis(250) {
            if let Some(value) = step.get("switch_off") {
                let mut p = app.prefs.interaction;
                if value == "drag" {
                    p.drag_move = false;
                } else {
                    p.grip_edit = false;
                }
                app.set_interaction_preferences(p);
            }

            match step["cancel"].as_str() {
                Some("escape" | "repeat") => {
                    for key in [egui::Key::Escape, egui::Key::Enter] {
                        raw.events.push(egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: true,
                            repeat: step["cancel"] == "repeat",
                            modifiers: Default::default(),
                        });
                    }
                }
                Some("blur") => {
                    raw.focused = false;
                    raw.events.push(egui::Event::WindowFocused(false));
                }
                Some("gone") => raw.events.push(egui::Event::PointerGone),
                Some("ime") => raw
                    .events
                    .push(egui::Event::Ime(egui::ImeEvent::Preedit("输入中".into()))),
                _ => {}
            }
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
        if matches!(self.request["stage"].as_str(), Some("S5-I2-B" | "S5-I2-C")) {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            ctx.request_repaint();
        }
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
            json!({"frame_id":self.frame_id,"step":self.step,"phase":self.phase,"state":current,"input_ns":self.started.elapsed().as_nanos() as u64,"pass_index":ctx.current_pass_index(),"delivered_input":self.delivered_input,"input_before":self.input_before,"grip_after":app.grip.is_some(),"sequence":app.sequence,"gesture_after":app.drag.as_ref().map(crate::drag::Gesture::evidence_state),"input":ctx.input(|i|json!({"events":events(&i.events),"position":i.pointer.interact_pos().map(|p|[p.x,p.y]),"down":i.pointer.primary_down(),"released":i.pointer.primary_released(),"focused":i.focused,"modifiers":modifiers(i.modifiers)}))}),
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
        if self.request["role"] == "feedback"
            && self.phase == 4
            && self.feedback_pending.is_none()
            && self.request["steps"][self.step]["kind"] == "feedback_move"
            && self.feedback.iter().all(|s| s["step"] != self.step)
        {
            let marker = app
                .object_snap_runtime
                .current
                .as_ref()
                .expect("feedback marker missing");
            let point = app.camera.screen(marker.point, app.canvas_rect);
            let sample = self.request["steps"][self.step]["sample"].as_u64().unwrap();
            let label = format!("feedback-{sample:02}-{}", self.frame_id);
            self.feedback_pending = Some(
                json!({"sample":sample,"step":self.step,"frame_id":self.frame_id,"label":label,"t0_ns":self.delivered_input["t0_ns"],"state":current,"pixel":[(point.x*ctx.pixels_per_point()).round() as usize,(point.y*ctx.pixels_per_point()).round() as usize]}),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                label,
            )));
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
            && self.feedback_pending.is_none()
        {
            let label = format!("step-{:02}", self.step);
            let snapshot = if self.request["role"] == "feedback" {
                None
            } else {
                app.view
                    .snap_snapshot
                    .as_ref()
                    .map(|s| serde_json::to_value(s.as_ref()).unwrap())
            };
            std::fs::write(self.dir.join(format!("{label}.json")),serde_json::to_vec_pretty(&json!({"input":self.request["steps"][self.step],"state":current,"snapshot":snapshot,"completed_frame":completed})).unwrap()).unwrap();
            if self.request["role"] != "feedback" {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    label.clone(),
                )));
            }
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
        let mut observation = json!({"schema_version":2,"stage":self.request.get("stage").cloned().unwrap_or(json!("S5-I1")),"request":self.request,"records":self.records,"frames":self.frames,"error":error,"adapter":app.adapter,"profile":"release","commit":option_env!("RCAM_BUILD_COMMIT"),"build_source":option_env!("RCAM_BUILD_SOURCE"),"binary_sha256":sha256_hex(&binary),"measurement_scope":"synthetic egui input, real worker and production Metal completion fence; functional evidence, no physical latency or PMIX claim"});
        if error.is_some() {
            observation["failure_context"] = json!({"step":self.step,"phase":self.phase,"request_step":self.request["steps"].get(self.step),"input_before":self.input_before,"state":state(app)});
        }
        if matches!(self.request["stage"].as_str(), Some("S5-I2-B" | "S5-I2-C")) {
            let mut shards = Vec::new();
            for (n, frames) in self.frames.chunks(2000).enumerate() {
                let path = format!("frames-{n:03}.json");
                let bytes = serde_json::to_vec(frames).unwrap();
                assert!(bytes.len() <= 128 * 1024 * 1024);
                std::fs::write(self.dir.join(&path), &bytes).unwrap();
                shards.push(json!({"path":path,"count":frames.len(),"sha256":sha256_hex(&bytes)}));
            }
            observation.as_object_mut().unwrap().remove("frames");
            observation["frame_files"] = json!(shards);
            observation["evidence_version"] = json!(3);
            observation["feedback"] = json!(self.feedback);
        }
        let bytes = if matches!(self.request["stage"].as_str(), Some("S5-I2-B" | "S5-I2-C")) {
            serde_json::to_vec(&observation)
        } else {
            serde_json::to_vec_pretty(&observation)
        }
        .unwrap();
        std::fs::write(self.dir.join("observations.json"), bytes).unwrap();
        self.finished = true;
        app.allow_quit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
