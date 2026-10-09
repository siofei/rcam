//! Typed host-only draft preparation. No JSON capability.
use super::*;
use editor_core::edit::ManufacturingDraft;
pub use editor_core::edit::{DraftCandidate, DraftResources, DraftStep};
use std::sync::Arc;

#[derive(Debug)]
pub struct UnifiedEditorSession {
    owner: Arc<()>,
    document_id: String,
    revision: String,
    workspace_revision: u64,
    draft: Option<ManufacturingDraft>,
    pending: Option<Arc<()>>,
    invalid_input: bool,
}
/// Opaque, single-use identity; Reset/Cancel invalidate it even before computation.
#[derive(Debug)]
pub struct UnifiedEditorApplyTicket {
    owner: Arc<()>,
    request: Arc<()>,
    generation: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedEditorApplyResult {
    pub changed: bool,
    pub edit: EditResult,
}
impl UnifiedEditorSession {
    pub fn is_closed(&self) -> bool {
        self.draft.is_none()
    }
    pub fn is_apply_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn generation(&self) -> Result<u64, ServiceError> {
        Ok(self.open()?.generation())
    }
    pub fn entry_geometry(&self) -> Result<&[SemanticGeometry], ServiceError> {
        Ok(self.open()?.entry_geometry())
    }
    pub fn work_candidate(&self) -> Result<DraftCandidate<'_>, ServiceError> {
        Ok(self.open()?.work_candidate())
    }
    pub fn work_geometry(&self) -> Result<&[SemanticGeometry], ServiceError> {
        Ok(self.open()?.work_geometry())
    }
    pub fn preview_geometry(&self) -> Result<Option<&[SemanticGeometry]>, ServiceError> {
        Ok(self.open()?.preview_geometry())
    }
    pub fn resources(&self) -> Result<DraftResources, ServiceError> {
        self.open()?.resources().map_err(map_edit_error)
    }
    pub fn has_work_changes(&self) -> bool {
        self.draft
            .as_ref()
            .is_some_and(ManufacturingDraft::has_work_changes)
    }
    fn open(&self) -> Result<&ManufacturingDraft, ServiceError> {
        self.draft
            .as_ref()
            .ok_or_else(|| ServiceError::invalid("编辑会话已结束"))
    }
    fn idle(&self) -> Result<(), ServiceError> {
        self.open()?;
        if self.pending.is_some() {
            return Err(ServiceError::invalid("最终应用正在等待完成"));
        }
        Ok(())
    }
    fn valid_input(&self) -> Result<(), ServiceError> {
        if self.invalid_input {
            Err(ServiceError::invalid("最新输入无效，请修正或复原"))
        } else {
            Ok(())
        }
    }
    /// Reset and Cancel are local discard operations; stale service fences cannot trap data.
    pub fn reset(&mut self) -> Result<u64, ServiceError> {
        self.open()?;
        let next = self
            .draft
            .as_mut()
            .unwrap()
            .reset()
            .map_err(map_edit_error)?;
        self.pending = None;
        self.invalid_input = false;
        Ok(next)
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.draft = None;
        self.invalid_input = false;
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedEditorHistory {
    Undo,
    Redo,
    Reset,
}
fn prepare_core(
    candidate: DraftCandidate<'_>,
    prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), ServiceError>,
    error: &mut Option<ServiceError>,
) -> Result<(), EditError> {
    prepare(candidate).map_err(|e| {
        *error = Some(e);
        EditError::ResourceLimit
    })
}
fn core_checkpoint(cancel: Option<&task::CancellationToken>) -> Result<(), EditError> {
    if cancel.is_some_and(|c| c.checkpoint().is_err()) {
        Err(EditError::ResourceLimit)
    } else {
        Ok(())
    }
}
fn checked_core<T>(
    result: Result<T, EditError>,
    cancel: Option<&task::CancellationToken>,
) -> Result<T, ServiceError> {
    if let Some(c) = cancel {
        c.checkpoint()?;
    }
    result.map_err(map_edit_error)
}
fn publish_barrier(
    cancel: Option<&task::CancellationToken>,
    error: &mut Option<ServiceError>,
) -> Result<(), EditError> {
    if let Some(c) = cancel
        && let Err(e) = c.begin_commit()
    {
        *error = Some(e);
        return Err(EditError::ResourceLimit);
    }
    Ok(())
}
fn published_core<T>(
    result: Result<T, EditError>,
    cancel: Option<&task::CancellationToken>,
    error: Option<ServiceError>,
) -> Result<T, ServiceError> {
    if let Some(error) = error {
        return Err(error);
    }
    match result {
        Ok(value) => Ok(value),
        Err(error) => checked_core(Err(error), cancel),
    }
}
impl ApplicationService {
    pub fn unified_editor_begin(
        &self,
        document_id: &str,
        expected_revision: &str,
        expected_workspace_revision: &str,
        groups: Vec<SelectionGroup>,
        cancel: Option<&task::CancellationToken>,
    ) -> Result<UnifiedEditorSession, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        check_revision(record.workspace_revision, expected_workspace_revision)?;
        selection_edit::check_groups(&groups, MAX_MOVE_OBJECTS)?;
        for group in &groups {
            selection_edit::check_targets_with_cancel(record, group, cancel)?;
        }
        let draft = checked_core(
            record
                .history
                .begin_manufacturing_draft(&record.document, groups, &mut || {
                    core_checkpoint(cancel)
                }),
            cancel,
        )?;
        Ok(UnifiedEditorSession {
            owner: Arc::clone(&self.unified_editor_owner),
            document_id: document_id.into(),
            revision: expected_revision.into(),
            workspace_revision: record.workspace_revision,
            draft: Some(draft),
            pending: None,
            invalid_input: false,
        })
    }
    fn unified_editor_record(
        &self,
        session: &UnifiedEditorSession,
        cancel: Option<&task::CancellationToken>,
    ) -> Result<&S1DocumentRecord, ServiceError> {
        if !Arc::ptr_eq(&self.unified_editor_owner, &session.owner) {
            return Err(ServiceError::invalid("编辑会话不属于此服务"));
        }
        let draft = session.open()?;
        if let Some(c) = cancel {
            c.checkpoint()?;
        }
        let record = self
            .documents
            .get(&session.document_id)
            .ok_or_else(|| ServiceError::not_found("document", &session.document_id))?;
        check_revision(record.revision, &session.revision)?;
        check_revision(
            record.workspace_revision,
            &session.workspace_revision.to_string(),
        )?;
        // Workspace-only changes and precision changes fence the session too.
        for group in draft.groups() {
            selection_edit::check_targets_with_cancel(record, group, cancel)?;
        }
        Ok(record)
    }
    pub fn unified_editor_set_step(
        &self,
        session: &mut UnifiedEditorSession,
        generation: u64,
        step: DraftStep,
    ) -> Result<u64, ServiceError> {
        session.idle()?;
        if session.generation()? != generation {
            return Err(ServiceError::invalid("编辑输入版本已过期"));
        }
        let record = self.unified_editor_record(session, None)?;
        // A rejected newest input must never silently fall back to an older valid preview.
        let result = session
            .draft
            .as_mut()
            .unwrap()
            .set_step(&record.history, &record.document, generation, step)
            .map_err(map_edit_error);
        session.invalid_input = result.is_err();
        result
    }
    pub fn unified_editor_preview(
        &self,
        session: &mut UnifiedEditorSession,
        generation: u64,
        cancel: Option<&task::CancellationToken>,
    ) -> Result<(), ServiceError> {
        self.unified_editor_preview_prepared(session, generation, cancel, &mut |_| Ok(()))
    }
    pub fn unified_editor_preview_prepared(
        &self,
        session: &mut UnifiedEditorSession,
        generation: u64,
        cancel: Option<&task::CancellationToken>,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        session.idle()?;
        session.valid_input()?;
        if session.generation()? != generation {
            return Err(ServiceError::invalid("编辑输入版本已过期"));
        }
        let record = self.unified_editor_record(session, cancel)?;
        let mut publish_error = None;
        let mut prepare_error = None;
        let result = session.draft.as_mut().unwrap().preview_step_prepared(
            &record.history,
            &record.document,
            &mut || core_checkpoint(cancel),
            &mut |candidate| prepare_core(candidate, prepare, &mut prepare_error),
            &mut || publish_barrier(cancel, &mut publish_error),
        );
        published_core(result, cancel, prepare_error.or(publish_error))
    }
    pub fn unified_editor_execute(
        &self,
        session: &mut UnifiedEditorSession,
        generation: u64,
        cancel: Option<&task::CancellationToken>,
    ) -> Result<u64, ServiceError> {
        self.unified_editor_execute_prepared(session, generation, cancel, &mut |_| Ok(()))
    }
    pub fn unified_editor_execute_prepared(
        &self,
        session: &mut UnifiedEditorSession,
        generation: u64,
        cancel: Option<&task::CancellationToken>,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), ServiceError>,
    ) -> Result<u64, ServiceError> {
        session.idle()?;
        session.valid_input()?;
        if session.generation()? != generation {
            return Err(ServiceError::invalid("编辑输入版本已过期"));
        }
        let record = self.unified_editor_record(session, cancel)?;
        let mut publish_error = None;
        let mut prepare_error = None;
        let result = session.draft.as_mut().unwrap().execute_step_prepared(
            &record.history,
            &record.document,
            &mut || core_checkpoint(cancel),
            &mut |candidate| prepare_core(candidate, prepare, &mut prepare_error),
            &mut || publish_barrier(cancel, &mut publish_error),
        );
        published_core(result, cancel, prepare_error.or(publish_error))
    }
    pub fn unified_editor_undo(
        &self,
        session: &mut UnifiedEditorSession,
    ) -> Result<u64, ServiceError> {
        session.idle()?;
        self.unified_editor_record(session, None)?;
        let result = session
            .draft
            .as_mut()
            .unwrap()
            .undo()
            .map_err(map_edit_error);
        if result.is_ok() {
            session.invalid_input = false;
        }
        result
    }
    pub fn unified_editor_redo(
        &self,
        session: &mut UnifiedEditorSession,
    ) -> Result<u64, ServiceError> {
        session.idle()?;
        self.unified_editor_record(session, None)?;
        let result = session
            .draft
            .as_mut()
            .unwrap()
            .redo()
            .map_err(map_edit_error);
        if result.is_ok() {
            session.invalid_input = false;
        }
        result
    }
    /// Prepare exact prospective stored checkpoint before any local-history publication.
    pub fn unified_editor_history_prepared(
        &self,
        session: &mut UnifiedEditorSession,
        action: UnifiedEditorHistory,
        cancel: Option<&task::CancellationToken>,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), ServiceError>,
    ) -> Result<u64, ServiceError> {
        session.idle()?;
        if !Arc::ptr_eq(&self.unified_editor_owner, &session.owner) {
            return Err(ServiceError::invalid("编辑会话不属于此服务"));
        }
        if let Some(c) = cancel {
            c.checkpoint()?;
        }
        if action != UnifiedEditorHistory::Reset {
            self.unified_editor_record(session, cancel)?;
        }
        let mut prepare_error = None;
        let mut publish_error = None;
        let mut prepare =
            |candidate: DraftCandidate<'_>| prepare_core(candidate, prepare, &mut prepare_error);
        let mut publish = || {
            core_checkpoint(cancel)?;
            publish_barrier(cancel, &mut publish_error)
        };
        let draft = session.draft.as_mut().unwrap();
        let result = match action {
            UnifiedEditorHistory::Undo => draft.undo_prepared(&mut prepare, &mut publish),
            UnifiedEditorHistory::Redo => draft.redo_prepared(&mut prepare, &mut publish),
            UnifiedEditorHistory::Reset => draft.reset_prepared(&mut prepare, &mut publish),
        };
        let result = published_core(result, cancel, prepare_error.or(publish_error));
        if result.is_ok() {
            session.invalid_input = false;
        }
        result
    }
    pub fn unified_editor_begin_apply(
        &self,
        session: &mut UnifiedEditorSession,
    ) -> Result<UnifiedEditorApplyTicket, ServiceError> {
        session.idle()?;
        session.valid_input()?;
        self.unified_editor_record(session, None)?;
        let request = Arc::new(());
        let ticket = UnifiedEditorApplyTicket {
            owner: Arc::clone(&session.owner),
            request: Arc::clone(&request),
            generation: session.generation()?,
        };
        session.pending = Some(request);
        Ok(ticket)
    }
    pub fn unified_editor_complete_apply(
        &mut self,
        session: &mut UnifiedEditorSession,
        ticket: UnifiedEditorApplyTicket,
        cancel: Option<&task::CancellationToken>,
    ) -> Result<UnifiedEditorApplyResult, ServiceError> {
        self.unified_editor_complete_apply_prepared(session, ticket, cancel, &mut |_| Ok(()))
    }
    pub fn unified_editor_complete_apply_prepared(
        &mut self,
        session: &mut UnifiedEditorSession,
        ticket: UnifiedEditorApplyTicket,
        cancel: Option<&task::CancellationToken>,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), ServiceError>,
    ) -> Result<UnifiedEditorApplyResult, ServiceError> {
        if !Arc::ptr_eq(&self.unified_editor_owner, &ticket.owner)
            || !Arc::ptr_eq(&session.owner, &ticket.owner)
            || !session
                .pending
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &ticket.request))
            || session.generation()? != ticket.generation
        {
            return Err(ServiceError::invalid("最终应用请求已过期或不属于此会话"));
        }
        let result = (|| {
            self.unified_editor_record(session, cancel)?;
            session.valid_input()?;
            let record = self
                .documents
                .get_mut(&session.document_id)
                .ok_or_else(|| ServiceError::not_found("document", &session.document_id))?;
            let revision_exhausted = record.revision == u64::MAX;
            let draft = session.draft.as_ref().unwrap();
            let mut commit_error = None;
            let mut prepare_error = None;
            let ids = record.history.apply_manufacturing_draft_prepared(
                &mut record.document,
                draft,
                &mut || core_checkpoint(cancel),
                &mut |candidate| prepare_core(candidate, prepare, &mut prepare_error),
                &mut |changed| {
                    if changed && revision_exhausted {
                        commit_error =
                            Some(ServiceError::resource("revision", usize::MAX, usize::MAX));
                        return Err(EditError::ResourceLimit);
                    }
                    if let Some(c) = cancel
                        && let Err(error) = c.begin_commit()
                    {
                        commit_error = Some(error);
                        return Err(EditError::ResourceLimit);
                    }
                    Ok(())
                },
            );
            if let Some(error) = prepare_error.or(commit_error) {
                return Err(error);
            }
            let ids = checked_core(ids, cancel)?;
            let changed = !ids.is_empty();
            if changed {
                record.revision += 1;
            }
            Ok(UnifiedEditorApplyResult {
                changed,
                edit: edit_result(&session.document_id, record, ids, usize::from(changed)),
            })
        })();
        session.pending = None;
        if result.is_ok() {
            session.draft = None;
            session.invalid_input = false;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_barrier_decides_cancellation_before_any_draft_install() {
        let token = task::CancellationToken::default();
        token.start().unwrap();
        token.cancel();
        let mut error = None;
        assert!(publish_barrier(Some(&token), &mut error).is_err());
        assert_eq!(error.unwrap().code, "CANCELLED");
        let token = task::CancellationToken::default();
        token.start().unwrap();
        let mut error = None;
        publish_barrier(Some(&token), &mut error).unwrap();
        assert_eq!(token.cancel(), task::CancelOutcome::TooLate);
        assert_eq!(published_core(Ok(42), Some(&token), error).unwrap(), 42);
    }
}
