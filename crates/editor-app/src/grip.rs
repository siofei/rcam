//! Single-object transient gesture; release alone submits a service transaction.
use crate::{
    camera::Camera,
    state::{Action, Classifier, View},
};
use editor_core::{
    grip::{GripEditPreview, GripFeature, GripFeatureId},
    *,
};
use eframe::egui::{self, Pos2, Rect};
use std::collections::HashSet;

pub const MARKER_PX: f32 = crate::ui::tokens::GRIP_MARKER_PX;
pub const HIT_PX: f32 = crate::ui::tokens::GRIP_HIT_PX;
pub fn selected(view: &View) -> Option<&editor_service::ObjectInfo> {
    if view.selected.ordered.len() != 1 || view.blocked.is_some() {
        return None;
    }
    let object = view.selected.primary()?;
    let classifier = Classifier::new(&view.layers, &view.apertures);
    (classifier.selectable(object)
        && classifier.edit_refusal(object).is_none()
        && view
            .layers
            .iter()
            .any(|l| l.layer_id == object.layer_id && l.is_active))
    .then_some(object)
}
fn aperture<'a>(view: &'a View, object: &SemanticObject) -> Option<&'a ApertureShape> {
    match &object.geometry {
        SemanticGeometry::Flash { aperture_id, .. } => view
            .apertures
            .iter()
            .find(|a| &a.id == aperture_id)
            .map(|a| &a.shape),
        _ => None,
    }
}

pub fn features(view: &View) -> Result<Vec<GripFeature>, String> {
    let Some(object) = selected(view) else {
        return Ok(vec![]);
    };
    editor_core::grip::grip_features(&object.object, aperture(view, &object.object))
        .map_err(|e| format!("Grip: {e:?}"))
}
pub fn hit(
    features: &[GripFeature],
    position: Pos2,
    camera: Camera,
    rect: Rect,
    ppp: f32,
) -> Option<GripFeatureId> {
    features
        .iter()
        .filter_map(|f| {
            let d = camera.screen(f.position_mm, rect).distance(position) * ppp;
            (d <= HIT_PX).then_some((f.id, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|v| v.0)
}
#[derive(Clone)]
pub struct Session {
    pub document: String,
    pub revision: String,
    pub layer: String,
    pub object: SemanticObject,
    pub id: GripFeatureId,
    pub target: MmPoint,
    pub excluded: HashSet<String>,
    aperture: Option<ApertureShape>,
    pub preview: Result<GripEditPreview, String>,
    pub moved: bool,
    pub pressed: Option<Pos2>,
}
impl Session {
    pub fn arm(view: &View, id: GripFeatureId) -> Option<Self> {
        let object = selected(view)?;
        let info = view.info.as_ref()?;
        let shape = aperture(view, &object.object).cloned();
        let target = editor_core::grip::grip_features(&object.object, shape.as_ref())
            .ok()?
            .into_iter()
            .find(|f| f.id == id)?
            .position_mm;
        rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info, "grip.begin");
        Some(Self {
            document: info.document_id.clone(),
            revision: info.revision.clone(),
            layer: object.layer_id.clone(),
            object: object.object.clone(),
            id,
            target,
            excluded: [object.object.object_id.clone()].into_iter().collect(),
            preview: Ok(GripEditPreview {
                geometry: object.object.geometry.clone(),
                aperture_shape: shape.clone(),
            }),
            aperture: shape,
            moved: false,
            pressed: None,
        })
    }
    pub fn features(&self) -> Result<Vec<GripFeature>, String> {
        match &self.preview {
            Ok(preview) => {
                let object = SemanticObject {
                    geometry: preview.geometry.clone(),
                    object_id: self.object.object_id.clone(),
                    exposure: self.object.exposure,
                    origin: self.object.origin.clone(),
                };
                editor_core::grip::grip_features(
                    &object,
                    preview.aperture_shape.as_ref().or(self.aperture.as_ref()),
                )
                .map_err(|e| format!("Grip: {e:?}"))
            }
            Err(_) => editor_core::grip::grip_features(&self.object, self.aperture.as_ref())
                .map_err(|e| format!("Grip: {e:?}")),
        }
    }
    pub fn valid(&self, view: &View) -> bool {
        selected(view).is_some_and(|o| {
            o.layer_id == self.layer && o.object.object_id == self.object.object_id
        }) && view
            .info
            .as_ref()
            .is_some_and(|i| i.document_id == self.document && i.revision == self.revision)
    }
    pub fn update(&mut self, target: MmPoint) {
        if self.target == target {
            return;
        }
        self.target = target;
        self.moved = true;
        self.preview = editor_core::grip::preview_grip_edit(
            &self.object,
            self.aperture.as_ref(),
            self.id,
            target,
        )
        .map_err(|e| format!("无效控制点：{e:?}"));
    }
    pub fn release(self) -> Option<Action> {
        (self.moved && self.preview.is_ok()).then_some(Action::GripEdit(Box::new(self)))
    }
    pub fn paint(&self, painter: &egui::Painter, camera: Camera, rect: Rect, ppp: f32) {
        let color = if self.preview.is_ok() {
            crate::ui::tokens::GRIP_ACTIVE
        } else {
            crate::ui::tokens::destructive()
        };
        let Ok(preview) = &self.preview else {
            painter.circle_stroke(
                camera.screen(self.target, rect),
                7. / ppp,
                egui::Stroke::new(1. / ppp, color),
            );
            return;
        };
        let stroke = egui::Stroke::new(1.5 / ppp, color);
        let line = |points: Vec<MmPoint>| {
            painter.add(egui::Shape::line(
                points.into_iter().map(|p| camera.screen(p, rect)).collect(),
                stroke,
            ));
        };
        let circle = |center: MmPoint, r: f64| {
            (0..=128)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / 128.;
                    MmPoint::new(center.x_mm + r * a.cos(), center.y_mm + r * a.sin())
                })
                .collect::<Vec<_>>()
        };
        match &preview.geometry {
            SemanticGeometry::Flash {
                center, transform, ..
            } => {
                let Some(shape) = &preview.aperture_shape else {
                    return;
                };
                let map = |p: MmPoint| {
                    let x = p.x_mm
                        * if matches!(transform.mirror, Mirror::X | Mirror::Xy) {
                            -1.
                        } else {
                            1.
                        };
                    let y = p.y_mm
                        * if matches!(transform.mirror, Mirror::Y | Mirror::Xy) {
                            -1.
                        } else {
                            1.
                        };
                    let (s, c) = transform.rotation_deg.to_radians().sin_cos();
                    MmPoint::new(
                        center.x_mm + transform.scale * (x * c - y * s),
                        center.y_mm + transform.scale * (x * s + y * c),
                    )
                };
                let points = match shape {
                    ApertureShape::Circle { diameter_mm, .. } => {
                        circle(MmPoint::new(0., 0.), diameter_mm / 2.)
                    }
                    ApertureShape::Rectangle {
                        width_mm: w,
                        height_mm: h,
                        ..
                    } => vec![
                        MmPoint::new(-w / 2., -h / 2.),
                        MmPoint::new(w / 2., -h / 2.),
                        MmPoint::new(w / 2., h / 2.),
                        MmPoint::new(-w / 2., h / 2.),
                        MmPoint::new(-w / 2., -h / 2.),
                    ],
                    ApertureShape::Polygon {
                        diameter_mm,
                        vertices,
                        rotation_deg,
                        ..
                    } => (0..=usize::from(*vertices))
                        .map(|i| {
                            let a = rotation_deg.to_radians()
                                + std::f64::consts::TAU * i as f64 / f64::from(*vertices);
                            MmPoint::new(diameter_mm / 2. * a.cos(), diameter_mm / 2. * a.sin())
                        })
                        .collect(),
                    ApertureShape::Obround {
                        width_mm: w,
                        height_mm: h,
                        ..
                    } => {
                        let r = w.min(*h) / 2.;
                        (0..=128)
                            .map(|i| {
                                let a = std::f64::consts::TAU * i as f64 / 128.;
                                MmPoint::new(
                                    r * a.cos()
                                        + if w > h {
                                            (w - h) / 2. * a.cos().signum()
                                        } else {
                                            0.
                                        },
                                    r * a.sin()
                                        + if h > w {
                                            (h - w) / 2. * a.sin().signum()
                                        } else {
                                            0.
                                        },
                                )
                            })
                            .collect()
                    }
                    _ => vec![],
                };
                line(points.into_iter().map(map).collect());
                let hole = match shape {
                    ApertureShape::Circle {
                        hole_diameter_mm, ..
                    }
                    | ApertureShape::Rectangle {
                        hole_diameter_mm, ..
                    }
                    | ApertureShape::Obround {
                        hole_diameter_mm, ..
                    }
                    | ApertureShape::Polygon {
                        hole_diameter_mm, ..
                    } => *hole_diameter_mm,
                    _ => None,
                };
                if let Some(diameter) = hole {
                    line(
                        circle(MmPoint::new(0., 0.), diameter / 2.)
                            .into_iter()
                            .map(map)
                            .collect(),
                    );
                }
            }
            SemanticGeometry::Line {
                start,
                end,
                width_mm,
            } => {
                painter.line_segment(
                    [camera.screen(*start, rect), camera.screen(*end, rect)],
                    egui::Stroke::new((width_mm * camera.scale) as f32, color.gamma_multiply(0.45)),
                );
                line(vec![*start, *end]);
            }
            SemanticGeometry::RectangularSweep {
                start,
                end,
                width_mm: w,
                height_mm: h,
            } => {
                let (left, right, bottom, top) = (
                    start.x_mm.min(end.x_mm) - w / 2.,
                    start.x_mm.max(end.x_mm) + w / 2.,
                    start.y_mm.min(end.y_mm) - h / 2.,
                    start.y_mm.max(end.y_mm) + h / 2.,
                );
                line(vec![
                    MmPoint::new(left, bottom),
                    MmPoint::new(right, bottom),
                    MmPoint::new(right, top),
                    MmPoint::new(left, top),
                    MmPoint::new(left, bottom),
                ]);
            }
            SemanticGeometry::Arc { path, .. } => {
                let start =
                    (path.start.y_mm - path.center.y_mm).atan2(path.start.x_mm - path.center.x_mm);
                let sweep = path.sweep_radians().unwrap_or(0.)
                    * if path.direction == ArcDirection::Clockwise {
                        -1.
                    } else {
                        1.
                    };
                line(
                    (0..=128)
                        .map(|i| {
                            let a = start + sweep * i as f64 / 128.;
                            MmPoint::new(
                                path.center.x_mm + path.radius() * a.cos(),
                                path.center.y_mm + path.radius() * a.sin(),
                            )
                        })
                        .collect(),
                );
            }
            SemanticGeometry::Region { contours } => {
                for c in contours {
                    for e in &c.edges {
                        if let RegionEdge::Line { start, end } = e {
                            line(vec![*start, *end]);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
pub fn paint_features(
    painter: &egui::Painter,
    features: &[GripFeature],
    hover: Option<GripFeatureId>,
    active: Option<GripFeatureId>,
    camera: Camera,
    rect: Rect,
    ppp: f32,
) {
    for f in features {
        let color = if Some(f.id) == active {
            crate::ui::tokens::GRIP_ACTIVE
        } else if Some(f.id) == hover {
            crate::ui::tokens::GRIP_HOVER
        } else {
            crate::ui::tokens::GRIP_NORMAL
        };
        painter.rect_filled(
            Rect::from_center_size(
                camera.screen(f.position_mm, rect),
                egui::Vec2::splat(MARKER_PX / ppp),
            ),
            0.,
            color,
        );
    }
}

#[cfg(test)]
#[path = "grip_tests.rs"]
mod tests;
