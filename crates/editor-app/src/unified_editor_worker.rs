//! One real worker/service; all prospective artifacts are local until publication.
use crate::{
    camera::Camera,
    display::Scene,
    point_input::Context,
    state::{Model, View},
    unified_editor_resources as budget,
    world_index::WorldIndex,
};
use editor_core::{BoundsMm, MmPoint, SemanticGeometry};
use editor_service::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};
#[derive(Clone, Copy, Debug)]
pub struct RenderInput {
    pub camera: Camera,
    pub rect: eframe::egui::Rect,
    pub ppp: f32,
    pub ppm: f64,
    pub retained_ui_bytes: usize,
}
#[derive(Clone, Debug)]
pub enum Command {
    Begin,
    Preview(DraftStep),
    Execute(DraftStep),
    History(UnifiedEditorHistory),
    Apply(Option<DraftStep>),
    Cancel,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub draft: Arc<()>,
    pub request: Arc<()>,
    pub generation: u64,
    pub context: Context,
    pub command: Command,
    pub render: RenderInput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Open,
    Changed,
    NoChange,
    Cancelled,
    Refused,
}
pub struct Artifacts {
    pub snapshot: Arc<RenderSnapshot>,
    pub index: Arc<WorldIndex>,
    pub scene: Arc<Scene>,
    pub bounds: Option<BoundsMm>,
    pub selected: crate::shared_snapshot::SnapshotVec<ObjectInfo>,
    pub owned_bytes: usize,
}
#[derive(Clone)]
pub struct Reply {
    pub draft: Arc<()>,
    pub request: Arc<()>,
    pub generation: u64,
    pub terminal: Terminal,
    pub resources: Option<DraftResources>,
    pub changed_work: bool,
    pub work: Option<Arc<Artifacts>>,
    pub reference: Arc<Vec<Vec<MmPoint>>>,
    pub host_peak_bytes: usize,
}
pub struct WorkerSession {
    pub draft: Arc<()>,
    pub context: Context,
    pub backend: UnifiedEditorSession,
    pub entry_snapshot: Arc<RenderSnapshot>,
    pub entry_scene: Arc<Scene>,
    pub work: Arc<Artifacts>,
    pub preview: Option<Arc<Artifacts>>,
    pub reference: Arc<Vec<Vec<MmPoint>>>,
    pub host_peak_bytes: usize,
}
fn fail(s: &str) -> ServiceError {
    ServiceError {
        code: "STALE_TASK".into(),
        message: s.into(),
        details: serde_json::json!({}),
    }
}
fn check(token: Option<&task::CancellationToken>, started: Instant) -> Result<(), ServiceError> {
    if let Some(t) = token {
        t.checkpoint()?;
    }
    if started.elapsed() > budget::DEADLINE {
        return Err(budget::refuse(
            "编辑会话准备超过现有2秒预算；工作和历史已保留",
        ));
    }
    Ok(())
}
fn barrier(token: Option<&task::CancellationToken>) -> Result<(), ServiceError> {
    if let Some(t) = token {
        t.begin_commit()?;
    }
    Ok(())
}
fn reference_bytes(paths: &Vec<Vec<MmPoint>>) -> usize {
    paths
        .iter()
        .map(|p| p.capacity() * size_of::<MmPoint>() + 64)
        .sum::<usize>()
        + paths.capacity() * size_of::<Vec<MmPoint>>()
}
impl WorkerSession {
    fn live_bytes(&self, main_index: &WorldIndex, view: &View) -> Result<usize, ServiceError> {
        let mut n = budget::add(
            self.backend.resources()?.reserved_peak_bytes,
            budget::selection_cost(&view.selected.ordered)?,
        )?;
        n = budget::add(
            n,
            budget::snapshot_cost(&self.entry_snapshot)?
                + self.entry_scene.owned_bytes()
                + main_index.owned_bytes()
                + reference_bytes(&self.reference),
        )?;
        let mut seen = HashSet::new();
        for a in std::iter::once(&self.work).chain(self.preview.iter()) {
            if seen.insert(Arc::as_ptr(a)) {
                n = budget::add(n, a.owned_bytes)?;
            }
        }
        Ok(n)
    }
    fn reply(&self, r: &Request, terminal: Terminal) -> Reply {
        Reply {
            draft: self.draft.clone(),
            request: r.request.clone(),
            generation: self.backend.generation().unwrap_or(r.generation),
            terminal,
            resources: self.backend.resources().ok(),
            changed_work: self.backend.has_work_changes(),
            work: Some(self.work.clone()),
            reference: self.reference.clone(),
            host_peak_bytes: self.host_peak_bytes,
        }
    }
}
#[allow(clippy::too_many_arguments)] // One prospective-publication context, no second worker.
fn prepare(
    source: &RenderSnapshot,
    candidate: DraftCandidate<'_>,
    view: &View,
    render: RenderInput,
    serial: u64,
    live: usize,
    token: Option<&task::CancellationToken>,
    started: Instant,
) -> Result<(Artifacts, usize), ServiceError> {
    let reserve = budget::candidate_reserve(source, render.ppm, &mut || check(token, started))?;
    let peak = budget::add(budget::add(live, render.retained_ui_bytes)?, reserve)?;
    budget::admit(peak)?;
    check(token, started)?;
    let mut overrides = HashMap::new();
    overrides
        .try_reserve(candidate.objects().len())
        .map_err(|_| budget::refuse("候选槽位分配失败"))?;
    for (l, o, g) in candidate.objects() {
        if overrides.insert((l, o), g).is_some() {
            return Err(fail("候选槽位重复"));
        }
    }
    let mut snapshot = source.clone();
    let mut selected = Vec::new();
    selected
        .try_reserve_exact(overrides.len())
        .map_err(|_| budget::refuse("候选选择分配失败"))?;
    for layer in &mut snapshot.layers {
        for object in &mut layer.objects {
            check(token, started)?;
            if let Some(g) = overrides.remove(&(layer.id.as_str(), object.object_id.as_str())) {
                object.geometry = g.clone();
                selected.push(ObjectInfo {
                    layer_id: layer.id.clone(),
                    object: object.clone(),
                });
            }
        }
    }
    if !overrides.is_empty() {
        return Err(fail("候选槽位未找到"));
    }
    let mut by_id: HashMap<_, _> = selected
        .into_iter()
        .map(|o| ((o.layer_id.clone(), o.object.object_id.clone()), o))
        .collect();
    let selected: Vec<_> = view
        .selected
        .ordered
        .iter()
        .map(|o| {
            by_id
                .remove(&(o.layer_id.clone(), o.object.object_id.clone()))
                .ok_or_else(|| fail("候选选择身份未找到"))
        })
        .collect::<Result<_, _>>()?;
    // Matching f64 manufacture/index first; never derive Snap from Scene paths.
    let index = Arc::new(WorldIndex::build(&snapshot).map_err(|e| fail(&e))?);
    check(token, started)?;
    let bounds = index.visible_bounds(&snapshot, &view.layers);
    let scene = Arc::new(
        Scene::build_cached_with_cancel(
            &snapshot,
            &view.layers,
            render.camera.center,
            render.ppm,
            serial,
            None,
            &mut crate::block_display::BlockDisplayCache::default(),
            token,
        )
        .map_err(|e| ServiceError {
            code: if e == "CANCELLED" {
                "CANCELLED"
            } else {
                "DISPLAY_PREPARE_FAILED"
            }
            .into(),
            message: e,
            details: serde_json::json!({}),
        })?,
    );
    let flags = crate::gpu::selection_flags(&scene, &view.selected.ids());
    crate::gpu::prepare(
        &scene,
        render.camera,
        render.rect,
        render.ppp,
        &flags,
        MmPoint::new(0., 0.),
    )
    .map_err(|e| ServiceError {
        code: "DISPLAY_PREPARE_FAILED".into(),
        message: e,
        details: serde_json::json!({}),
    })?;
    check(token, started)?;
    let owned_bytes = budget::add(
        budget::snapshot_cost(&snapshot)?,
        budget::add(
            index.owned_bytes(),
            budget::add(scene.owned_bytes(), budget::selection_cost(&selected)?)?,
        )?,
    )?;
    budget::admit(budget::add(
        budget::add(live, render.retained_ui_bytes)?,
        owned_bytes,
    )?)?;
    Ok((
        Artifacts {
            snapshot: Arc::new(snapshot),
            index,
            scene,
            bounds,
            selected: selected.into(),
            owned_bytes,
        },
        peak,
    ))
}
fn reference(
    source: &RenderSnapshot,
    candidate: DraftCandidate<'_>,
    ppm: f64,
    token: Option<&task::CancellationToken>,
    started: Instant,
) -> Result<Arc<Vec<Vec<MmPoint>>>, ServiceError> {
    let mut result = Vec::new();
    let mut count = 0usize;
    for (_, _, g) in candidate.objects() {
        check(token, started)?;
        let resolved = if let SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } = g
        {
            let d = source
                .block_definitions
                .iter()
                .find(|d| &d.id == definition_id)
                .ok_or_else(|| fail("入口Block未找到"))?;
            editor_core::block::resolve_instance(d, transform)
                .map_err(|e| fail(&format!("{e:?}")))?
                .into_iter()
                .map(|o| o.geometry)
                .collect::<Vec<_>>()
        } else {
            vec![g.clone()]
        };
        let paths = crate::block_ui::preview_paths(resolved.iter(), &source.apertures, ppm)?;
        count += paths.iter().map(Vec::len).sum::<usize>();
        if count > 200_000 {
            return Err(budget::refuse("入口参考轮廓超过既有显示点预算"));
        }
        result.extend(paths);
    }
    Ok(Arc::new(result))
}
impl Model {
    fn next_unified_serial(&mut self) -> Result<u64, ServiceError> {
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| budget::refuse("显示场景身份耗尽"))?;
        Ok(self.serial)
    }
    fn install_unified_artifact(&mut self, a: &Artifacts, work: &Artifacts) {
        self.view.scene = Some(a.scene.clone());
        self.view.snap_snapshot = Some(work.snapshot.clone());
        self.view.snap_index = work.index.clone();
        self.view.bounds = a.bounds;
        self.view.render_ppm = a.scene.ppm;
        self.view.render_viewport = None;
        self.view.render_coverage_complete = true;
        self.view.display_attempt = None;
        self.view.blocked = None;
        self.view.display_transient = None;
    }
    pub(crate) fn unified_editor_action(&mut self, r: Request) -> Result<(), ServiceError> {
        let token = self.active_cancel.clone();
        let started = Instant::now();
        check(token.as_ref(), started)?;
        if matches!(r.command, Command::Begin) {
            if self.unified_editor.is_some() || !r.context.valid(&self.view) {
                return Err(fail("编辑会话入口已失效或重复"));
            }
            let d = self.info()?;
            let source = self
                .snapshot
                .clone()
                .ok_or_else(|| fail("制造快照不可用"))?;
            let entry_scene = self
                .view
                .scene
                .clone()
                .ok_or_else(|| fail("入口显示不可用"))?;
            let preliminary = budget::candidate_reserve(&source, r.render.ppm, &mut || {
                check(token.as_ref(), started)
            })?;
            budget::admit(
                preliminary
                    + r.render.retained_ui_bytes
                    + budget::selection_cost(&self.view.selected.ordered)?
                    + budget::snapshot_cost(&source)?
                    + entry_scene.owned_bytes()
                    + self.world_index.owned_bytes(),
            )?;
            let backend = self.service.unified_editor_begin(
                &d.document_id,
                &d.revision,
                &d.workspace_revision,
                self.view.selected.groups(),
                token.as_ref(),
            )?;
            let live = budget::selection_cost(&self.view.selected.ordered)?
                + backend.resources()?.reserved_peak_bytes
                + budget::snapshot_cost(&source)?
                + entry_scene.owned_bytes()
                + self.world_index.owned_bytes();
            let serial = self.next_unified_serial()?;
            let (work, peak) = prepare(
                &source,
                backend.work_candidate()?,
                &self.view,
                r.render,
                serial,
                live,
                token.as_ref(),
                started,
            )?;
            let reference = reference(
                &source,
                backend.work_candidate()?,
                r.render.ppm,
                token.as_ref(),
                started,
            )?;
            let peak = budget::add(peak, reference_bytes(&reference))?;
            budget::admit(peak)?;
            check(token.as_ref(), started)?;
            barrier(token.as_ref())?;
            let session = WorkerSession {
                draft: r.draft.clone(),
                context: r.context.clone(),
                backend,
                entry_snapshot: source,
                entry_scene,
                work: Arc::new(work),
                preview: None,
                reference,
                host_peak_bytes: peak,
            };
            self.install_unified_artifact(&session.work, &session.work);
            self.view.unified_editor = Some(Arc::new(session.reply(&r, Terminal::Open)));
            self.unified_editor = Some(session);
            return Ok(());
        }
        let mut session = self
            .unified_editor
            .take()
            .ok_or_else(|| fail("编辑会话不存在"))?;
        let result = (|| {
            if !Arc::ptr_eq(&session.draft, &r.draft)
                || session.context != r.context
                || !r.context.valid(&self.view)
                || r.generation != session.backend.generation()?
            {
                return Err(fail("草稿、输入版本或所属工程已改变"));
            }
            if matches!(r.command, Command::Cancel) {
                check(token.as_ref(), started)?;
                barrier(token.as_ref())?;
                session.backend.cancel();
                self.view.scene = Some(session.entry_scene.clone());
                self.view.snap_snapshot = self.snapshot.clone();
                self.view.snap_index = self.world_index.clone();
                self.view.bounds = self
                    .world_index
                    .visible_bounds(&session.entry_snapshot, &self.view.layers);
                self.view.render_ppm = session.entry_scene.ppm;
                self.view.render_coverage_complete = false;
                self.view.display_attempt = None;
                return Ok(Terminal::Cancelled);
            }
            if let Command::Preview(step) | Command::Execute(step) | Command::Apply(Some(step)) =
                &r.command
            {
                let generation = session.backend.generation()?;
                self.service.unified_editor_set_step(
                    &mut session.backend,
                    generation,
                    step.clone(),
                )?;
            }
            let live = session.live_bytes(&self.world_index, &self.view)?;
            let serial = self.next_unified_serial()?;
            let mut staged = None;
            let source = session.entry_snapshot.clone();
            let view = self.view.clone();
            let mut peak = 0;
            let mut stage = |candidate: DraftCandidate<'_>| {
                let (a, p) = prepare(
                    &source,
                    candidate,
                    &view,
                    r.render,
                    serial,
                    live,
                    token.as_ref(),
                    started,
                )?;
                staged = Some(a);
                peak = p;
                Ok(())
            };
            let generation = session.backend.generation()?;
            let terminal = match &r.command {
                Command::Preview(_) => {
                    self.service.unified_editor_preview_prepared(
                        &mut session.backend,
                        generation,
                        token.as_ref(),
                        &mut stage,
                    )?;
                    Terminal::Open
                }
                Command::Execute(_) => {
                    self.service.unified_editor_execute_prepared(
                        &mut session.backend,
                        generation,
                        token.as_ref(),
                        &mut stage,
                    )?;
                    Terminal::Open
                }
                Command::History(action) => {
                    self.service.unified_editor_history_prepared(
                        &mut session.backend,
                        *action,
                        token.as_ref(),
                        &mut stage,
                    )?;
                    Terminal::Open
                }
                Command::Apply(_) => {
                    let ticket = self
                        .service
                        .unified_editor_begin_apply(&mut session.backend)?;
                    let result = self.service.unified_editor_complete_apply_prepared(
                        &mut session.backend,
                        ticket,
                        token.as_ref(),
                        &mut stage,
                    )?;
                    // No fallible refresh follows the manufacturing commit. Exact result metadata and
                    // prepared geometry are installed even if later optional metrics fail.
                    self.view.info = Some(result.info);
                    if result.changed {
                        let a = staged
                            .as_mut()
                            .expect("successful Apply prepared exactly once");
                        Arc::get_mut(&mut a.snapshot)
                            .expect("staged snapshot is uniquely owned")
                            .revision = result.edit.revision;
                        Terminal::Changed
                    } else {
                        Terminal::NoChange
                    }
                }
                _ => unreachable!(),
            };
            let a = Arc::new(staged.expect("successful publication prepared exactly once"));
            session.host_peak_bytes = peak;
            if matches!(r.command, Command::Preview(_)) {
                session.preview = Some(a.clone());
                self.install_unified_artifact(&a, &session.work);
            } else {
                session.work = a.clone();
                session.preview = None;
                self.install_unified_artifact(&a, &a);
            }
            if terminal == Terminal::Changed {
                self.snapshot = Some(a.snapshot.clone());
                self.world_index = a.index.clone();
                self.view.selected.ordered = a.selected.clone();
                self.metrics_identity.clear();
                self.viewport = None;
                self.ppm = r.render.ppm;
                // Publish the committed selection epoch before App can enqueue
                // geometry reads. Optional metrics failures remain metrics_error
                // and cannot turn the completed manufacturing commit into failure.
                self.refresh_metrics();
            }
            if terminal == Terminal::NoChange {
                self.view.scene = Some(session.entry_scene.clone());
                self.view.snap_snapshot = self.snapshot.clone();
                self.view.snap_index = self.world_index.clone();
                self.view.bounds = self
                    .world_index
                    .visible_bounds(&session.entry_snapshot, &self.view.layers);
                self.view.render_ppm = session.entry_scene.ppm;
                self.view.render_coverage_complete = false;
                self.view.display_attempt = None;
            }
            Ok(terminal)
        })();
        let terminal = result.as_ref().copied().unwrap_or(Terminal::Refused);
        let mut reply = session.reply(&r, terminal);
        if matches!(
            terminal,
            Terminal::Changed | Terminal::NoChange | Terminal::Cancelled
        ) {
            reply.work = None;
            reply.reference = Arc::new(Vec::new());
        }
        self.view.unified_editor = Some(Arc::new(reply));
        if !matches!(
            terminal,
            Terminal::Changed | Terminal::NoChange | Terminal::Cancelled
        ) {
            self.unified_editor = Some(session);
        }
        result.map(|_| ())
    }
}
