//! Internal ownership for the single worker and independently held documents.
use crate::state::{Action, Model, ModelSessionState, View};
use editor_service::{ServiceError, task::TaskContext};
use eframe::egui;
use std::{collections::HashMap, sync::Arc};

pub(crate) type Request = (
    u64,
    rcam_diagnostics::Source,
    Action,
    TaskContext,
    RequestRoute,
);
pub(crate) type Reply = (u64, View, ReplyRoute);

#[derive(Clone, Debug)]
pub(crate) struct Owner {
    namespace: Arc<()>,
    slot: u64,
    generation: u64,
}
impl PartialEq for Owner {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.namespace, &other.namespace)
            && self.slot == other.slot
            && self.generation == other.generation
    }
}
impl Eq for Owner {}
impl Owner {
    pub(crate) fn slot(&self) -> u64 {
        self.slot
    }
    fn next(&self, generation: u64) -> Self {
        Self {
            namespace: self.namespace.clone(),
            slot: self.slot,
            generation,
        }
    }
}
fn binding(view: &View) -> Option<String> {
    view.info.as_ref().map(|info| info.document_id.clone())
}
fn error(message: &str) -> ServiceError {
    ServiceError {
        code: "STALE_TASK".into(),
        message: message.into(),
        details: serde_json::json!({}),
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RequestRoute {
    owner: Owner,
    binding: Option<String>,
    replacement: Option<Owner>,
    save: bool,
    recovery: bool,
    empty_import: bool,
    intent: Option<Intent>,
    registration: bool,
    retire: bool,
    close: bool,
    recovery_key: String,
}
#[derive(Clone, Debug)]
pub(crate) struct ReplyRoute {
    request_owner: Owner,
    result_owner: Owner,
    rejected: Option<ServiceError>,
    startup: bool,
    retired: bool,
}
impl RequestRoute {
    /// Enumerate every Action: adding a new action requires an ownership choice.
    fn changes_binding(action: &Action, empty: bool) -> bool {
        match action {
            Action::Open(..)
            | Action::OpenProject(..)
            | Action::RestoreProject(..)
            | Action::RestoreSnapshot(..)
            | Action::NewWorkspace
            | Action::DiscardNewWorkspace
            | Action::Close(..) => true,
            Action::ImportGerbers(..) => empty,
            Action::UnifiedEditor(..)
            | Action::DefinitionCenters(..)
            | Action::PointPreview(..)
            | Action::PointApply(..)
            | Action::SelectionCenters(..)
            | Action::PnpPreview(..)
            | Action::PnpImport(..)
            | Action::BoardRegistration(..)
            | Action::ComponentSearch(..)
            | Action::CandidateQuery(..)
            | Action::CandidateSelect(..)
            | Action::ArrayPreview(..)
            | Action::ArrayApply(..)
            | Action::BlockEdit(..)
            | Action::BlockPreview(..)
            | Action::BlockSelect(..)
            | Action::Precision(..)
            | Action::SaveProject(..)
            | Action::ProjectWorkspace(..)
            | Action::RecoveryWrite(..)
            | Action::CreateEmptyLayer(..)
            | Action::LayerSummary(..)
            | Action::RemoveLayer(..)
            | Action::ReorderLayers(..)
            | Action::SetActiveLayer(..)
            | Action::SetSoloLayer(..)
            | Action::SetAllLayersVisible(..)
            | Action::ResetLayerColors
            | Action::FitLayer(..)
            | Action::TextFont(..)
            | Action::FontCatalog
            | Action::SystemFont(..)
            | Action::TextPreview(..)
            | Action::TextCreate(..)
            | Action::Select(..)
            | Action::CanvasSelect(..)
            | Action::SelectRect(..)
            | Action::CanvasSelectRect(..)
            | Action::SelectAll
            | Action::Move(..)
            | Action::Align(..)
            | Action::Distribute(..)
            | Action::SetFlashSize(..)
            | Action::Rotate(..)
            | Action::Mirror(..)
            | Action::ProbeDrag(..)
            | Action::DragMove(..)
            | Action::GripEdit(..)
            | Action::Duplicate
            | Action::Delete
            | Action::History(..)
            | Action::Layer(..)
            | Action::Save(..)
            | Action::SaveWithPrecision(..)
            | Action::Viewport(..) => false,
            #[cfg(test)]
            Action::Rebuild(..) => false,
        }
    }
    fn reply(&self, view: &View) -> Result<ReplyRoute, ServiceError> {
        let result_owner = if binding(view) != self.binding {
            self.replacement
                .clone()
                .ok_or_else(|| error("任务未获准更换工程归属"))?
        } else {
            self.owner.clone()
        };
        // A Failed receipt can describe a committed document followed by a
        // refresh failure. The authoritative binding, not error text, decides.
        Ok(ReplyRoute {
            request_owner: self.owner.clone(),
            result_owner,
            rejected: None,
            startup: false,
            retired: self.retire && view.info.is_none(),
        })
    }
}
#[derive(Clone)]
struct Accepted {
    route: RequestRoute,
    task: TaskContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Intent {
    owner: Owner,
    serial: u64,
}
pub(crate) struct Permit {
    route: ReplyRoute,
    accepted: Option<Box<Accepted>>,
}
impl Permit {
    pub(crate) fn close(&self) -> bool {
        self.accepted.as_ref().is_some_and(|a| a.route.close)
    }
    pub(crate) fn retired(&self) -> bool {
        self.route.retired
    }
    pub(crate) fn startup(&self) -> bool {
        self.route.startup
    }
    pub(crate) fn recovery(&self) -> bool {
        self.accepted.as_ref().is_some_and(|a| a.route.recovery)
    }
    pub(crate) fn save(&self) -> bool {
        self.accepted.as_ref().is_some_and(|a| a.route.save)
    }
    pub(crate) fn empty_import(&self, id: u64, before: &Owner) -> bool {
        self.accepted.as_ref().is_some_and(|a| {
            a.route.empty_import && a.task.task_id == id && &a.route.owner == before
        }) && self.route.result_owner != *before
    }
}
pub(crate) enum Gate {
    Ignore,
    Result(Permit),
    Rejected(ServiceError),
}
pub(crate) struct SessionRouting {
    owner: Owner,
    binding: Option<String>,
    next_generation: u64,
    pending: HashMap<u64, Box<Accepted>>,
    startup: bool,
    awaiting_startup: bool,
    disconnected: bool,
    intent: Option<Intent>,
    intent_serial: u64,
    installed_save: Option<(u64, Intent, bool)>,
    recovery_namespace: Arc<str>,
    registration: bool,
    retire_on_close: bool,
}
impl Default for SessionRouting {
    fn default() -> Self {
        Self {
            owner: Owner {
                namespace: Arc::new(()),
                slot: 1,
                generation: 0,
            },
            binding: None,
            next_generation: 0,
            pending: HashMap::new(),
            startup: true,
            awaiting_startup: false,
            disconnected: false,
            intent: None,
            intent_serial: 0,
            installed_save: None,
            recovery_namespace: format!(
                "{}:{:?}:{:?}",
                std::process::id(),
                std::time::SystemTime::now(),
                std::time::Instant::now()
            )
            .into(),
            registration: false,
            retire_on_close: false,
        }
    }
}
impl ReplyRoute {
    pub(crate) fn slot(&self) -> u64 {
        self.request_owner.slot
    }
}
impl SessionRouting {
    pub(crate) fn fork(&self, slot: u64) -> Self {
        Self {
            owner: Owner {
                namespace: self.owner.namespace.clone(),
                slot,
                generation: 0,
            },
            recovery_namespace: self.recovery_namespace.clone(),
            registration: true,
            ..Default::default()
        }
    }
    pub(crate) fn recovery_key(&self) -> String {
        editor_core::hash::sha256_hex(
            format!(
                "{}:{}:{}",
                self.recovery_namespace, self.owner.slot, self.owner.generation
            )
            .as_bytes(),
        )
    }
    pub(crate) fn idle(&self) -> bool {
        self.pending.is_empty() && !self.awaiting_startup && !self.disconnected
    }
    pub(crate) fn retire_on_close(&mut self, retire: bool) {
        self.retire_on_close = retire;
    }
    pub(crate) fn owner(&self) -> Owner {
        self.owner.clone()
    }
    pub(crate) fn await_startup(&mut self, requested: bool) {
        self.awaiting_startup = requested;
    }
    pub(crate) fn prepare(
        &mut self,
        action: &Action,
        view: &View,
    ) -> Result<RequestRoute, ServiceError> {
        if self.disconnected {
            return Err(error("后台连接已关闭，请重新启动"));
        }
        if self.awaiting_startup || self.binding != binding(view) {
            return Err(error("界面工程归属未确认"));
        }
        let replacement = if RequestRoute::changes_binding(action, self.binding.is_none()) {
            self.next_generation = self
                .next_generation
                .checked_add(1)
                .ok_or_else(|| error("工程会话身份已耗尽"))?;
            Some(self.owner.next(self.next_generation))
        } else {
            None
        };
        Ok(RequestRoute {
            owner: self.owner.clone(),
            binding: self.binding.clone(),
            replacement,
            save: matches!(action, Action::SaveProject(..)),
            recovery: matches!(action, Action::RecoveryWrite(..)),
            empty_import: self.binding.is_none() && matches!(action, Action::ImportGerbers(..)),
            intent: self.intent.clone(),
            registration: self.registration,
            retire: self.retire_on_close && matches!(action, Action::Close(..)),
            close: matches!(action, Action::Close(..)),
            recovery_key: self.recovery_key(),
        })
    }
    pub(crate) fn accepted(&mut self, route: RequestRoute, task: TaskContext) {
        // Every request may carry registration until one owned publication.
        // A full queue does not imply that the worker saw a registration.
        self.startup = false;
        self.installed_save = None;
        assert!(
            self.pending
                .insert(task.task_id, Box::new(Accepted { route, task }))
                .is_none()
        );
    }
    /// Check authority before observations or any lane/document side effects.
    /// Receipt/context fences remain mandatory in the owning original lane.
    pub(crate) fn validate(
        &mut self,
        id: u64,
        view: &View,
        reply: &ReplyRoute,
        current: &View,
    ) -> Gate {
        if reply.startup {
            if self.startup
                && self.awaiting_startup
                && id == 0
                && self.pending.is_empty()
                && self.binding.is_none()
                && current.info.is_none()
                && reply.request_owner == self.owner
                && reply.rejected.is_none()
                && reply.result_owner == self.owner.next(1)
                && view.task_receipt.is_none()
            {
                self.awaiting_startup = false;
                self.startup = false;
                return Gate::Result(Permit {
                    route: reply.clone(),
                    accepted: None,
                });
            }
            return Gate::Ignore;
        }
        let Some(accepted) = self.pending.get(&id) else {
            return Gate::Ignore;
        };
        if accepted.route.owner != reply.request_owner {
            return Gate::Ignore;
        }
        if self.owner != reply.request_owner {
            // A terminal from a retired owner is accounting only. It cannot
            // resurrect that owner or clear any lane of the published owner.
            if terminal_matches(&accepted.task, view)
                && accepted
                    .route
                    .reply(view)
                    .is_ok_and(|r| r.result_owner == reply.result_owner)
            {
                self.pending.remove(&id);
            }
            return Gate::Ignore;
        }
        if accepted.route.binding != binding(current) {
            return Gate::Ignore;
        }
        if let Some(cause) = &reply.rejected {
            if reply.result_owner != reply.request_owner || reply.retired {
                return Gate::Ignore;
            }
            self.pending.remove(&id);
            return Gate::Rejected(cause.clone());
        }
        let Ok(expected) = accepted.route.reply(view) else {
            return Gate::Ignore;
        };
        if reply.result_owner != expected.result_owner || reply.retired != expected.retired {
            return Gate::Ignore;
        }
        let accepted = self.pending.remove(&id).unwrap();
        Gate::Result(Permit {
            route: reply.clone(),
            accepted: Some(accepted),
        })
    }
    /// Only called after the existing exact receipt/full-view install fence.
    pub(crate) fn installed(&mut self, id: u64, view: &View, permit: &Permit) -> bool {
        self.registration = false;
        let changed = self.owner != permit.route.result_owner;
        if changed {
            self.owner = permit.route.result_owner.clone();
            self.next_generation = self.next_generation.max(self.owner.generation);
            self.installed_save = None;
        }
        self.binding = binding(view);
        if let Some(accepted) = &permit.accepted
            && accepted.route.save
            && accepted.task.task_id == id
            && terminal_matches(&accepted.task, view)
            && let Some(intent) = &accepted.route.intent
            && self.intent.as_ref() == Some(intent)
            && intent.owner == self.owner
        {
            self.installed_save = Some((
                id,
                intent.clone(),
                view.error.is_none()
                    && view
                        .task_receipt
                        .as_ref()
                        .is_some_and(|r| r.state == editor_service::task::TaskState::Completed)
                    && view.info.as_ref().is_some_and(|d| !d.project_dirty),
            ));
        }
        changed
    }
    pub(crate) fn claim_intent(&mut self) -> Result<(), ServiceError> {
        if self.intent.is_none() {
            self.intent_serial = self
                .intent_serial
                .checked_add(1)
                .ok_or_else(|| error("工程操作身份已耗尽"))?;
            self.intent = Some(Intent {
                owner: self.owner.clone(),
                serial: self.intent_serial,
            });
        }
        if !self.intent_matches() {
            return Err(error("工程操作归属已过期"));
        }
        Ok(())
    }
    pub(crate) fn intent_matches(&self) -> bool {
        self.intent.as_ref().is_some_and(|i| i.owner == self.owner)
    }
    pub(crate) fn clear_intent(&mut self) {
        self.intent = None;
        self.installed_save = None;
    }
    pub(crate) fn saved_for_intent(&mut self) -> Option<bool> {
        self.installed_save
            .take()
            .and_then(|(_, intent, succeeded)| {
                (self.intent.as_ref() == Some(&intent) && intent.owner == self.owner)
                    .then_some(succeeded)
            })
    }
    pub(crate) fn migrate_import_intent(&mut self, id: u64, before: &Owner, permit: &Permit) {
        if permit.empty_import(id, before)
            && let Some(intent) = &mut self.intent
            && &intent.owner == before
        {
            intent.owner = self.owner.clone();
        }
    }
    pub(crate) fn disconnect(&mut self) {
        self.disconnected = true;
        self.pending.clear();
        self.clear_intent();
    }
    #[cfg(test)]
    pub(crate) fn bind_fixture(&mut self, view: &View) {
        assert!(self.pending.is_empty() || self.binding == binding(view));
        self.binding = binding(view);
        self.startup = false;
    }
    #[cfg(test)]
    pub(crate) fn reply_fixture(&self, id: u64, view: &View) -> ReplyRoute {
        self.pending.get(&id).map_or_else(
            || ReplyRoute {
                request_owner: self.owner.clone(),
                result_owner: self.owner.clone(),
                rejected: None,
                startup: false,
                retired: false,
            },
            |a| {
                a.route.reply(view).unwrap_or_else(|_| ReplyRoute {
                    request_owner: a.route.owner.clone(),
                    result_owner: a.route.owner.clone(),
                    rejected: None,
                    startup: false,
                    retired: false,
                })
            },
        )
    }
    #[cfg(test)]
    pub(crate) fn saved_fixture(&mut self) {
        self.claim_intent().unwrap();
        self.installed_save = Some((0, self.intent.clone().unwrap(), true));
    }
}
fn terminal_matches(task: &TaskContext, view: &View) -> bool {
    view.task_receipt.as_ref().is_some_and(|r| {
        r.task_id == task.task_id
            && r.input == task.input
            && r.result_version
                == editor_service::task::TaskVersion::capture(
                    view.info.as_ref(),
                    view.task_generation,
                    view.rule_revision,
                )
            && matches!(
                r.state,
                editor_service::task::TaskState::Completed
                    | editor_service::task::TaskState::Cancelled
                    | editor_service::task::TaskState::Failed
            )
    })
}

struct Record {
    owner: Owner,
    state: Option<ModelSessionState>,
}
pub(crate) struct WorkerHost {
    pub(crate) model: Model,
    records: HashMap<u64, Record>,
    executing: u64,
    namespace: Arc<()>,
    last_slot: u64,
}
impl WorkerHost {
    pub(crate) fn new(owner: Owner, model: Model) -> Self {
        let executing = owner.slot;
        let namespace = owner.namespace.clone();
        let records = HashMap::from([(executing, Record { owner, state: None })]);
        Self {
            model,
            records,
            executing,
            namespace,
            last_slot: executing,
        }
    }
    pub(crate) fn route_action(
        &mut self,
        request: &RequestRoute,
        action: &Action,
    ) -> Result<(), ServiceError> {
        if !self.records.contains_key(&request.owner.slot) {
            if !request.registration
                || !Arc::ptr_eq(&request.owner.namespace, &self.namespace)
                || request.owner.generation != 0
                || request.binding.is_some()
                || request.owner.slot <= self.last_slot
                || self.records.len() >= crate::multiproject::MAX_SESSIONS
                || !matches!(
                    action,
                    Action::NewWorkspace
                        | Action::OpenProject(..)
                        | Action::RestoreProject(..)
                        | Action::RestoreSnapshot(..)
                )
            {
                return Err(error("新工程登记无效或会话数量已达上限"));
            }
            self.last_slot = request.owner.slot;
            self.records.insert(
                request.owner.slot,
                Record {
                    owner: request.owner.clone(),
                    state: Some(ModelSessionState::empty()),
                },
            );
        }
        self.route(request)?;
        self.model.recovery_key = Some(request.recovery_key.clone());
        self.model.reserved_paths = self
            .records
            .iter()
            .filter(|(slot, _)| **slot != self.executing)
            .filter_map(|(_, record)| {
                record
                    .state
                    .as_ref()?
                    .view()
                    .info
                    .as_ref()?
                    .project_path
                    .as_ref()
                    .map(std::path::PathBuf::from)
            })
            .collect();
        Ok(())
    }
    pub(crate) fn route(&mut self, request: &RequestRoute) -> Result<(), ServiceError> {
        let Some(record) = self.records.get(&request.owner.slot) else {
            return Err(error("工程会话不存在"));
        };
        if record.owner != request.owner {
            return Err(error("工程会话已过期"));
        }
        let current = if self.executing == request.owner.slot {
            &self.model.view
        } else {
            record
                .state
                .as_ref()
                .ok_or_else(|| error("工程会话状态不存在"))?
                .view()
        };
        if binding(current) != request.binding {
            return Err(error("后台工程归属已过期"));
        }
        if self.executing != request.owner.slot {
            let state = self
                .records
                .get_mut(&request.owner.slot)
                .unwrap()
                .state
                .take()
                .unwrap();
            let old = self.model.take_session_state();
            if let Some(record) = self.records.get_mut(&self.executing) {
                record.state = Some(old);
            }
            self.model.install_session_state(state);
            self.executing = request.owner.slot;
        }
        Ok(())
    }
    pub(crate) fn finish(&mut self, request: &RequestRoute) -> Result<ReplyRoute, ServiceError> {
        if self.records[&self.executing].owner != request.owner {
            return Err(error("执行工程归属已过期"));
        }
        let actual = self.model.task_version()?;
        if actual
            != editor_service::task::TaskVersion::capture(
                self.model.view.info.as_ref(),
                self.model.view.task_generation,
                self.model.view.rule_revision,
            )
        {
            return Err(error("后台工程版本未确认"));
        }
        let reply = request.reply(&self.model.view)?;
        if reply.retired {
            self.records.remove(&self.executing);
        } else {
            self.records.get_mut(&self.executing).unwrap().owner = reply.result_owner.clone();
        }
        Ok(reply)
    }
    pub(crate) fn rejection(request: &RequestRoute, cause: ServiceError) -> ReplyRoute {
        ReplyRoute {
            request_owner: request.owner.clone(),
            result_owner: request.owner.clone(),
            rejected: Some(cause),
            startup: false,
            retired: false,
        }
    }
    #[cfg(any(test, feature = "internal-evidence"))]
    pub(crate) fn startup(&mut self) -> ReplyRoute {
        let record = self.records.get_mut(&self.executing).unwrap();
        let before = record.owner.clone();
        record.owner = before.next(1);
        ReplyRoute {
            request_owner: before,
            result_owner: record.owner.clone(),
            rejected: None,
            startup: true,
            retired: false,
        }
    }
}

/// The production bounded-channel worker, shared with real threaded regressions.
pub(crate) fn run_worker(
    mut host: WorkerHost,
    request: std::sync::mpsc::Receiver<Request>,
    reply: std::sync::mpsc::SyncSender<Reply>,
    ctx: egui::Context,
) {
    #[cfg(feature = "internal-evidence")]
    use crate::{native_a2, native_pmix, native_s5m1};
    use std::time::Instant;
    while let Ok((id, source, action, task, route)) = request.recv() {
        if let Err(cause) = host.route_action(&route, &action) {
            if reply
                .send((id, View::default(), WorkerHost::rejection(&route, cause)))
                .is_err()
            {
                break;
            }
            ctx.request_repaint();
            continue;
        }
        let model = &mut host.model;
        let start = Instant::now();
        #[cfg(feature = "internal-evidence")]
        let measured_action = native_s5m1::action_label(&action);
        #[cfg(feature = "internal-evidence")]
        let pmix_action = native_pmix::action_label(&action);
        #[cfg(feature = "internal-evidence")]
        native_a2::worker_begin(&task, &model.view);
        rcam_diagnostics::with_source(source, || model.run_task(task, action));
        #[cfg(feature = "internal-evidence")]
        native_a2::worker_finished(id, &model.view);
        #[cfg(feature = "internal-evidence")]
        native_s5m1::worker_result(id, measured_action, start, &model.view);
        #[cfg(feature = "internal-evidence")]
        native_pmix::worker_result(id, pmix_action, start, &model.view);
        if start.elapsed().as_millis() > 100 {
            rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Warn, "gui.worker.slow");
        }
        #[cfg(feature = "internal-evidence")]
        native_a2::returning(id);
        let result = model.view.clone();
        let envelope = match host.finish(&route) {
            Ok(route) => (id, result, route),
            Err(cause) => (id, View::default(), WorkerHost::rejection(&route, cause)),
        };
        if reply.send(envelope).is_err() {
            break;
        }
        ctx.request_repaint();
        #[cfg(feature = "internal-evidence")]
        native_s5m1::gpu_event("worker-request-repaint", 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_service::task::{TaskReceipt, TaskState, TaskVersion};

    fn execute(
        routing: &mut SessionRouting,
        model: &mut Model,
        id: u64,
        action: Action,
    ) -> (View, ReplyRoute) {
        let route = routing.prepare(&action, &model.view).unwrap();
        let task = TaskContext::new(id, model.task_version().unwrap());
        routing.accepted(route.clone(), task.clone());
        model.run_task(task, action);
        (model.view.clone(), route.reply(&model.view).unwrap())
    }
    fn permit(gate: Gate) -> Permit {
        match gate {
            Gate::Result(permit) => permit,
            _ => panic!("expected owned result"),
        }
    }
    #[test]
    fn foreign_and_forged_same_id_leave_real_terminal_pending() {
        let mut routing = SessionRouting::default();
        let mut model = Model::default();
        let current = model.view.clone();
        let (result, reply) = execute(&mut routing, &mut model, 1, Action::NewWorkspace);
        let mut foreign = reply.clone();
        foreign.request_owner.namespace = Arc::new(());
        assert!(matches!(
            routing.validate(1, &result, &foreign, &current),
            Gate::Ignore
        ));
        assert_eq!(routing.pending.len(), 1);
        let mut forged = reply.clone();
        forged.result_owner.generation += 1;
        assert!(matches!(
            routing.validate(1, &result, &forged, &current),
            Gate::Ignore
        ));
        let publication = permit(routing.validate(1, &result, &reply, &current));
        assert!(routing.installed(1, &result, &publication));
        assert!(routing.pending.is_empty());
        assert!(matches!(
            routing.validate(1, &result, &reply, &result),
            Gate::Ignore
        ));
    }
    #[test]
    fn owned_rejection_releases_exact_record_without_publishing_view() {
        let mut routing = SessionRouting::default();
        let current = View::default();
        let route = routing.prepare(&Action::NewWorkspace, &current).unwrap();
        routing.accepted(route.clone(), TaskContext::new(7, TaskVersion::default()));
        let reply = WorkerHost::rejection(&route, error("owned rejection"));
        assert!(matches!(
            routing.validate(7, &View::default(), &reply, &current),
            Gate::Rejected(_)
        ));
        assert!(routing.pending.is_empty());
        assert!(routing.binding.is_none());
        assert_eq!(routing.owner.generation, 0);
        assert!(matches!(
            routing.validate(7, &View::default(), &reply, &current),
            Gate::Ignore
        ));
    }
    #[test]
    fn failed_lifecycle_publication_and_retired_ack_do_not_resurrect() {
        let mut routing = SessionRouting::default();
        let mut model = Model::default();
        let current = model.view.clone();
        let old_route = routing.prepare(&Action::FontCatalog, &current).unwrap();
        let old_task = TaskContext::new(8, model.task_version().unwrap());
        routing.accepted(old_route.clone(), old_task.clone());
        let (mut result, reply) = execute(&mut routing, &mut model, 9, Action::NewWorkspace);
        result.error = Some(error("display refresh failed after commit"));
        result.task_receipt.as_mut().unwrap().state = TaskState::Failed;
        let publication = permit(routing.validate(9, &result, &reply, &current));
        assert!(routing.installed(9, &result, &publication));
        let owner = routing.owner();
        let mut retired = current.clone();
        retired.task_receipt = Some(TaskReceipt {
            task_id: 8,
            input: old_task.input.clone(),
            result_version: old_task.input,
            state: TaskState::Cancelled,
        });
        let retired_route = old_route.reply(&retired).unwrap();
        assert!(matches!(
            routing.validate(8, &retired, &retired_route, &result),
            Gate::Ignore
        ));
        assert!(routing.pending.is_empty());
        assert_eq!(routing.owner(), owner);
        assert_eq!(routing.binding, binding(&result));
    }
    #[test]
    fn startup_has_one_explicit_outcome_and_blocks_competing_requests() {
        for failed in [false, true] {
            let mut routing = SessionRouting::default();
            routing.await_startup(true);
            let current = View::default();
            assert!(routing.prepare(&Action::NewWorkspace, &current).is_err());
            let mut host = WorkerHost::new(routing.owner(), Model::default());
            if failed {
                host.model.view.error = Some(error("autoload failed"));
            } else {
                host.model.new_workspace(false).unwrap();
            }
            let reply = host.startup();
            assert!(matches!(
                routing.validate(1, &host.model.view, &reply, &current),
                Gate::Ignore
            ));
            let publication = permit(routing.validate(0, &host.model.view, &reply, &current));
            assert!(routing.installed(0, &host.model.view, &publication));
            assert!(matches!(
                routing.validate(0, &host.model.view, &reply, &host.model.view),
                Gate::Ignore
            ));
            assert!(
                routing
                    .prepare(&Action::DiscardNewWorkspace, &host.model.view)
                    .is_ok()
            );
        }
    }
    #[test]
    fn save_proof_requires_exact_installed_terminal_and_current_intent() {
        let mut model = Model::default();
        model.new_workspace(false).unwrap();
        let mut routing = SessionRouting::default();
        routing.bind_fixture(&model.view);
        assert!(!routing.intent_matches());
        assert_eq!(routing.saved_for_intent(), None);
        routing.claim_intent().unwrap();
        let current = model.view.clone();
        let action = Action::SaveProject(None, false, None);
        let route = routing.prepare(&action, &current).unwrap();
        let task = TaskContext::new(3, model.task_version().unwrap());
        routing.accepted(route.clone(), task.clone());
        let mut result = current.clone();
        result.task_receipt = Some(TaskReceipt {
            task_id: 3,
            input: task.input.clone(),
            result_version: task.input,
            state: TaskState::Completed,
        });
        result.info.as_mut().unwrap().project_dirty = false;
        let reply = route.reply(&result).unwrap();
        let publication = permit(routing.validate(3, &result, &reply, &current));
        assert_eq!(routing.saved_for_intent(), None); // preliminary gate grants no save authority
        routing.installed(3, &result, &publication);
        assert_eq!(routing.saved_for_intent(), Some(true));
        assert_eq!(routing.saved_for_intent(), None);
        routing.clear_intent();
        routing.installed(3, &result, &publication);
        assert_eq!(routing.saved_for_intent(), None);
    }
    #[test]
    fn checked_generation_exhaustion_cannot_reuse_identity() {
        let mut routing = SessionRouting {
            next_generation: u64::MAX,
            ..Default::default()
        };
        assert!(
            routing
                .prepare(&Action::NewWorkspace, &View::default())
                .is_err()
        );
        assert_eq!(routing.owner.generation, 0);
        assert!(routing.pending.is_empty());
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    use crate::state::session_tests::{fixture, variant_bytes};
    use editor_service::task::TaskVersion;

    fn request(owner: Owner, view: &View) -> RequestRoute {
        RequestRoute {
            owner,
            binding: binding(view),
            replacement: None,
            save: false,
            recovery: false,
            empty_import: false,
            intent: None,
            registration: false,
            retire: false,
            close: false,
            recovery_key: editor_core::hash::sha256_hex(b"legacy fixture"),
        }
    }
    fn apply(host: &mut WorkerHost, route: &RequestRoute, id: u64, action: Action) {
        host.route(route).unwrap();
        let task = TaskContext::new(id, host.model.task_version().unwrap());
        host.model.run_task(task, action);
        assert!(
            host.model.view.error.is_none(),
            "{:?}",
            host.model.view.error
        );
        host.finish(route).unwrap();
    }
    #[test]
    fn two_real_records_share_one_service_and_keep_colliding_geometry_edit_history_isolated() {
        let mut model = fixture();
        let a_view = model.view.clone();
        let bytes = variant_bytes(&model);
        let a_state = model.take_session_state();
        // Only this synthetic registry test creates a second slot. There is no
        // production registration/switch API and no second ApplicationService.
        model.restore_project(&bytes).unwrap();
        model.run(Action::SelectAll);
        let b_view = model.view.clone();
        assert_ne!(binding(&a_view), binding(&b_view));
        assert_eq!(
            a_view.info.as_ref().unwrap().project_id,
            b_view.info.as_ref().unwrap().project_id
        );
        assert_eq!(
            a_view.block_definitions[0].id,
            b_view.block_definitions[0].id
        );
        assert_eq!(
            a_view.block_definitions[0].revision,
            b_view.block_definitions[0].revision
        );
        assert_eq!(
            a_view.selected.ordered[0].object.object_id,
            b_view.selected.ordered[0].object.object_id
        );
        assert_ne!(a_view.bounds, b_view.bounds);
        assert!(b_view.scene.as_ref().unwrap().serial > a_view.scene.as_ref().unwrap().serial);
        let a_owner = SessionRouting::default().owner();
        let b_owner = Owner {
            namespace: a_owner.namespace.clone(),
            slot: 2,
            generation: 0,
        };
        let mut host = WorkerHost::new(b_owner.clone(), model);
        host.records.insert(
            1,
            Record {
                owner: a_owner.clone(),
                state: Some(a_state),
            },
        );
        let a = request(a_owner, &a_view);
        let b = request(b_owner, &b_view);
        let b_bytes = host
            .model
            .service
            .project_recovery_bytes(b.binding.as_deref().unwrap())
            .unwrap();
        let b_scene = host.model.view.scene.clone().unwrap();
        let b_selected = host.model.view.selected.ordered.clone();
        let mut bad = a.clone();
        bad.binding = Some("foreign-document".into());
        assert!(host.route(&bad).is_err());
        assert_eq!(host.executing, 2);
        assert!(Arc::ptr_eq(
            &b_scene,
            host.model.view.scene.as_ref().unwrap()
        ));
        let mut foreign = a.clone();
        foreign.owner.namespace = Arc::new(());
        assert!(host.route(&foreign).is_err());
        assert_eq!(host.executing, 2);
        apply(&mut host, &a, 1, Action::Move("3".into(), "0".into()));
        let a_after = host.model.view.clone();
        assert_eq!(
            a_after.info.as_ref().unwrap().undo_entries,
            a_view.info.as_ref().unwrap().undo_entries + 1
        );
        assert!(a_after.scene.as_ref().unwrap().serial > b_scene.serial);
        assert_eq!(
            host.model
                .service
                .project_recovery_bytes(b.binding.as_deref().unwrap())
                .unwrap(),
            b_bytes
        );
        host.route(&b).unwrap();
        assert!(Arc::ptr_eq(
            &b_scene,
            host.model.view.scene.as_ref().unwrap()
        ));
        assert!(b_selected.shares_storage(&host.model.view.selected.ordered));
        assert_eq!(host.model.view.bounds, b_view.bounds);
        apply(&mut host, &b, 2, Action::Move("0".into(), "7".into()));
        assert!(
            host.model.view.scene.as_ref().unwrap().serial > a_after.scene.as_ref().unwrap().serial
        );
        apply(&mut host, &b, 3, Action::History(false));
        assert_eq!(
            host.model
                .service
                .project_recovery_bytes(b.binding.as_deref().unwrap())
                .unwrap(),
            b_bytes
        );
        host.route(&a).unwrap();
        assert_eq!(host.model.view.info, a_after.info);
        assert_eq!(host.model.view.bounds, a_after.bounds);
        apply(&mut host, &a, 4, Action::History(false));
        assert_eq!(host.model.view.bounds, a_view.bounds);
        assert_eq!(
            host.model.view.info.as_ref().unwrap().project_dirty,
            a_view.info.as_ref().unwrap().project_dirty
        );
        host.route(&b).unwrap();
        assert_eq!(host.model.view.bounds, b_view.bounds);
        assert_eq!(
            host.model.task_version().unwrap(),
            TaskVersion::capture(
                host.model.view.info.as_ref(),
                host.model.view.task_generation,
                host.model.view.rule_revision
            )
        );
        assert_eq!(host.records.len(), 2);
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    use crate::{EditorApp, project_ui::Transition};
    use eframe::{App, egui};
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

    struct Run {
        app: EditorApp,
        host: WorkerHost,
        requests: Receiver<Request>,
        replies: SyncSender<Reply>,
        ctx: egui::Context,
        frame: eframe::Frame,
    }
    impl Run {
        fn new() -> Self {
            let model = crate::state::session_tests::fixture();
            let mut app = crate::modal::tests::app();
            app.view = model.view.clone();
            app.routing.bind_fixture(&app.view);
            let host = WorkerHost::new(app.routing.owner(), model);
            let (tx, requests) = sync_channel(16);
            let (replies, rx) = sync_channel(16);
            app.tx = tx;
            app.rx = rx;
            let ctx = egui::Context::default();
            ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
            Self {
                app,
                host,
                requests,
                replies,
                ctx,
                frame: eframe::Frame::_new_kittest(),
            }
        }
        fn work(&mut self) -> Reply {
            let (id, source, action, task, route) = self.requests.try_recv().unwrap();
            self.host.route(&route).unwrap();
            rcam_diagnostics::with_source(source, || self.host.model.run_task(task, action));
            let publication = self.host.finish(&route).unwrap();
            (id, self.host.model.view.clone(), publication)
        }
        fn no_document_request(&self) {
            while let Ok((_, _, action, _, _)) = self.requests.try_recv() {
                assert!(matches!(
                    action,
                    Action::SelectionCenters(..) | Action::Viewport(..)
                ));
            }
        }
        fn deliver(&mut self, reply: Reply) {
            self.replies.send(reply).unwrap();
            let raw = egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 900.),
                )),
                ..Default::default()
            };
            let _ = self
                .ctx
                .run(raw, |ctx| self.app.update(ctx, &mut self.frame));
        }
    }
    #[test]
    fn foreign_canvas_terminal_cannot_clear_selection_lane_then_real_and_duplicate_settle_once() {
        let mut run = Run::new();
        run.app.send(Action::SelectAll);
        let result = run.work();
        let before = run.app.view.selected.ordered.clone();
        let mut foreign = result.clone();
        foreign.2.request_owner.namespace = Arc::new(());
        run.deliver(foreign);
        assert!(run.app.selection_read_pending());
        assert!(before.shares_storage(&run.app.view.selected.ordered));
        assert_eq!(run.app.routing.pending.len(), 1);
        run.deliver(result.clone());
        assert!(!run.app.selection_read_pending());
        assert!(!run.app.canvas_selection_unconfirmed);
        let selected = run.app.view.selected.ordered.clone();
        let pending = run.app.routing.pending.len();
        run.deliver(result);
        assert!(selected.shares_storage(&run.app.view.selected.ordered));
        assert_eq!(run.app.routing.pending.len(), pending);
    }
    #[test]
    fn foreign_save_and_malformed_owned_save_cannot_close_dirty_source() {
        for malformed in [false, true] {
            let mut run = Run::new();
            let dir = std::env::temp_dir().join(format!(
                "rcam-session-save-{}-{malformed}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("saved.rcam");
            run.app
                .begin_transition(Transition::Open(dir.join("next.rcam")));
            assert!(run.app.close_prompt);
            let before = run.app.view.info.clone();
            run.app
                .send(Action::SaveProject(Some(path.clone()), false, None));
            run.app.waiting_save = true;
            run.app.close_prompt = false;
            let result = run.work();
            assert!(result.1.error.is_none());
            let mut foreign = result.clone();
            foreign.2.request_owner.namespace = Arc::new(());
            run.deliver(foreign);
            assert!(run.app.busy && run.app.waiting_save);
            assert_eq!(run.app.view.info, before);
            assert!(run.requests.try_recv().is_err());
            if malformed {
                let mut broken = result;
                broken.1.task_receipt.as_mut().unwrap().task_id += 100;
                run.deliver(broken);
                assert!(!run.app.busy && !run.app.waiting_save);
                assert!(run.app.view.blocked.is_some());
                assert_eq!(run.app.view.info, before);
                run.no_document_request();
            } else {
                run.deliver(result.clone());
                assert!(!run.app.waiting_save);
                let next = run.requests.try_recv().unwrap();
                assert!(matches!(next.2, Action::OpenProject(..)));
                assert!(!run.app.view.info.as_ref().unwrap().project_dirty);
                run.deliver(result);
                assert!(run.requests.try_recv().is_err());
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn ordinary_clean_or_familiar_message_reply_cannot_complete_recovery_or_saved_intent() {
        let mut run = Run::new();
        run.app.routing.claim_intent().unwrap();
        run.app.transition = Some(Transition::Quit);
        run.app.waiting_save = true;
        run.app.pending_recovery_identity = Some("uncompleted snapshot".into());
        run.app.pending_recovery_task = Some((run.app.routing.owner(), 99));
        run.app.send(Action::FontCatalog);
        let mut result = run.work();
        result.1.message = "工程已保存".into();
        result.1.info.as_mut().unwrap().project_dirty = false;
        run.deliver(result);
        assert!(run.app.waiting_save);
        assert!(matches!(run.app.transition, Some(Transition::Quit)));
        assert_eq!(
            run.app.pending_recovery_identity.as_deref(),
            Some("uncompleted snapshot")
        );
        assert!(run.app.last_recovered_identity.is_empty());
        assert!(!run.app.allow_quit);
        run.no_document_request();
    }
    #[test]
    fn queue_failure_consumes_global_id_and_exhaustion_never_wraps() {
        let mut run = Run::new();
        let (tx, rx) = sync_channel(1);
        let filler = run
            .app
            .routing
            .prepare(&Action::FontCatalog, &run.app.view)
            .unwrap();
        tx.send((
            99,
            rcam_diagnostics::Source::System,
            Action::FontCatalog,
            TaskContext::new(99, run.host.model.task_version().unwrap()),
            filler,
        ))
        .unwrap();
        run.app.tx = tx;
        run.app.send(Action::FontCatalog);
        assert_eq!(run.app.sequence, 0);
        assert_eq!(run.app.task_serial, 1);
        assert!(run.app.routing.pending.is_empty());
        rx.try_recv().unwrap();
        run.app.send(Action::FontCatalog);
        assert_eq!(rx.try_recv().unwrap().0, 2);
        assert_eq!(run.app.sequence, 2);
        run.app.busy = false;
        run.app.task_serial = u64::MAX;
        run.app.send(Action::FontCatalog);
        assert_eq!(run.app.task_serial, u64::MAX);
        assert_eq!(run.app.sequence, 2);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn rejected_old_read_cannot_cancel_current_save_intent() {
        let mut run = Run::new();
        run.app.send(Action::Viewport(
            editor_core::MmPoint::new(0., 0.),
            editor_core::BoundsMm {
                min_x_mm: -5.,
                min_y_mm: -5.,
                max_x_mm: 5.,
                max_y_mm: 5.,
            },
            20.,
        ));
        let read = run.requests.try_recv().unwrap();
        run.app.begin_transition(Transition::Quit);
        run.app.send(Action::SaveProject(None, false, None));
        run.app.waiting_save = true;
        run.app.close_prompt = false;
        let save_id = run.app.pending_task.as_ref().unwrap().task_id;
        let rejection = WorkerHost::rejection(&read.4, error("old read rejected"));
        run.deliver((read.0, View::default(), rejection));
        assert!(run.app.busy && run.app.waiting_save);
        assert_eq!(run.app.pending_task.as_ref().unwrap().task_id, save_id);
        assert!(run.app.routing.intent_matches());
        assert!(matches!(run.app.transition, Some(Transition::Quit)));
        assert!(run.app.viewport_task.is_none());
        assert!(run.app.view.blocked.is_none());
    }
    #[test]
    fn ordinary_save_disconnect_releases_lane_and_blocks_unconfirmed_write() {
        let mut run = Run::new();
        run.app.begin_transition(Transition::Quit);
        run.app.send(Action::SaveProject(None, false, None));
        run.app.waiting_save = true;
        run.app.close_prompt = false;
        let before = run.app.view.info.clone();
        let (tx, rx) = sync_channel(1);
        drop(tx);
        run.app.rx = rx;
        let raw = egui::RawInput {
            focused: true,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 900.),
            )),
            ..Default::default()
        };
        let _ = run.ctx.run(raw, |ctx| run.app.update(ctx, &mut run.frame));
        assert!(!run.app.busy && !run.app.waiting_save);
        assert!(run.app.pending_task.is_none());
        assert!(run.app.routing.pending.is_empty());
        assert!(run.app.transition.is_none() && !run.app.allow_quit);
        assert_eq!(run.app.view.info, before);
        assert!(run.app.view.blocked.is_some());
    }
}
