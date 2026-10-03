//! Ordered workspace-only selection. Primary is the most recently added item.
use editor_service::ObjectInfo;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
    Remove,
}
impl SelectionMode {
    pub fn from_modifiers(modifiers: eframe::egui::Modifiers) -> Self {
        if modifiers.shift {
            Self::Remove
        } else if modifiers.ctrl {
            Self::Add
        } else {
            Self::Replace
        }
    }
}
#[derive(Clone, Default, Debug, PartialEq)]
pub struct SelectionSet {
    pub ordered: Vec<ObjectInfo>,
}
impl SelectionSet {
    pub fn groups(&self) -> Vec<editor_service::SelectionGroup> {
        let mut groups: Vec<editor_service::SelectionGroup> = Vec::new();
        for object in &self.ordered {
            if let Some(group) = groups.iter_mut().find(|g| g.layer_id == object.layer_id) {
                group.object_ids.push(object.object.object_id.clone());
            } else {
                groups.push(editor_service::SelectionGroup {
                    layer_id: object.layer_id.clone(),
                    object_ids: vec![object.object.object_id.clone()],
                });
            }
        }
        groups
    }
    pub fn primary(&self) -> Option<&ObjectInfo> {
        self.ordered.last()
    }
    pub fn contains(&self, layer: &str, id: &str) -> bool {
        self.ordered
            .iter()
            .any(|o| o.layer_id == layer && o.object.object_id == id)
    }
    pub fn click(&mut self, object: Option<ObjectInfo>, mode: SelectionMode) {
        if mode == SelectionMode::Replace {
            self.ordered.clear();
        }
        if let Some(o) = object {
            if let Some(i) = self
                .ordered
                .iter()
                .position(|x| x.layer_id == o.layer_id && x.object.object_id == o.object.object_id)
            {
                if mode == SelectionMode::Remove {
                    self.ordered.remove(i);
                }
            } else if mode != SelectionMode::Remove {
                self.ordered.push(o);
            }
        }
    }
    pub fn ids(&self) -> Vec<&str> {
        self.ordered
            .iter()
            .map(|o| o.object.object_id.as_str())
            .collect()
    }
}

/// Screen-space identity, never manufacturing geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct ClickContext {
    pub point: eframe::egui::Pos2,
    pub world: editor_core::MmPoint,
    pub tolerance: f64,
    pub ppp: f32,
    pub camera: [f64; 3],
    pub rect: eframe::egui::Rect,
    pub navigation_epoch: u64,
}
impl ClickContext {
    pub fn new(
        point: eframe::egui::Pos2,
        camera: crate::camera::Camera,
        rect: eframe::egui::Rect,
        ppp: f32,
    ) -> Self {
        Self {
            point,
            world: camera.world(point, rect),
            tolerance: camera.tolerance(ppp),
            ppp,
            camera: [camera.center.x_mm, camera.center.y_mm, camera.scale],
            rect,
            navigation_epoch: 0,
        }
    }
    pub fn same_place(&self, other: &Self) -> bool {
        self.camera == other.camera
            && self.rect == other.rect
            && self.ppp == other.ppp
            && self.navigation_epoch == other.navigation_epoch
            && self.point.distance(other.point) * self.ppp <= 2.
    }
}
#[derive(Clone, Debug)]
pub struct ClickCycle {
    pub context: ClickContext,
    pub document: String,
    pub revision: String,
    pub workspace: String,
    pub candidates: Vec<(String, String)>,
    pub index: usize,
}
#[derive(Default)]
pub struct ClickNavigation {
    previous: Option<([f64; 3], eframe::egui::Rect, f32)>,
    epoch: u64,
}
impl ClickNavigation {
    #[cfg(feature = "internal-evidence")]
    pub(crate) fn evidence_epoch(&self) -> u64 {
        self.epoch
    }
    pub fn observe(
        &mut self,
        camera: crate::camera::Camera,
        rect: eframe::egui::Rect,
        ppp: f32,
    ) -> u64 {
        let key = (
            [camera.center.x_mm, camera.center.y_mm, camera.scale],
            rect,
            ppp,
        );
        if self.previous != Some(key) {
            self.epoch = self.epoch.wrapping_add(1);
            self.previous = Some(key);
        }
        self.epoch
    }
}
