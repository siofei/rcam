//! Native project session and safe `.rcam` file boundary (S4-B3).
use super::*;
use rcam_project::{
    DisplayUnit, FORMAT_VERSION, GridSettings, LayerProjectState, ManufacturingProjectSettings,
    ProjectId, RCamProject, SnapSettingsState, WorkspaceProjectState,
};
use std::io::Write;

pub(crate) fn default_workspace_settings() -> WorkspaceProjectState {
    WorkspaceProjectState {
        display_unit: DisplayUnit::Millimeters,
        grid: GridSettings {
            spacing_mm: 0.1,
            visible: false,
            snap: false,
        },
        snap: SnapSettingsState {
            enabled: false,
            enabled_kinds: vec![
                editor_core::snap::SnapKind::Endpoint,
                editor_core::snap::SnapKind::Vertex,
                editor_core::snap::SnapKind::Midpoint,
                editor_core::snap::SnapKind::Center,
                editor_core::snap::SnapKind::Quadrant,
                editor_core::snap::SnapKind::Intersection,
            ],
            radius_px: 8.0,
            manufacturing_boundary: true,
            original_path: false,
        },
        active_layer_id: None,
        camera: None,
    }
}

fn project_error(error: rcam_project::ProjectError) -> ServiceError {
    ServiceError {
        code: match error {
            rcam_project::ProjectError::UnknownFormatVersion(_)
            | rcam_project::ProjectError::UnsupportedFeature(_) => "UNSUPPORTED_FEATURE",
            rcam_project::ProjectError::ResourceLimit { .. } => "RESOURCE_LIMIT",
            _ => "VALIDATION_FAILED",
        }
        .into(),
        message: error.to_string(),
        details: serde_json::json!({}),
    }
}

impl S1DocumentRecord {
    fn project_snapshot(&self) -> RCamProject {
        let mut workspace = self.project_settings.clone();
        workspace.active_layer_id = self.active_layer_id.clone();
        RCamProject {
            format_version: FORMAT_VERSION,
            project_id: ProjectId(self.project_id.clone()),
            manufacturing: ManufacturingProjectSettings {
                precision: self.manufacturing_precision,
            },
            workspace,
            layer_order: self.display_order.clone(),
            // The codec rebuilds layers in layer_order. Match that order in
            // the pre-save snapshot so importing above an existing layer
            // cannot fail the exact project round-trip check.
            layers: self
                .display_order
                .iter()
                .map(|id| {
                    let layer = self
                        .document
                        .layers
                        .iter()
                        .find(|layer| &layer.id == id)
                        .expect("display layer exists in document");
                    LayerProjectState {
                        layer: layer.clone(),
                        workspace: self
                            .workspace
                            .get(&layer.id)
                            .cloned()
                            .expect("live layer has workspace state"),
                        provenance: self
                            .sources
                            .get(&layer.id)
                            .and_then(|source| source.provenance.clone()),
                        compatibility_issues: self
                            .sources
                            .get(&layer.id)
                            .map(|source| source.metadata.compatibility_issues.clone())
                            .unwrap_or_default(),
                    }
                })
                .collect(),
            apertures: self.document.apertures.clone(),
            block_definitions: self.document.block_definitions.clone(),
            board: None,
        }
    }

    pub(crate) fn project_state_hash(&self) -> Result<String, ServiceError> {
        let mut settings = self.project_settings.clone();
        settings.active_layer_id = None;
        settings.camera = None;
        let styles: Vec<_> = self
            .display_order
            .iter()
            .map(|id| {
                (
                    id,
                    self.workspace.get(id),
                    self.sources
                        .get(id)
                        .and_then(|source| source.provenance.as_ref()),
                )
            })
            .collect();
        let json = serde_json::to_vec(&(
            &self.project_id,
            self.manufacturing_precision,
            &settings,
            &styles,
        ))
        .map_err(serialize_error)?;
        Ok(sha256_hex(&json))
    }

    pub(crate) fn is_project_dirty(&self) -> bool {
        self.is_dirty()
            || self
                .project_state_hash()
                .map_or(true, |hash| hash != self.saved_project_state_hash)
    }
}

fn require_rcam(path: &Path) -> Result<(), ServiceError> {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rcam"))
    {
        Ok(())
    } else {
        Err(ServiceError::invalid_field(
            "params.path",
            "native project path must end in .rcam",
        ))
    }
}

fn atomic_project_write(
    path: &Path,
    bytes: &[u8],
    expected: Option<&str>,
    allow_replace: bool,
) -> Result<(), ServiceError> {
    atomic_project_write_with_hook(path, bytes, expected, allow_replace, || Ok(()))
}

fn atomic_project_write_with_hook(
    path: &Path,
    bytes: &[u8],
    expected: Option<&str>,
    allow_replace: bool,
    before_publish: impl FnOnce() -> Result<(), ServiceError>,
) -> Result<(), ServiceError> {
    let replacing = path.exists();
    if expected.is_some() && !replacing {
        return Err(ServiceError {
            code: "EXTERNAL_MODIFICATION".into(),
            message: "saved project disappeared from disk".into(),
            details: serde_json::json!({}),
        });
    }
    if replacing {
        if !allow_replace {
            return Err(ServiceError {
                code: "CONFIRMATION_REQUIRED".into(),
                message: "existing project requires replace confirmation".into(),
                details: serde_json::json!({}),
            });
        }
        if let Some(expected) = expected {
            let old = fs::read(path).map_err(|e| ServiceError::io("read", path, e))?;
            if sha256_hex(&old) != expected {
                return Err(ServiceError {
                    code: "EXTERNAL_MODIFICATION".into(),
                    message: "project changed on disk since last save/open".into(),
                    details: serde_json::json!({}),
                });
            }
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::invalid("project has no parent directory"))?;
    let filename = path
        .file_name()
        .ok_or_else(|| ServiceError::invalid("project has no file name"))?
        .to_string_lossy();
    let temp = parent.join(format!(
        ".{filename}.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| ServiceError::io("create temp", &temp, e))?;
        file.write_all(bytes)
            .map_err(|e| ServiceError::io("write temp", &temp, e))?;
        file.flush()
            .map_err(|e| ServiceError::io("flush temp", &temp, e))?;
        file.sync_all()
            .map_err(|e| ServiceError::io("sync temp", &temp, e))?;
        drop(file);
        let persisted = fs::read(&temp).map_err(|e| ServiceError::io("verify temp", &temp, e))?;
        if persisted != bytes {
            return Err(ServiceError::invalid("persisted project bytes differ"));
        }
        rcam_project::decode(&persisted).map_err(project_error)?;
        before_publish()?;
        if replacing {
            if let Some(expected) = expected {
                let current = fs::read(path).map_err(|e| ServiceError::io("read", path, e))?;
                if sha256_hex(&current) != expected {
                    return Err(ServiceError {
                        code: "EXTERNAL_MODIFICATION".into(),
                        message: "project changed before replacement".into(),
                        details: serde_json::json!({}),
                    });
                }
            }
            fs::rename(&temp, path).map_err(|e| ServiceError::io("replace project", path, e))?;
        } else {
            fs::hard_link(&temp, path).map_err(|e| {
                ServiceError::io("publish new project without replacement", path, e)
            })?;
            let _ = fs::remove_file(&temp);
        }
        let final_bytes =
            fs::read(path).map_err(|e| ServiceError::io("verify project", path, e))?;
        if final_bytes != bytes {
            return Err(ServiceError::invalid("final project bytes differ"));
        }
        rcam_project::decode(&final_bytes).map_err(project_error)?;
        // Directory sync is advisory on platforms/filesystems that permit it.
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

impl ApplicationService {
    /// Read a project through the host policy, decode it fully, and insert a
    /// candidate session only after every validation succeeds. The caller can
    /// still reject the candidate without touching its existing session.
    pub fn project_open(&mut self, path: &str) -> Result<DocumentInfo, ServiceError> {
        require_rcam(Path::new(path))?;
        let access = self
            .file_access
            .as_ref()
            .ok_or_else(|| ServiceError::permission(Path::new(path), "read"))?;
        let (canonical, bytes) = access.read_path_limited(path, 512 * 1024 * 1024)?;
        let project = rcam_project::decode(&bytes).map_err(project_error)?;
        self.insert_project(project, Some(canonical), Some(sha256_hex(&bytes)), false)
    }

    /// Recovery opens as a dirty, unsaved copy. It never adopts the original
    /// project path or writes to it without an explicit later Save As.
    pub fn project_restore(&mut self, bytes: &[u8]) -> Result<DocumentInfo, ServiceError> {
        let project = rcam_project::decode(bytes).map_err(project_error)?;
        self.insert_project(project, None, None, true)
    }

    fn insert_project(
        &mut self,
        project: RCamProject,
        path: Option<PathBuf>,
        saved_hash: Option<String>,
        recovered: bool,
    ) -> Result<DocumentInfo, ServiceError> {
        let document_id = self.allocate_document_id()?;
        let mut document = project.to_semantic_document();
        document.id = document_id.clone();
        let mut record = self.new_record(document)?;
        record.project_id = project.project_id.0;
        record.project_path = path;
        record.last_saved_project_hash = saved_hash;
        record.manufacturing_precision = project.manufacturing.precision;
        record.saved_precision = record.manufacturing_precision;
        record.project_settings = project.workspace;
        record.active_layer_id = record.project_settings.active_layer_id.clone();
        record.display_order = project.layer_order;
        for layer in project.layers {
            let id = layer.layer.id;
            record.workspace.insert(id.clone(), layer.workspace);
            let mut source = workspace::LayerSource::empty();
            source.provenance = layer.provenance;
            source.metadata.compatibility_issues = layer.compatibility_issues;
            source.diagnostics = source.metadata.compatibility_issues.clone();
            record.sources.insert(id, source);
        }
        record.next_layer_number = record
            .document
            .layers
            .iter()
            .filter_map(|l| l.id.strip_prefix("layer-")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let provenance_max = record
            .sources
            .values()
            .filter_map(|source| source.provenance.as_ref())
            .filter_map(|p| p.import_id.strip_prefix("import-")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        let aperture_max = record
            .document
            .apertures
            .iter()
            .filter_map(|aperture| {
                aperture
                    .id
                    .strip_prefix("src-")?
                    .split_once("::")?
                    .0
                    .parse::<u64>()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        record.next_source_number = provenance_max
            .max(aperture_max)
            .max(record.sources.len() as u64)
            .saturating_add(1);
        record.next_color_index = record.workspace.len();
        record.saved_content_hash = content_hash(&record.document);
        record.saved_project_state_hash = if recovered {
            String::new()
        } else {
            record.project_state_hash()?
        };
        let info = document_info(&document_id, &record);
        self.documents.insert(document_id, record);
        Ok(info)
    }

    pub fn project_snapshot(&self, document_id: &str) -> Result<RCamProject, ServiceError> {
        self.documents
            .get(document_id)
            .map(S1DocumentRecord::project_snapshot)
            .ok_or_else(|| ServiceError::not_found("document", document_id))
    }

    pub fn project_recovery_bytes(&self, document_id: &str) -> Result<Vec<u8>, ServiceError> {
        let project = self.project_snapshot(document_id)?;
        rcam_project::encode_v1(&project).map_err(project_error)
    }

    pub fn project_workspace(
        &self,
        document_id: &str,
    ) -> Result<WorkspaceProjectState, ServiceError> {
        Ok(self.project_snapshot(document_id)?.workspace)
    }

    pub fn project_set_workspace(
        &mut self,
        document_id: &str,
        settings: WorkspaceProjectState,
    ) -> Result<DocumentInfo, ServiceError> {
        let record = self
            .documents
            .get_mut(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        let changed = record.project_settings != settings;
        let next_revision = if changed {
            Some(workspace::next_workspace_revision(record)?)
        } else {
            None
        };
        let old = std::mem::replace(&mut record.project_settings, settings);
        if let Err(error) = record.project_snapshot().validate() {
            record.project_settings = old;
            return Err(project_error(error));
        }
        if let Some(next_revision) = next_revision {
            record.workspace_revision = next_revision;
        }
        Ok(document_info(document_id, record))
    }

    pub fn project_save(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        path: Option<&str>,
        allow_replace: bool,
    ) -> Result<DocumentInfo, ServiceError> {
        self.project_save_with_camera(document_id, expected_revision, path, allow_replace, None)
    }

    pub fn project_save_with_camera(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        path: Option<&str>,
        allow_replace: bool,
        camera: Option<rcam_project::CameraState>,
    ) -> Result<DocumentInfo, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        let requested = path
            .map(str::to_owned)
            .or_else(|| {
                record
                    .project_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .ok_or_else(|| {
                ServiceError::invalid_field("params.path", "unsaved project requires Save As path")
            })?;
        require_rcam(Path::new(&requested))?;
        let access = self
            .file_access
            .as_ref()
            .ok_or_else(|| ServiceError::permission(Path::new(&requested), "write"))?;
        let target = access.write_path(&requested)?;
        let same = record.project_path.as_ref().is_some_and(|p| p == &target);
        let expected = if same {
            record.last_saved_project_hash.as_deref()
        } else {
            None
        };
        let mut project = record.project_snapshot();
        if let Some(camera) = camera {
            project.workspace.camera = Some(camera);
        }
        let bytes = rcam_project::encode_v1(&project).map_err(project_error)?;
        if rcam_project::decode(&bytes).map_err(project_error)? != project {
            return Err(ServiceError::invalid("project roundtrip mismatch"));
        }
        atomic_project_write(&target, &bytes, expected, allow_replace || same)?;
        let record = self
            .documents
            .get_mut(document_id)
            .expect("record exists after write");
        record.project_settings.camera = project.workspace.camera;
        record.project_path = Some(target);
        record.last_saved_project_hash = Some(sha256_hex(&bytes));
        record.saved_project_state_hash = record.project_state_hash()?;
        record.saved_content_hash = content_hash(&record.document);
        record.saved_precision = record.manufacturing_precision;
        Ok(document_info(document_id, record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_validation_and_publish_leave_original_intact() {
        let dir = std::env::temp_dir().join(format!("rcam-atomic-fault-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("original.rcam");
        fs::write(&path, b"original").unwrap();
        assert!(atomic_project_write(&path, b"not a project", None, true).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        let bytes = fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/s4b2/sample.rcam"
        ))
        .unwrap();
        assert!(
            atomic_project_write_with_hook(&path, &bytes, None, true, || Err(
                ServiceError::invalid("injected rename failure")
            ))
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
