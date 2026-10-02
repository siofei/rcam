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

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryMetadata {
    pub project_id: String,
    pub original_project_path: Option<PathBuf>,
    pub autosave_unix_seconds: u64,
    pub app_version: String,
    pub source_project_hash: Option<String>,
    pub snapshot_hash: String,
    pub revision: String,
    pub workspace_revision: String,
}

pub(crate) fn directory() -> Option<PathBuf> {
    #[cfg(feature = "internal-evidence")]
    if let Some(dir) = crate::shortcut_store::native_directory()
        .or_else(crate::native_s5m1::directory)
        .or_else(crate::native_d2::directory)
        .or_else(crate::native_d1::directory)
    {
        return Some(dir.join("state/recovery"));
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches/RCam/recovery"))
}

fn base(dir: &Path, project_id: &str) -> PathBuf {
    dir.join(sha256_hex(project_id.as_bytes()))
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

pub(crate) fn write(
    dir: &Path,
    info: &editor_service::DocumentInfo,
    bytes: &[u8],
) -> io::Result<()> {
    rcam_diagnostics::with_source(rcam_diagnostics::Source::Recovery, || {
        let operation = rcam_diagnostics::Operation::begin_document(
            "recovery.write",
            &info.project_id,
            info.revision.parse().ok(),
        );
        let result = write_observed(dir, info, bytes);
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

fn write_observed(dir: &Path, info: &editor_service::DocumentInfo, bytes: &[u8]) -> io::Result<()> {
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
    let base = base(dir, &info.project_id);
    let metadata = RecoveryMetadata {
        project_id: info.project_id.clone(),
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
    // Snapshot first, metadata last: a crash never advertises incomplete data.
    write_atomic(&base.with_extension("rcam"), bytes)?;
    let json = serde_json::to_vec(&metadata)?;
    if json.len() as u64 > MAX_METADATA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery metadata too large",
        ));
    }
    write_atomic(&base.with_extension("json"), &json)
}

pub(crate) fn remove(dir: &Path, project_id: &str) {
    let base = base(dir, project_id);
    let _ = fs::remove_file(base.with_extension("json"));
    let _ = fs::remove_file(base.with_extension("rcam"));
}

pub(crate) fn discover(dir: &Path) -> Vec<RecoveryMetadata> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.take(100).flatten() {
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
        if base(dir, &meta.project_id).with_extension("json") != path {
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
    let path = base(dir, &meta.project_id).with_extension("rcam");
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
            }
        }
    }

    pub(crate) fn recovery_prompt(&mut self, ctx: &egui::Context) {
        let Some(candidate) = self.recovery_candidate.clone() else {
            return;
        };
        if self.recovery_prompt_reported.as_ref() != Some(&candidate.snapshot_hash) {
            event("recovery.prompt_shown");
            self.recovery_prompt_reported = Some(candidate.snapshot_hash.clone());
        }
        egui::Modal::new(egui::Id::new("project-recovery")).show(ctx, |ui| {
            ui.set_width(crate::ui::tokens::modal_width(ctx, 440., 180.));
            ui.heading("检测到未恢复的工程");
            ui.label("可将恢复副本作为未保存的工程打开，原工程不会被覆盖。");
            ui.horizontal(|ui| {
                if crate::ui::buttons::primary(ui, "打开恢复副本", !self.busy).clicked() {
                    event("recovery.restore_requested");
                    match directory().and_then(|dir| load(&dir, &candidate).ok()) {
                        Some(bytes) => {
                            self.recovery_candidate = None;
                            self.send(Action::RestoreProject(bytes));
                        }
                        None => {
                            event("recovery.restore_failed");
                            self.ui_error = Some("恢复快照损坏或无法读取".into());
                        }
                    }
                }
                if crate::ui::buttons::secondary_enabled(ui, "忽略", !self.busy).clicked() {
                    self.recovery_ignore_confirm = true;
                }
            });
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
                        remove(&dir, &candidate.project_id);
                        self.recovery_candidate = discover(&dir).into_iter().next();
                    } else {
                        self.recovery_candidate = None;
                    }
                    self.recovery_ignore_confirm = false;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_snapshot_is_rejected() {
        let dir = std::env::temp_dir().join(format!("rcam-recovery-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let meta = RecoveryMetadata {
            project_id: "test".into(),
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
        let _ = fs::remove_dir(&dir);
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
