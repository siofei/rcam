//! Bounded local recovery snapshots; never writes beside the project file.
use crate::{EditorApp, state::Action};
use editor_core::hash::sha256_hex;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn event(command: &'static str) {
    rcam_diagnostics::with_source(rcam_diagnostics::Source::Recovery, || {
        rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info, command)
    });
}
pub(crate) fn scheduled(retry: bool) {
    event(if retry {
        "recovery.write_retry"
    } else {
        "recovery.write_scheduled"
    });
}

const MAX_METADATA: u64 = 16 * 1024;
const MAX_SNAPSHOT: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryMetadata {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_generation: Option<String>,
    pub original_project_path: Option<PathBuf>,
    pub autosave_unix_seconds: u64,
    pub app_version: String,
    pub source_project_hash: Option<String>,
    pub snapshot_hash: String,
    pub revision: String,
    pub workspace_revision: String,
}

pub(crate) fn directory() -> Option<PathBuf> {
    if let Some(paths) = crate::startup_config::paths() {
        return Some(paths.recovery.clone());
    }
    #[cfg(feature = "internal-evidence")]
    if let Some(dir) = crate::shortcut_store::native_directory()
        .or_else(crate::native_s5m1::directory)
        .or_else(crate::native_a2::directory)
        .or_else(crate::native_batch_drag::directory)
        .or_else(crate::native_i1::directory)
        .or_else(crate::native_pmix::directory)
        .or_else(crate::native_d2::directory)
        .or_else(crate::native_d1::directory)
    {
        return Some(dir.join("state/recovery"));
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches/RCam/recovery"))
}

fn scoped_base(dir: &Path, project_id: &str, session_key: Option<&str>) -> PathBuf {
    let identity = session_key.map_or_else(
        || project_id.to_owned(),
        |key| format!("{project_id}:{key}"),
    );
    dir.join(sha256_hex(identity.as_bytes()))
}
#[cfg(test)]
fn base(dir: &Path, project_id: &str) -> PathBuf {
    scoped_base(dir, project_id, None)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)?;
    use io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)
}

pub(crate) fn write_scoped(
    dir: &Path,
    info: &editor_service::DocumentInfo,
    bytes: &[u8],
    session_key: Option<&str>,
) -> io::Result<()> {
    rcam_diagnostics::with_source(rcam_diagnostics::Source::Recovery, || {
        let operation = rcam_diagnostics::Operation::begin_document(
            "recovery.write",
            &info.project_id,
            info.revision.parse().ok(),
        );
        let result = write_observed(dir, info, bytes, session_key);
        operation.end(
            info.revision.parse().ok(),
            result.as_ref().err().map(|_| "RECOVERY_WRITE_FAILED"),
        );
        event(if result.is_ok() {
            "recovery.write_success"
        } else {
            "recovery.write_failed"
        });
        result
    })
}

fn write_observed(
    dir: &Path,
    info: &editor_service::DocumentInfo,
    bytes: &[u8],
    session_key: Option<&str>,
) -> io::Result<()> {
    if session_key.is_some_and(|key| key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid recovery identity",
        ));
    }
    if !info.project_dirty {
        return Ok(());
    }
    if bytes.len() as u64 > MAX_SNAPSHOT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery too large",
        ));
    }
    fs::create_dir_all(dir)?;
    let base = scoped_base(dir, &info.project_id, session_key);
    let metadata = RecoveryMetadata {
        project_id: info.project_id.clone(),
        session_key: session_key.map(str::to_owned),
        snapshot_generation: Some(sha256_hex(bytes)),
        original_project_path: info.project_path.as_ref().map(PathBuf::from),
        autosave_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        source_project_hash: info.last_saved_project_hash.clone(),
        snapshot_hash: sha256_hex(bytes),
        revision: info.revision.clone(),
        workspace_revision: info.workspace_revision.clone(),
    };
    let json = serde_json::to_vec(&metadata)?;
    if json.len() as u64 > MAX_METADATA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery metadata too large",
        ));
    }
    // Validate both budgets before any I/O. Each content-addressed snapshot is
    // immutable; failure before the metadata pointer commits keeps the old pair.
    let path = snapshot_path(dir, &metadata)?;
    let created = write_immutable(&path, bytes)?;
    let previous = read_metadata(&base.with_extension("json"))
        .ok()
        .filter(|old| {
            old.project_id == info.project_id && old.session_key.as_deref() == session_key
        });
    if let Err(error) = write_atomic(&base.with_extension("json"), &json) {
        if created
            && previous
                .as_ref()
                .and_then(|old| snapshot_path(dir, old).ok())
                .as_ref()
                != Some(&path)
        {
            let _ = fs::remove_file(&path);
        }
        return Err(error);
    }
    if let Some(old) = previous
        && let Ok(old_path) = snapshot_path(dir, &old)
        && old_path != path
    {
        let _ = fs::remove_file(old_path);
    }
    Ok(())
}

fn valid_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}
fn snapshot_path(dir: &Path, meta: &RecoveryMetadata) -> io::Result<PathBuf> {
    if meta
        .session_key
        .as_deref()
        .is_some_and(|key| !valid_key(key))
        || !valid_key(&meta.snapshot_hash)
        || meta
            .snapshot_generation
            .as_deref()
            .is_some_and(|key| key != meta.snapshot_hash)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid recovery identity",
        ));
    }
    let base = scoped_base(dir, &meta.project_id, meta.session_key.as_deref());
    Ok(match meta.snapshot_generation.as_deref() {
        Some(hash) => base.with_extension(format!("{hash}.rcam")),
        None => base.with_extension("rcam"), // Read old single-document snapshots.
    })
}
fn read_metadata(path: &Path) -> io::Result<RecoveryMetadata> {
    if fs::metadata(path)?.len() > MAX_METADATA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery metadata too large",
        ));
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
fn write_immutable(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    use io::Write;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            let result = file.write_all(bytes).and_then(|()| file.sync_all());
            drop(file);
            if result.is_err() {
                let _ = fs::remove_file(path);
            }
            result.map(|()| true)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            // A crash can leave a partial orphan. Never overwrite it or an old
            // record; the caller remains retryable without advertising bad data.
            if fs::metadata(path)?.len() == bytes.len() as u64 && fs::read(path)? == bytes {
                Ok(false)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "recovery snapshot conflict",
                ))
            }
        }
        Err(error) => Err(error),
    }
}
/// Persistent exclusive record claim. The owning Model retains the successful
/// claim across writes and UI switches. A restarted/colliding owner has no such
/// claim and fails closed, even if every clock/PID/namespace component repeats.
/// Keep the tiny marker as a tombstone after cleanup so identities never reuse.
pub(crate) fn reserve_record(dir: &Path, project_id: &str, key: &str) -> io::Result<()> {
    reserve_record_with(dir, project_id, key, fs::File::sync_all)
}
fn reserve_record_with(
    dir: &Path,
    project_id: &str,
    key: &str,
    sync: impl FnOnce(&fs::File) -> io::Result<()>,
) -> io::Result<()> {
    if !valid_key(key) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid recovery identity",
        ));
    }
    fs::create_dir_all(dir)?;
    let marker = scoped_base(dir, project_id, Some(key)).with_extension("owner");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)?;
    if let Err(error) = sync(&file) {
        drop(file);
        // Only the marker exclusively created by this call is removed. No
        // snapshot was published yet; an existing claim/tombstone is untouched.
        let _ = fs::remove_file(marker);
        return Err(error);
    }
    if scoped_base(dir, project_id, Some(key))
        .with_extension("json")
        .exists()
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "recovery record already exists",
        ));
    }
    Ok(())
}
pub(crate) fn remove_scoped(dir: &Path, project_id: &str, session_key: Option<&str>) {
    let base = scoped_base(dir, project_id, session_key);
    if let Ok(meta) = read_metadata(&base.with_extension("json"))
        && meta.project_id == project_id
        && meta.session_key.as_deref() == session_key
    {
        let _ = remove_candidate(dir, &meta);
    }
}
/// A prompt is an observed exact record, not permission to delete a newer one.
pub(crate) fn remove_candidate(dir: &Path, observed: &RecoveryMetadata) -> io::Result<()> {
    let base = scoped_base(dir, &observed.project_id, observed.session_key.as_deref());
    let current = read_metadata(&base.with_extension("json"))?;
    if serde_json::to_vec(&current)? != serde_json::to_vec(observed)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery record changed",
        ));
    }
    let path = snapshot_path(dir, &current)?;
    // Pointer deletion first: an interruption cannot advertise missing data.
    fs::remove_file(base.with_extension("json"))?;
    fs::remove_file(path)
}

pub(crate) fn discover(dir: &Path) -> Vec<RecoveryMetadata> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .take(100)
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(stat) = fs::metadata(&path) else {
            continue;
        };
        if stat.len() > MAX_METADATA {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<RecoveryMetadata>(&bytes) else {
            continue;
        };
        if meta
            .session_key
            .as_ref()
            .is_some_and(|key| key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()))
            || snapshot_path(dir, &meta).is_err()
            || scoped_base(dir, &meta.project_id, meta.session_key.as_deref())
                .with_extension("json")
                != path
        {
            continue;
        }
        let source_newer = meta
            .original_project_path
            .as_ref()
            .and_then(|source| fs::metadata(source).ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .is_some_and(|duration| duration.as_secs() > meta.autosave_unix_seconds);
        if !source_newer {
            found.push(meta);
        }
    }
    if !found.is_empty() {
        event("recovery.discovered");
    }
    found.sort_by_key(|m| std::cmp::Reverse(m.autosave_unix_seconds));
    found
}

pub(crate) fn load(dir: &Path, meta: &RecoveryMetadata) -> io::Result<Vec<u8>> {
    let path = snapshot_path(dir, meta)?;
    let stat = fs::metadata(&path)?;
    if stat.len() > MAX_SNAPSHOT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery too large",
        ));
    }
    let bytes = fs::read(path)?;
    if sha256_hex(&bytes) != meta.snapshot_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery checksum mismatch",
        ));
    }
    rcam_project::decode(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(bytes)
}

fn identity(info: &editor_service::DocumentInfo) -> String {
    format!(
        "{}:{}:{}",
        info.project_id, info.revision, info.workspace_revision
    )
}

fn reset_after_save(
    info: &editor_service::DocumentInfo,
    last_dirty: &mut String,
    last_recovered: &mut String,
) -> bool {
    if info.project_dirty {
        return false;
    }
    if !last_dirty.is_empty() || !last_recovered.is_empty() {
        event("recovery.cleaned_after_save");
    }
    last_dirty.clear();
    last_recovered.clear();
    true
}

pub(crate) fn complete_write(
    pending: &mut Option<String>,
    last_recovered: &mut String,
    succeeded: bool,
    current: Option<&editor_service::DocumentInfo>,
) {
    if let Some(identity) = pending.take()
        && succeeded
        && current.is_some_and(|info| info.project_dirty && self::identity(info) == identity)
    {
        *last_recovered = identity;
    }
}

impl EditorApp {
    pub(crate) fn tick_recovery(&mut self, now: std::time::Instant) {
        let Some(info) = self.view.info.as_ref() else {
            return;
        };
        if reset_after_save(
            info,
            &mut self.last_dirty_identity,
            &mut self.last_recovered_identity,
        ) {
            return;
        }
        let identity = identity(info);
        if self.last_dirty_identity != identity {
            self.last_dirty_identity = identity.clone();
            self.dirty_since = now;
        }
        if self.busy
            || self.unified_editor.is_some()
            || self.canvas_read.is_some()
            || self.drag.is_some()
            || self.grip.is_some()
            || self.close_prompt
            || self.recovery_candidate.is_some()
            || self.modal.is_some()
        {
            return;
        }
        if identity == self.last_recovered_identity
            || now.duration_since(self.dirty_since).as_secs() < 30
            || now.duration_since(self.last_recovery_at).as_secs() < 60
        {
            return;
        }
        if let Some(dir) = directory() {
            scheduled(self.recovery_attempted_identity.as_ref() == Some(&identity));
            self.recovery_attempted_identity = Some(identity.clone());
            self.send(Action::RecoveryWrite(dir));
            if self.busy {
                self.last_recovery_at = now;
                self.pending_recovery_identity = Some(identity);
                self.pending_recovery_task = self
                    .pending_task
                    .as_ref()
                    .map(|t| (self.routing.owner(), t.task_id));
            }
        }
    }

    pub(crate) fn recovery_prompt(&mut self, ctx: &egui::Context) {
        if self.restore_pending() || self.project_error.is_some() {
            return;
        }
        let Some(candidate) = self.recovery_candidate.clone() else {
            return;
        };
        if self.recovery_prompt_reported.as_ref() != Some(&candidate.snapshot_hash) {
            event("recovery.prompt_shown");
            self.recovery_prompt_reported = Some(candidate.snapshot_hash.clone());
        }
        crate::ui::modal_widgets::fixed_modal(
            ctx,
            egui::Id::new("project-recovery"),
            egui::vec2(440., 300.),
            |ui| {
                ui.heading("检测到未恢复的工程");
                ui.label("可将恢复副本作为未保存的工程打开，原工程不会被覆盖。");
                ui.horizontal(|ui| {
                    if crate::ui::buttons::primary(ui, "打开恢复副本", !self.busy).clicked() {
                        event("recovery.restore_requested");
                        if let Some(dir) = directory() {
                            self.request_restore(dir, candidate.clone());
                        } else {
                            self.ui_error = Some("恢复目录不可用；原记录已保留".into());
                        }
                    }
                    if crate::ui::buttons::secondary_enabled(ui, "稍后", !self.busy).clicked() {
                        // Keep the exact disk record. The tabs header reopens the
                        // recovery directory after the user has freed a slot.
                        self.recovery_candidate = None;
                        self.recovery_ignore_confirm = false;
                        self.recovery_prompt_reported = None;
                    }
                    if crate::ui::buttons::secondary_enabled(ui, "忽略", !self.busy).clicked() {
                        self.recovery_ignore_confirm = true;
                    }
                });
                if let Some(error) = &self.ui_error {
                    crate::ui::modal_widgets::status_slot(ui, error, 48., true);
                }
                if self.recovery_ignore_confirm {
                    ui.label("忽略将删除这个恢复副本。");
                    let (cancel, delete) = crate::ui::modal_widgets::cancel_destructive_row(
                        ui,
                        "删除恢复副本",
                        !self.busy,
                    );
                    if cancel {
                        self.recovery_ignore_confirm = false;
                    }
                    if delete {
                        event("recovery.dismissed");
                        if let Some(dir) = directory() {
                            if remove_candidate(&dir, &candidate).is_ok() {
                                self.refresh_recovery_candidate();
                            } else {
                                self.ui_error = Some("恢复记录已变更或无法删除，请重新检查".into());
                            }
                        } else {
                            self.recovery_candidate = None;
                        }
                        self.recovery_ignore_confirm = false;
                    }
                }
            },
        );
    }
}

#[cfg(test)]
pub(crate) fn write(
    dir: &Path,
    info: &editor_service::DocumentInfo,
    bytes: &[u8],
) -> io::Result<()> {
    write_scoped(dir, info, bytes, None)
}
#[cfg(test)]
pub(crate) fn remove(dir: &Path, project_id: &str) {
    remove_scoped(dir, project_id, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_update_preserves_previous_pointer_snapshot_and_exact_ignore_fence() {
        let dir =
            std::env::temp_dir().join(format!("rcam-recovery-transaction-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut model = crate::state::Model::default();
        model.run(Action::NewWorkspace);
        model.run(Action::CreateEmptyLayer(Some("A".into())));
        let info = model.view.info.clone().unwrap();
        let bytes = model
            .service
            .project_recovery_bytes(&info.document_id)
            .unwrap();
        let key = sha256_hex(b"transaction owner");
        reserve_record(&dir, &info.project_id, &key).unwrap();
        write_scoped(&dir, &info, &bytes, Some(&key)).unwrap();
        let old = discover(&dir).pop().unwrap();
        let pointer = scoped_base(&dir, &info.project_id, Some(&key)).with_extension("json");
        let old_pointer = fs::read(&pointer).unwrap();
        model.run(Action::CreateEmptyLayer(Some("B".into())));
        let mut newer = model.view.info.clone().unwrap();
        let next = model
            .service
            .project_recovery_bytes(&newer.document_id)
            .unwrap();
        newer.project_path = Some("x".repeat(MAX_METADATA as usize));
        assert!(write_scoped(&dir, &newer, &next, Some(&key)).is_err());
        assert_eq!(fs::read(&pointer).unwrap(), old_pointer);
        assert_eq!(load(&dir, &old).unwrap(), bytes);
        newer.project_path = None;
        fs::create_dir(pointer.with_extension("tmp")).unwrap();
        assert!(write_scoped(&dir, &info, &bytes, Some(&key)).is_err());
        assert_eq!(load(&dir, &old).unwrap(), bytes);
        assert!(write_scoped(&dir, &newer, &next, Some(&key)).is_err());
        assert_eq!(fs::read(&pointer).unwrap(), old_pointer);
        assert_eq!(load(&dir, &old).unwrap(), bytes);
        fs::remove_dir(pointer.with_extension("tmp")).unwrap();
        write_scoped(&dir, &newer, &next, Some(&key)).unwrap();
        assert!(
            remove_candidate(&dir, &old).is_err(),
            "stale prompt cannot delete newer record"
        );
        let current = discover(&dir).pop().unwrap();
        assert_eq!(load(&dir, &current).unwrap(), next);
        remove_candidate(&dir, &current).unwrap();
        assert!(discover(&dir).is_empty());
        assert!(
            reserve_record(&dir, &info.project_id, &key).is_err(),
            "closed identity remains reserved"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_new_marker_sync_is_retryable_without_deleting_an_existing_claim() {
        let dir =
            std::env::temp_dir().join(format!("rcam-recovery-marker-sync-{}", std::process::id()));
        let key = sha256_hex(b"sync failure owner");
        assert!(
            reserve_record_with(&dir, "project", &key, |_| Err(io::Error::other(
                "injected sync"
            )))
            .is_err()
        );
        assert!(
            !scoped_base(&dir, "project", Some(&key))
                .with_extension("owner")
                .exists()
        );
        reserve_record(&dir, "project", &key).unwrap();
        assert!(
            reserve_record_with(&dir, "project", &key, |_| panic!(
                "existing marker never enters sync"
            ))
            .is_err()
        );
        assert!(
            scoped_base(&dir, "project", Some(&key))
                .with_extension("owner")
                .exists()
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn persisted_record_claim_fails_closed_on_restart_and_markers_do_not_hide_candidates() {
        let dir =
            std::env::temp_dir().join(format!("rcam-recovery-reservation-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut model = crate::state::Model::default();
        model.run(Action::NewWorkspace);
        model.run(Action::CreateEmptyLayer(Some("A".into())));
        let key = sha256_hex(b"forced identical restarted namespace");
        model.recovery_key = Some(key.clone());
        model.run(Action::RecoveryWrite(dir.clone()));
        assert!(model.view.error.is_none());
        let old = discover(&dir).pop().unwrap();
        let bytes = load(&dir, &old).unwrap();
        model.run(Action::CreateEmptyLayer(Some("B".into())));
        model.run(Action::RecoveryWrite(dir.clone()));
        assert!(
            model.view.error.is_none(),
            "owner can update its claimed record"
        );
        let current = discover(&dir).pop().unwrap();
        let updated = load(&dir, &current).unwrap();
        assert_ne!(bytes, updated);
        let mut restarted = crate::state::Model::default();
        restarted.run(Action::RestoreProject(updated.clone()));
        restarted.recovery_key = Some(key.clone());
        restarted.run(Action::RecoveryWrite(dir.clone()));
        assert!(restarted.view.error.is_some());
        assert_eq!(load(&dir, &current).unwrap(), updated);
        for index in 0..120 {
            reserve_record(
                &dir,
                &old.project_id,
                &sha256_hex(format!("marker-{index}").as_bytes()),
            )
            .unwrap();
        }
        assert_eq!(
            discover(&dir).len(),
            1,
            "non-metadata markers do not consume discovery quota"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn same_project_sessions_have_distinct_persisted_recovery_and_exact_cleanup() {
        let root =
            std::env::temp_dir().join(format!("rcam-scoped-recovery-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut model = crate::state::Model::default();
        model.run(Action::NewWorkspace);
        model.run(Action::CreateEmptyLayer(Some("synthetic".into())));
        let info = model.view.info.clone().unwrap();
        assert!(info.project_dirty);
        let bytes = model
            .service
            .project_recovery_bytes(&info.document_id)
            .unwrap();
        let a = sha256_hex(b"window-1:slot-1:generation-1");
        let b = sha256_hex(b"window-1:slot-2:generation-1");
        write_scoped(&root, &info, &bytes, Some(&a)).unwrap();
        write_scoped(&root, &info, &bytes, Some(&b)).unwrap();
        write(&root, &info, &bytes).unwrap();
        let records = discover(&root);
        assert_eq!(records.len(), 3);
        assert!(records.iter().all(|r| r.project_id == info.project_id));
        for record in &records {
            assert_eq!(load(&root, record).unwrap(), bytes);
        }
        remove_scoped(&root, &info.project_id, Some(&a));
        let after = discover(&root);
        assert_eq!(after.len(), 2);
        assert!(after.iter().any(|r| r.session_key.as_deref() == Some(&b)));
        assert!(after.iter().any(|r| r.session_key.is_none()));
        assert!(write_scoped(&root, &info, &bytes, Some("../invalid")).is_err());
        assert_eq!(discover(&root).len(), 2);
        let persisted = after
            .iter()
            .find(|r| r.session_key.as_deref() == Some(&b))
            .unwrap();
        let restored = model
            .service
            .project_restore(&load(&root, persisted).unwrap())
            .unwrap();
        assert!(restored.project_dirty && restored.project_path.is_none());
        assert_eq!(restored.project_id, info.project_id);
        assert_eq!(discover(&root).len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn corrupt_snapshot_is_rejected() {
        let dir = std::env::temp_dir().join(format!("rcam-recovery-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let meta = RecoveryMetadata {
            project_id: "test".into(),
            session_key: None,
            snapshot_generation: None,
            original_project_path: None,
            autosave_unix_seconds: 1,
            app_version: "0".into(),
            source_project_hash: None,
            snapshot_hash: sha256_hex(b"bad"),
            revision: "1".into(),
            workspace_revision: "0".into(),
        };
        fs::write(base(&dir, "test").with_extension("rcam"), b"bad").unwrap();
        assert!(load(&dir, &meta).is_err());
        remove(&dir, "test");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dirty_snapshot_is_discovered_and_restored_without_touching_project() {
        use editor_service::{ApplicationService, CreateEmptyLayerParams, FileAccessPolicy};
        let log_dir =
            std::env::temp_dir().join(format!("rcam-recovery-events-{}", std::process::id()));
        let guard = rcam_diagnostics::Runtime::start(log_dir.clone(), "test", "test").unwrap();
        assert!(guard.install_sink());
        let dir = std::env::temp_dir().join(format!("rcam-recovery-flow-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let root = dir.canonicalize().unwrap();
        let mut svc = ApplicationService::with_file_access(FileAccessPolicy::new(
            &root,
            [root.clone()],
            [root.clone()],
        ));
        let doc = svc.document_new().unwrap();
        let saved_path = root.join("project.rcam");
        svc.project_save(
            &doc.document_id,
            &doc.revision,
            Some(saved_path.to_str().unwrap()),
            false,
        )
        .unwrap();
        let original = fs::read(&saved_path).unwrap();
        svc.create_empty_layer(
            &doc.document_id,
            &doc.revision,
            CreateEmptyLayerParams::default(),
        )
        .unwrap();
        let info = svc.document_get(&doc.document_id).unwrap();
        let bytes = svc.project_recovery_bytes(&doc.document_id).unwrap();
        let blocked_dir = root.join("blocked-cache");
        fs::write(&blocked_dir, b"not a directory").unwrap();
        let mut pending = Some(identity(&info));
        let mut last_recovered = String::new();
        assert!(write(&blocked_dir, &info, &bytes).is_err());
        complete_write(&mut pending, &mut last_recovered, false, Some(&info));
        assert!(pending.is_none());
        assert!(
            last_recovered.is_empty(),
            "failed write must remain retryable"
        );
        let recovery_dir = root.join("cache");
        pending = Some(identity(&info));
        scheduled(true);
        write(&recovery_dir, &info, &bytes).unwrap();
        complete_write(&mut pending, &mut last_recovered, true, Some(&info));
        assert_eq!(last_recovered, identity(&info));
        assert_eq!(fs::read(&saved_path).unwrap(), original);
        let candidates = discover(&recovery_dir);
        assert_eq!(candidates.len(), 1);
        let mut last_dirty = identity(&info);
        let saved = svc
            .project_save(&doc.document_id, &info.revision, None, false)
            .unwrap();
        let current_saved = fs::read(&saved_path).unwrap();
        assert!(reset_after_save(
            &saved,
            &mut last_dirty,
            &mut last_recovered
        ));
        assert!(last_dirty.is_empty());
        assert!(last_recovered.is_empty());
        let restored = svc
            .project_restore(&load(&recovery_dir, &candidates[0]).unwrap())
            .unwrap();
        assert!(restored.project_dirty);
        assert!(restored.project_path.is_none());
        assert_eq!(fs::read(&saved_path).unwrap(), current_saved);
        remove(&recovery_dir, &info.project_id);
        assert!(discover(&recovery_dir).is_empty());
        assert!(guard.runtime().flush());
        let events = fs::read_to_string(log_dir.join("rcam.log")).unwrap();
        let failed = events.find("recovery.write_failed").unwrap();
        let retry = events.find("recovery.write_retry").unwrap();
        let success = events.find("recovery.write_success").unwrap();
        assert!(failed < retry && retry < success);
        println!(
            "recovery failure -> retry -> success; failed write unmarked; original unchanged; restored dirty"
        );
        drop(guard);
        fs::remove_dir_all(log_dir).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
