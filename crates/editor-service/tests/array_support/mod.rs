#![allow(dead_code)]
use editor_service::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Run {
    pub service: ApplicationService,
    pub dir: PathBuf,
    pub document: String,
    pub layer: String,
    pub source: Vec<u8>,
}
impl Run {
    pub fn new(count: usize) -> Self {
        let mut source = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.8*%\nD10*\n");
        for n in 0..count {
            source += &format!("X{}Y{}D03*\n", (n % 20) * 2_000_000, (n / 20) * 2_000_000);
        }
        source += "M02*\n";
        Self::from_bytes(source.as_bytes())
    }
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-array-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.gbr"), bytes).unwrap();
        let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
            dir.clone(),
            [dir.clone()],
            [dir.clone()],
        ));
        let opened = service.open("source.gbr").unwrap();
        Self {
            service,
            dir,
            document: opened.document_id,
            layer: opened.layer_ids[0].clone(),
            source: bytes.into(),
        }
    }
    pub fn info(&self) -> DocumentInfo {
        self.service.document_get(&self.document).unwrap()
    }
    pub fn snapshot(&self) -> RenderSnapshot {
        self.service.render_snapshot(&self.document).unwrap()
    }
    pub fn ids(&self) -> Vec<String> {
        self.snapshot().layers[0]
            .objects
            .iter()
            .map(|o| o.object_id.clone())
            .collect()
    }
    pub fn params(&self, rows: u64, columns: u64) -> ArrayRectangularParams {
        ArrayRectangularParams {
            layer_id: self.layer.clone(),
            object_ids: self.ids(),
            rows,
            columns,
            pitch_x_mm: 50.,
            pitch_y_mm: 50.,
        }
    }
    pub fn array(&mut self, p: ArrayRectangularParams) -> Result<EditResult, ServiceError> {
        self.service
            .objects_array_rectangular(&self.document, &self.info().revision, p)
    }
    pub fn block(&mut self) {
        let p = CreateBlockDefinitionParams {
            layer_id: self.layer.clone(),
            object_ids: self.ids(),
            local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
            name: "Synthetic 400".into(),
        };
        self.service
            .blocks_create_definition_from_objects(&self.document, &self.info().revision, p)
            .unwrap();
    }
    pub fn export(&mut self, name: &str) -> RenderSnapshot {
        let path = self.dir.join(name);
        self.service
            .export_layer(
                &self.document,
                &self.info().revision,
                ExportParams {
                    layer_id: self.layer.clone(),
                    path: path.to_string_lossy().into(),
                    overwrite: OverwritePolicy {
                        mode: "deny".into(),
                        expected_sha256: None,
                    },
                    metadata_policy: MetadataPolicy {
                        mode: "require_confirmation".into(),
                        categories: None,
                    },
                    compatibility_precision_override_mm: None,
                },
            )
            .unwrap();
        let opened = self.service.open(path.to_str().unwrap()).unwrap();
        self.service.render_snapshot(&opened.document_id).unwrap()
    }
    pub fn patch(&mut self, patch: LayerPatch) {
        let i = self.info();
        self.service
            .layers_update_many(
                &self.document,
                &i.revision,
                UpdateLayersParams {
                    expected_workspace_revision: i.workspace_revision,
                    updates: vec![patch],
                },
            )
            .unwrap();
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
