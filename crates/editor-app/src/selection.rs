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
