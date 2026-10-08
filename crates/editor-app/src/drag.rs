//! Gesture state belongs to the UI; only release produces a service action.
use crate::{
    camera::Camera,
    state::{Action, View},
};
use editor_core::MmPoint;
use eframe::egui::{Pos2, Rect};
use std::collections::HashSet;

/// Resource-only reply owned by one confirmed probe; final service validation
/// still decides whether the actual delta is legal and commits atomically.
#[derive(Clone)]
pub struct MoveAdmission {
    version: editor_service::task::TaskVersion,
    selection_epoch: u64,
    project_id: Option<String>,
    selected: crate::shared_snapshot::SnapshotVec<editor_service::ObjectInfo>,
    pub result: Result<editor_service::MoveDemand, editor_service::ServiceError>,
}
impl MoveAdmission {
    pub fn new(
        view: &View,
        result: Result<editor_service::MoveDemand, editor_service::ServiceError>,
    ) -> Self {
        Self {
            version: editor_service::task::TaskVersion::capture(
                view.info.as_ref(),
                view.task_generation,
                view.rule_revision,
            ),
            selection_epoch: view.selection_epoch,
            project_id: view.info.as_ref().map(|d| d.project_id.clone()),
            selected: view.selected.ordered.clone(),
            result,
        }
    }
    pub(crate) fn matches(&self, view: &View) -> bool {
        self.version
            == editor_service::task::TaskVersion::capture(
                view.info.as_ref(),
                view.task_generation,
                view.rule_revision,
            )
            && self.project_id == view.info.as_ref().map(|d| d.project_id.clone())
            && self.selection_epoch == view.selection_epoch
            && self.selected.shares_storage(&view.selected.ordered)
    }
}

pub const THRESHOLD_PX: f32 = 4.;
#[derive(Clone)]
pub struct Drag {
    pub document: String,
    pub revision: String,
    pub groups: Vec<editor_service::SelectionGroup>,
    pub objects: Vec<String>,
    excluded_snap_objects: HashSet<String>,
    start: Pos2,
    camera: Camera,
    rect: Rect,
    ppp: f32,
    pub confirmed: bool,
    pub last: Pos2,
    pub dragging: bool,
    pub delta: MmPoint,
    pub error: Option<String>,
}
pub fn editable_selection(view: &View) -> bool {
    let classifier = crate::state::Classifier::new(&view.layers, &view.apertures);
    view.blocked.is_none()
        && view.scene.is_some()
        && !view.selected.ordered.is_empty()
        && view
            .selected
            .ordered
            .iter()
            .all(|o| classifier.selectable(o) && classifier.edit_refusal(o).is_none())
}
impl Drag {
    pub fn arm(view: &View, start: Pos2, camera: Camera, rect: Rect, ppp: f32) -> Option<Self> {
        if !editable_selection(view) {
            return None;
        }
        let d = view.info.as_ref()?;
        view.selected.primary()?;
        let objects: Vec<_> = view
            .selected
            .ordered
            .iter()
            .map(|o| o.object.object_id.clone())
            .collect();
        Some(Self {
            document: d.document_id.clone(),
            revision: d.revision.clone(),
            groups: view.selected.groups(),
            excluded_snap_objects: objects.iter().cloned().collect(),
            objects,
            start,
            camera,
            rect,
            ppp,
            confirmed: false,
            last: start,
            dragging: false,
            delta: MmPoint::new(0., 0.),
            error: None,
        })
    }
    #[cfg(test)]
    pub fn update(&mut self, pos: Pos2) {
        self.update_snapped(pos, None);
    }
    pub fn update_snapped(&mut self, pos: Pos2, snapped_target: Option<MmPoint>) {
        self.last = pos;
        if !self.confirmed {
            return;
        }
        self.dragging |= pos.distance(self.start) * self.ppp >= THRESHOLD_PX;
        if self.dragging {
            let a = self.camera.world(self.start, self.rect);
            let b = snapped_target.unwrap_or_else(|| self.camera.world(pos, self.rect));
            self.delta = MmPoint::new(b.x_mm - a.x_mm, b.y_mm - a.y_mm);
        }
    }
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }
    pub fn release(self) -> Option<Action> {
        (self.error.is_none()
            && self.confirmed
            && self.dragging
            && (self.delta.x_mm != 0. || self.delta.y_mm != 0.))
            .then_some(Action::DragMove(Box::new(self)))
    }
}
/// PointerGone/blur always wins over a same-frame release.
pub fn cancelled(
    escape: bool,
    focused: bool,
    pointer_gone: bool,
    down: bool,
    released: bool,
) -> bool {
    escape || !focused || pointer_gone || (!down && !released)
}
pub fn shortcuts_allowed(text_focus: bool, busy: bool, modal: bool) -> bool {
    !text_focus && !busy && !modal
}

/// The pending hit retains press/release coordinates across the worker reply.
/// Selection changes only on release, so cancelling never loses the old set.
pub struct Gesture {
    pub(crate) probe_task_id: Option<u64>,
    pub start: Pos2,
    pub last: Pos2,
    pub released: bool,
    pub confirmed: bool,
    pub delta: MmPoint,
    pub box_select: bool,
    moved: bool,
    camera: Camera,
    rect: Rect,
    ppp: f32,
    mode: crate::selection::SelectionMode,
    navigation_epoch: u64,
    object_drag: Option<Drag>,
    capacity_error: Option<String>,
}
impl Gesture {
    pub fn arm(
        view: &View,
        start: Pos2,
        camera: Camera,
        rect: Rect,
        ppp: f32,
        mode: crate::selection::SelectionMode,
    ) -> Self {
        Self {
            probe_task_id: None,
            start,
            last: start,
            released: false,
            confirmed: false,
            delta: MmPoint::new(0., 0.),
            box_select: false,
            moved: false,
            camera,
            rect,
            ppp,
            mode,
            navigation_epoch: 0,
            capacity_error: None,
            object_drag: if mode != crate::selection::SelectionMode::Replace {
                None
            } else {
                Drag::arm(view, start, camera, rect, ppp)
            },
        }
    }
    pub fn with_navigation_epoch(mut self, epoch: u64) -> Self {
        self.navigation_epoch = epoch;
        self
    }
    /// Suppress movement arming without disabling click or box selection.
    /// Enabling the preference later cannot retroactively arm this press.
    pub fn with_movement_enabled(mut self, enabled: bool) -> Self {
        if !enabled {
            self.object_drag = None;
        }
        self
    }
    pub fn movement_armed(&self) -> bool {
        self.object_drag.is_some()
    }
    #[cfg(feature = "internal-evidence")]
    pub fn evidence_dragging(&self) -> bool {
        self.object_drag.as_ref().is_some_and(|d| d.dragging)
    }
    #[cfg(feature = "internal-evidence")]
    pub fn evidence_state(&self) -> serde_json::Value {
        serde_json::json!({"start":[self.start.x,self.start.y],"last":[self.last.x,self.last.y],"moved":self.moved,"confirmed":self.confirmed,"released":self.released,"mode":format!("{:?}",self.mode),"box_select":self.box_select,"object_drag":self.object_drag.as_ref().map(|d|serde_json::json!({"dragging":d.dragging,"confirmed":d.confirmed,"error":d.error}))})
    }
    pub fn snap_exclusions(&self) -> Option<&HashSet<String>> {
        self.object_drag
            .as_ref()
            .map(|drag| &drag.excluded_snap_objects)
    }
    pub fn error(&self) -> Option<&str> {
        self.capacity_error
            .as_deref()
            .or_else(|| self.object_drag.as_ref().and_then(|d| d.error.as_deref()))
    }
    pub fn set_snap_error(&mut self, error: Option<String>) {
        if let Some(drag) = &mut self.object_drag {
            drag.set_error(error);
        }
    }
    pub fn confirm(&mut self, view: &View) {
        self.confirmed = true;
        self.box_select =
            self.mode != crate::selection::SelectionMode::Replace || view.press_hit.is_none();
        if !view.drag_hit {
            self.object_drag = None;
        }
        if let Some(d) = &mut self.object_drag {
            self.capacity_error = match view.move_admission.as_deref().filter(|a| a.matches(view)) {
                Some(a) => a
                    .result
                    .as_ref()
                    .err()
                    .map(|e| format!("{} · {}", e.code, e.message)),
                None => Some("移动资源检查未确认；请重新点击后移动".into()),
            };
            d.confirmed = self.capacity_error.is_none();
        }
        self.update(self.last);
    }
    pub fn update(&mut self, pos: Pos2) {
        self.update_snapped(pos, None);
    }
    pub fn update_snapped(&mut self, pos: Pos2, snapped_target: Option<MmPoint>) {
        self.last = pos;
        self.moved |= pos.distance(self.start) * self.ppp >= THRESHOLD_PX;
        if let Some(d) = &mut self.object_drag {
            d.update_snapped(pos, snapped_target);
            self.delta = d.delta;
        }
    }
    pub fn preview_rect(&self) -> Option<(Rect, bool)> {
        (self.confirmed && self.box_select && self.moved).then_some((
            Rect::from_two_pos(self.start, self.last),
            self.last.x >= self.start.x,
        ))
    }
    pub fn release(self) -> Option<Action> {
        if !self.confirmed {
            return None;
        }
        if self.box_select && self.moved {
            if self.last == self.start {
                return None;
            }
            let a = self.camera.world(self.start, self.rect);
            let b = self.camera.world(self.last, self.rect);
            return Some(Action::CanvasSelectRect(
                editor_core::BoundsMm {
                    min_x_mm: a.x_mm.min(b.x_mm),
                    min_y_mm: a.y_mm.min(b.y_mm),
                    max_x_mm: a.x_mm.max(b.x_mm),
                    max_y_mm: a.y_mm.max(b.y_mm),
                },
                if self.last.x >= self.start.x {
                    editor_core::hit_test::SelectRectMode::Window
                } else {
                    editor_core::hit_test::SelectRectMode::Crossing
                },
                self.mode,
            ));
        }
        if let Some(d) = self.object_drag
            && d.dragging
            && self.capacity_error.is_none()
        {
            return d.release();
        }
        if self.moved {
            return None;
        }
        let mut context =
            crate::selection::ClickContext::new(self.start, self.camera, self.rect, self.ppp);
        context.navigation_epoch = self.navigation_epoch;
        Some(Action::CanvasSelect(context, self.mode))
    }
}

#[cfg(test)]
mod capacity_tests {
    use super::*;
    #[test]
    fn admission_fences_exact_view_and_shared_selection_identity() {
        let mut model = crate::state::Model::default();
        model.run(Action::Open(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/s5i2b/layer_a.gbr"),
        ));
        assert!(model.view.error.is_none());
        let original = model.view.clone();
        let a = MoveAdmission::new(
            &original,
            Err(editor_service::ServiceError {
                code: "RESOURCE_LIMIT".into(),
                message: "test".into(),
                details: serde_json::json!({}),
            }),
        );
        assert!(a.matches(&original));
        for field in [
            "document",
            "revision",
            "workspace",
            "project",
            "generation",
            "rules",
            "epoch",
            "selection",
            "precision",
        ] {
            let mut changed = original.clone();
            match field {
                "document" => changed.info.as_mut().unwrap().document_id.push('x'),
                "revision" => changed.info.as_mut().unwrap().revision.push('x'),
                "workspace" => changed.info.as_mut().unwrap().workspace_revision.push('x'),
                "project" => changed.info.as_mut().unwrap().project_id = "changed".into(),
                "generation" => changed.task_generation += 1,
                "rules" => changed.rule_revision += 1,
                "epoch" => changed.selection_epoch += 1,
                "selection" => changed.selected.ordered = Vec::new().into(),
                "precision" => {
                    changed
                        .info
                        .as_mut()
                        .unwrap()
                        .manufacturing_precision
                        .resolution_mm *= 2.
                }
                _ => unreachable!(),
            }
            assert!(!a.matches(&changed), "{field}");
        }
    }
}
