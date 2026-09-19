//! Gesture state belongs to the UI; only release produces a service action.
use crate::{
    camera::Camera,
    state::{Action, View},
};
use editor_core::MmPoint;
use eframe::egui::{Pos2, Rect};

pub const THRESHOLD_PX: f32 = 4.;
#[derive(Clone)]
pub struct Drag {
    pub document: String,
    pub revision: String,
    pub layer: String,
    pub object: String,
    start: Pos2,
    camera: Camera,
    rect: Rect,
    ppp: f32,
    pub confirmed: bool,
    pub last: Pos2,
    pub released: bool,
    pub dragging: bool,
    pub delta: MmPoint,
}
pub fn editable_selection(view: &View) -> bool {
    view.blocked.is_none()
        && view.scene.is_some()
        && view.selected.as_ref().is_some_and(|o| {
            view.layers
                .iter()
                .any(|l| l.layer_id == o.layer_id && l.visible && !l.locked)
        })
}
impl Drag {
    pub fn arm(view: &View, start: Pos2, camera: Camera, rect: Rect, ppp: f32) -> Option<Self> {
        if !editable_selection(view) {
            return None;
        }
        let d = view.info.as_ref()?;
        let o = view.selected.as_ref()?;
        Some(Self {
            document: d.document_id.clone(),
            revision: d.revision.clone(),
            layer: o.layer_id.clone(),
            object: o.object.object_id.clone(),
            start,
            camera,
            rect,
            ppp,
            confirmed: false,
            last: start,
            released: false,
            dragging: false,
            delta: MmPoint::new(0., 0.),
        })
    }
    pub fn update(&mut self, pos: Pos2) {
        self.last = pos;
        if !self.confirmed {
            return;
        }
        self.dragging |= pos.distance(self.start) * self.ppp >= THRESHOLD_PX;
        if self.dragging {
            let a = self.camera.world(self.start, self.rect);
            let b = self.camera.world(pos, self.rect);
            self.delta = MmPoint::new(b.x_mm - a.x_mm, b.y_mm - a.y_mm);
        }
    }
    pub fn release(self) -> Option<Action> {
        (self.confirmed && self.dragging && (self.delta.x_mm != 0. || self.delta.y_mm != 0.))
            .then_some(Action::DragMove(self))
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
