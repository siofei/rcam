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
    pub objects: Vec<String>,
    start: Pos2,
    camera: Camera,
    rect: Rect,
    ppp: f32,
    pub confirmed: bool,
    pub last: Pos2,
    pub dragging: bool,
    pub delta: MmPoint,
    pub grid: crate::tools::GridSettings,
    pub error: Option<String>,
}
pub fn editable_selection(view: &View) -> bool {
    view.blocked.is_none()
        && view.scene.is_some()
        && !view.selected.ordered.is_empty()
        && view.selected.ordered.iter().all(|o| {
            view.selected
                .primary()
                .is_some_and(|p| p.layer_id == o.layer_id)
                && view
                    .layers
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
        let o = view.selected.primary()?;
        Some(Self {
            document: d.document_id.clone(),
            revision: d.revision.clone(),
            layer: o.layer_id.clone(),
            objects: view
                .selected
                .ordered
                .iter()
                .map(|o| o.object.object_id.clone())
                .collect(),
            start,
            camera,
            rect,
            ppp,
            confirmed: false,
            last: start,
            dragging: false,
            delta: MmPoint::new(0., 0.),
            grid: Default::default(),
            error: None,
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
            let b = match self.grid.point(self.camera.world(pos, self.rect)) {
                Ok(b) => b,
                Err(e) => {
                    self.error = Some(e);
                    self.delta = MmPoint::new(0., 0.);
                    return;
                }
            };
            self.delta = MmPoint::new(b.x_mm - a.x_mm, b.y_mm - a.y_mm);
        }
    }
    pub fn release(self) -> Option<Action> {
        (self.error.is_none()
            && self.confirmed
            && self.dragging
            && (self.delta.x_mm != 0. || self.delta.y_mm != 0.))
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

/// The pending hit retains press/release coordinates across the worker reply.
/// Selection changes only on release, so cancelling never loses the old set.
pub struct Gesture {
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
    object_drag: Option<Drag>,
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
            object_drag: if mode != crate::selection::SelectionMode::Replace {
                None
            } else {
                Drag::arm(view, start, camera, rect, ppp)
            },
        }
    }
    pub fn set_grid(&mut self, grid: crate::tools::GridSettings) {
        if let Some(d) = &mut self.object_drag {
            d.grid = grid;
        }
    }
    pub fn error(&self) -> Option<&str> {
        self.object_drag.as_ref().and_then(|d| d.error.as_deref())
    }
    pub fn confirm(&mut self, view: &View) {
        self.confirmed = true;
        self.box_select = view.press_hit.is_none();
        if !view.drag_hit {
            self.object_drag = None;
        }
        if let Some(d) = &mut self.object_drag {
            d.confirmed = true;
        }
        self.update(self.last);
    }
    pub fn update(&mut self, pos: Pos2) {
        self.last = pos;
        self.moved |= pos.distance(self.start) * self.ppp >= THRESHOLD_PX;
        if let Some(d) = &mut self.object_drag {
            d.update(pos);
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
            let a = self.camera.world(self.start, self.rect);
            let b = self.camera.world(self.last, self.rect);
            return Some(Action::SelectRect(
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
            ));
        }
        if let Some(d) = self.object_drag
            && d.dragging
        {
            return d.release();
        }
        Some(Action::Select(
            self.camera.world(self.start, self.rect),
            self.camera.tolerance(self.ppp),
            self.mode,
        ))
    }
}
