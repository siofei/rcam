//! Real import/service/history and cancellation for the host-only AP foundation.
use editor_service::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Run {
    s: ApplicationService,
    id: String,
    dir: PathBuf,
}
impl Drop for Run {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}
impl Run {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-ap-draft-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = ApplicationService::with_file_access(FileAccessPolicy::new(
            dir.clone(),
            [dir.clone()],
            [dir.clone()],
        ));
        let info = s.document_new().unwrap();
        let id = info.document_id;
        let paths=(0..2).map(|i|{let p=dir.join(format!("{i}.gbr"));std::fs::write(&p,b"%FSLAX46Y46*%%MOMM*%%ADD10R,2X3X0.5*%D10*X0Y0D03*X5000000Y0D03*%LPC*%X10000000Y0D03*M02*").unwrap();p.to_string_lossy().into_owned()}).collect();
        s.import_gerber_layers(&id, &info.revision, ImportGerberLayersParams { paths })
            .unwrap();
        Self { s, id, dir }
    }
    fn scene(&self) -> RenderSnapshot {
        self.s.render_snapshot(&self.id).unwrap()
    }
    fn info(&self) -> DocumentInfo {
        self.s.document_get(&self.id).unwrap()
    }
    fn groups(&self) -> Vec<SelectionGroup> {
        self.scene()
            .layers
            .iter()
            .map(|l| SelectionGroup {
                layer_id: l.id.clone(),
                object_ids: l.objects[..2].iter().map(|o| o.object_id.clone()).collect(),
            })
            .collect()
    }
    fn begin(&self) -> UnifiedEditorSession {
        let i = self.info();
        self.s
            .unified_editor_begin(
                &self.id,
                &i.revision,
                &i.workspace_revision,
                self.groups(),
                None,
            )
            .unwrap()
    }
    fn set(&self, d: &mut UnifiedEditorSession, width: f64) -> u64 {
        let g = d.generation().unwrap();
        self.s
            .unified_editor_set_aperture_size_step(
                d,
                g,
                DraftApertureSizeStep {
                    groups: self.groups(),
                    width_mm: width,
                    height_mm: 3.,
                },
            )
            .unwrap()
    }
    fn execute(&self, d: &mut UnifiedEditorSession, width: f64) {
        let g = self.set(d, width);
        self.s.unified_editor_execute(d, g, None).unwrap();
    }
    fn apply(&mut self, d: &mut UnifiedEditorSession) -> UnifiedEditorApplyResult {
        let t = self.s.unified_editor_begin_apply(d).unwrap();
        self.s.unified_editor_complete_apply(d, t, None).unwrap()
    }
}
#[test]
fn imported_namespaces_selected_subset_and_final_prepared_table_match_one_transaction() {
    let mut r = Run::new();
    let entry = r.scene();
    let i = r.info();
    let mut d = r.begin();
    r.execute(&mut d, 4.);
    assert_eq!(r.info(), i);
    assert_eq!(r.scene(), entry);
    let temp: Vec<_> = d.work_candidate().unwrap().apertures().cloned().collect();
    assert_eq!(temp.len(), 2);
    let t = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let mut prepared = None;
    let result =
        r.s.unified_editor_complete_apply_prepared(&mut d, t, None, &mut |c| {
            prepared = Some(c.apertures().cloned().collect::<Vec<_>>());
            Ok(())
        })
        .unwrap();
    assert!(result.changed);
    assert!(d.is_closed());
    let final_scene = r.scene();
    let defs = prepared.unwrap();
    assert_eq!(&final_scene.apertures[entry.apertures.len()..], defs);
    assert!(defs.iter().all(|a| !temp.iter().any(|t| t.id == a.id)));
    assert_eq!(result.info.undo_entries, i.undo_entries + 1);
    for l in &entry.layers {
        let after = final_scene.layers.iter().find(|a| a.id == l.id).unwrap();
        assert_eq!(after.objects[2], l.objects[2]);
    }
    r.s.history_undo(&r.id, &r.info().revision).unwrap();
    assert_eq!(r.scene().layers, entry.layers);
    assert_eq!(r.scene().apertures, entry.apertures);
    r.s.history_redo(&r.id, &r.info().revision).unwrap();
    assert_eq!(r.scene().layers, final_scene.layers);
    assert_eq!(r.scene().apertures, final_scene.apertures);
}
#[test]
fn exact_nochange_reset_cancel_and_invalid_latest_never_mutate_main() {
    let mut r = Run::new();
    let entry = r.scene();
    let i = r.info();
    let mut d = r.begin();
    r.execute(&mut d, 4.);
    r.execute(&mut d, 2.);
    let a = r.apply(&mut d);
    assert!(!a.changed);
    assert_eq!(r.info(), i);
    assert_eq!(r.scene(), entry);
    let mut d = r.begin();
    r.execute(&mut d, 4.);
    d.reset().unwrap();
    assert_eq!(d.work_candidate().unwrap().apertures().len(), 0);
    d.cancel();
    assert!(d.is_closed());
    assert_eq!(r.info(), i);
    let mut d = r.begin();
    let g = r.set(&mut d, 4.);
    r.s.unified_editor_preview(&mut d, g, None).unwrap();
    let g = d.generation().unwrap();
    assert!(
        r.s.unified_editor_set_aperture_size_step(
            &mut d,
            g,
            DraftApertureSizeStep {
                groups: r.groups(),
                width_mm: f64::NAN,
                height_mm: 3.
            }
        )
        .is_err()
    );
    assert!(r.s.unified_editor_begin_apply(&mut d).is_err());
    assert_eq!(r.scene(), entry);
}
#[test]
fn cancelled_preparation_retains_work_and_authentic_ticket_retries_once() {
    let mut r = Run::new();
    let entry = r.scene();
    let i = r.info();
    let mut d = r.begin();
    r.execute(&mut d, 4.);
    let work = d.work_geometry().unwrap().to_vec();
    let g = d.generation().unwrap();
    let token = task::CancellationToken::default();
    let t = r.s.unified_editor_begin_apply(&mut d).unwrap();
    let e =
        r.s.unified_editor_complete_apply_prepared(&mut d, t, Some(&token), &mut |_| {
            token.cancel();
            Ok(())
        })
        .unwrap_err();
    assert_eq!(e.code, "CANCELLED");
    assert!(!d.is_closed());
    assert_eq!(d.generation().unwrap(), g);
    assert_eq!(d.work_geometry().unwrap(), work);
    assert_eq!(r.info(), i);
    assert_eq!(r.scene(), entry);
    assert!(r.apply(&mut d).changed);
}
#[test]
fn stale_workspace_and_foreign_service_cannot_publish_ap_geometry() {
    let mut r = Run::new();
    let mut d = r.begin();
    r.execute(&mut d, 4.);
    let other = Run::new();
    assert!(other.s.unified_editor_begin_apply(&mut d).is_err());
    let layer = r.scene().layers[0].id.clone();
    let i = r.info();
    r.s.layer_update(
        &r.id,
        &i.revision,
        LayerUpdateParams {
            layer_id: layer,
            expected_workspace_revision: i.workspace_revision,
            locked: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let scene = r.scene();
    let i = r.info();
    assert!(r.s.unified_editor_begin_apply(&mut d).is_err());
    assert_eq!(r.info(), i);
    assert_eq!(r.scene(), scene);
    d.cancel();
}
