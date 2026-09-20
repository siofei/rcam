//! Workspace-only text draft. Geometry is produced exclusively by the typed service.
use editor_core::{MmPoint, RegionEdge, SemanticGeometry};
use editor_service::{
    FontInfo, HorizontalAlign, ServiceError, TextLayout, TextParams, TextPreviewResult,
    VerticalAlign,
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

pub const DEBOUNCE: Duration = Duration::from_millis(200);
const RECENT_LIMIT: usize = 8;
pub const PRESETS: [(&str, f64); 3] = [("标准", 0.00025), ("高", 0.000125), ("超高", 0.0000625)];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    #[default]
    Mouse,
    Absolute,
    Relative,
}
#[derive(Clone)]
pub struct Request {
    pub generation: u64,
    pub document: String,
    pub revision: String,
    pub params: TextParams,
}
pub enum Reply {
    Catalog(Result<Arc<Vec<crate::font_catalog::InstalledFont>>, String>),
    Font {
        generation: u64,
        result: Result<FontInfo, ServiceError>,
    },
    Preview {
        request: Box<Request>,
        result: Result<Arc<TextPreviewResult>, ServiceError>,
        finished: Instant,
        worker_ms: f64,
    },
}
pub struct Draft {
    pub context: Option<(String, String, String, String)>,
    pub text: String,
    pub height: String,
    pub offset: String,
    pub tracking: String,
    pub rotation: String,
    pub tolerance: String,
    pub h_align: HorizontalAlign,
    pub v_align: VerticalAlign,
    pub x: String,
    pub y: String,
    pub rx: String,
    pub ry: String,
    pub dx: String,
    pub dy: String,
    pub placement: Placement,
    pub has_reference: bool,
    pub pick_reference: bool,
    pub snap_text: bool,
    pub font: Option<FontInfo>,
    pub recent: Vec<FontInfo>,
    pub catalog: Option<Arc<Vec<crate::font_catalog::InstalledFont>>>,
    pub catalog_requested: bool,
    pub catalog_error: Option<String>,
    pub font_search: String,
    pub pending_postscript: Option<String>,
    pub face: u32,
    pub font_path: Option<PathBuf>,
    pub pending_apply: Option<u64>,
    pub pending_font: Option<PathBuf>,
    pub font_generation: u64,
    pub generation: u64,
    pub changed_at: Option<Instant>,
    pub submitted: Option<u64>,
    pub preview: Option<Arc<TextPreviewResult>>,
    pub status: String,
    pub candidates: usize,
    pub publish_ms: f64,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            context: None,
            text: String::new(),
            height: "3".into(),
            offset: "0".into(),
            tracking: "0".into(),
            rotation: "0".into(),
            tolerance: "0.00025".into(),
            h_align: HorizontalAlign::Left,
            v_align: VerticalAlign::Bottom,
            x: "0".into(),
            y: "0".into(),
            rx: "0".into(),
            ry: "0".into(),
            dx: "0".into(),
            dy: "0".into(),
            placement: Placement::Mouse,
            has_reference: false,
            pick_reference: false,
            snap_text: false,
            font: None,
            recent: vec![],
            catalog: None,
            catalog_requested: false,
            catalog_error: None,
            font_search: String::new(),
            pending_postscript: None,
            face: 0,
            font_path: None,
            pending_apply: None,
            pending_font: None,
            font_generation: 0,
            generation: 0,
            changed_at: None,
            submitted: None,
            preview: None,
            status: "选择字体并输入单行文字".into(),
            candidates: 0,
            publish_ms: 0.,
        }
    }
}
fn number(value: &str) -> Result<f64, String> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| "请输入有限毫米数值".into())
}
impl Draft {
    pub fn changed(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
        self.changed_at = Some(Instant::now());
        self.submitted = None;
        self.preview = None;
        self.status = "等待预览…".into();
    }
    pub fn cancel(&mut self) {
        self.changed();
        self.font_generation += 1;
        self.text.clear();
        self.pending_font = None;
        self.pending_postscript = None;
        self.changed_at = None;
        self.pick_reference = false;
        self.status = "已取消，制造内容未改变".into();
    }
    pub fn anchor(&self) -> Result<MmPoint, String> {
        let (x, y) = if self.placement == Placement::Relative {
            if !self.has_reference {
                return Err("请先拾取或设置相对基点".into());
            }
            (
                number(&self.rx)? + number(&self.dx)?,
                number(&self.ry)? + number(&self.dy)?,
            )
        } else {
            (number(&self.x)?, number(&self.y)?)
        };
        if !x.is_finite() || !y.is_finite() {
            return Err("最终坐标溢出".into());
        }
        Ok(MmPoint::new(x, y))
    }
    pub fn set_mode(&mut self, mode: Placement) {
        if let Ok(p) = self.anchor() {
            self.x = p.x_mm.to_string();
            self.y = p.y_mm.to_string();
            if mode == Placement::Relative
                && self.has_reference
                && let (Ok(rx), Ok(ry)) = (number(&self.rx), number(&self.ry))
            {
                self.dx = (p.x_mm - rx).to_string();
                self.dy = (p.y_mm - ry).to_string();
            }
        }
        self.placement = mode;
        self.pick_reference = false;
        self.changed();
    }
    pub fn canvas_click(&mut self, p: MmPoint) {
        if self.placement == Placement::Relative && self.pick_reference {
            self.rx = p.x_mm.to_string();
            self.ry = p.y_mm.to_string();
            self.has_reference = true;
            self.pick_reference = false;
        } else if self.placement == Placement::Mouse {
            self.x = p.x_mm.to_string();
            self.y = p.y_mm.to_string();
        } else {
            return;
        }
        self.changed();
    }
    pub fn params(&self, layer: &str) -> Result<TextParams, String> {
        let font = self.font.as_ref().ok_or("请先选择并验证字体")?;
        let anchor = self.anchor()?;
        Ok(TextParams {
            layer_id: layer.into(),
            font: font.identity.clone(),
            layout: TextLayout {
                text: self.text.clone(),
                x_mm: anchor.x_mm,
                y_mm: anchor.y_mm,
                height_mm: number(&self.height)?,
                outline_offset_mm: number(&self.offset)?,
                tracking_mm: number(&self.tracking)?,
                rotation_deg: number(&self.rotation)?,
                curve_tolerance_mm: number(&self.tolerance)?,
                h_align: self.h_align,
                v_align: self.v_align,
            },
        })
    }
    pub fn ready(&self, now: Instant) -> bool {
        !self.text.trim().is_empty()
            && self.submitted != Some(self.generation)
            && self
                .changed_at
                .is_some_and(|t| now.duration_since(t) >= DEBOUNCE)
    }
    pub fn queue_font(&mut self, path: PathBuf) {
        self.font_generation += 1;
        self.font_path = Some(path.clone());
        self.pending_font = Some(path);
        self.pending_postscript = None;
        self.font = None;
        self.changed();
    }
    pub fn accept_font(&mut self, font: FontInfo) {
        self.recent.retain(|f| {
            f.identity.path != font.identity.path
                || f.identity.face_index != font.identity.face_index
        });
        self.recent.insert(0, font.clone());
        self.recent.truncate(RECENT_LIMIT);
        self.face = font.identity.face_index;
        self.font = Some(font);
        self.changed();
    }
    pub fn matches(
        &self,
        r: &Request,
        view: &crate::state::View,
        layer: Option<&str>,
        active: bool,
    ) -> bool {
        active
            && r.generation == self.generation
            && layer == Some(r.params.layer_id.as_str())
            && view
                .info
                .as_ref()
                .is_some_and(|d| d.document_id == r.document && d.revision == r.revision)
            && view
                .layers
                .iter()
                .any(|l| l.layer_id == r.params.layer_id && l.visible && !l.locked)
            && self
                .font
                .as_ref()
                .is_some_and(|f| f.identity == r.params.font)
    }
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        camera: crate::camera::Camera,
        rect: egui::Rect,
    ) {
        self.candidates = 0;
        let Some(preview) = &self.preview else {
            return;
        };
        let mut mesh = egui::Mesh::default();
        for geometry in &preview.geometries {
            let SemanticGeometry::Region { contours } = geometry else {
                continue;
            };
            for contour in contours {
                let world: Vec<_> = contour
                    .edges
                    .iter()
                    .filter_map(|e| match e {
                        RegionEdge::Line { start, .. } => Some(*start),
                        _ => None,
                    })
                    .collect();
                if world.len() < 3 {
                    continue;
                }
                let lo = camera.world(rect.left_bottom(), rect);
                let hi = camera.world(rect.right_top(), rect);
                if world.iter().all(|p| p.x_mm < lo.x_mm)
                    || world.iter().all(|p| p.x_mm > hi.x_mm)
                    || world.iter().all(|p| p.y_mm < lo.y_mm)
                    || world.iter().all(|p| p.y_mm > hi.y_mm)
                {
                    continue;
                }
                let points: Vec<_> = world.into_iter().map(|p| camera.screen(p, rect)).collect();
                self.candidates += 1;
                let base = mesh.vertices.len() as u32;
                for p in &points {
                    mesh.colored_vertex(
                        *p,
                        egui::Color32::from_rgba_unmultiplied(70, 240, 180, 150),
                    );
                }
                for i in 1..points.len() - 1 {
                    mesh.add_triangle(base, base + i as u32, base + i as u32 + 1);
                }
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placements_preserve_exact_absolute_anchor_and_require_explicit_reference() {
        let mut d = Draft::default();
        d.canvas_click(MmPoint::new(1.23456789, -2.34567891));
        let absolute = d.anchor().unwrap();
        d.set_mode(Placement::Absolute);
        assert_eq!(d.anchor().unwrap(), absolute);
        d.set_mode(Placement::Relative);
        assert!(d.anchor().is_err());
        d.pick_reference = true;
        d.canvas_click(MmPoint::new(1., -2.));
        d.dx = "0.23456789".into();
        d.dy = "-0.34567891".into();
        assert!(d.anchor().unwrap().distance_mm(absolute) < 1e-14);
        d.set_mode(Placement::Absolute);
        assert!(d.anchor().unwrap().distance_mm(absolute) < 1e-14);
        // This draft has no dependency on global Drag/Grid settings.
        assert!(!d.snap_text);
    }
    #[test]
    fn debounce_generation_cancel_and_recent_bound() {
        let mut d = Draft {
            text: "口8".into(),
            ..Default::default()
        };
        d.changed();
        let changed = d.changed_at.unwrap();
        assert!(!d.ready(changed + DEBOUNCE / 2));
        assert!(d.ready(changed + DEBOUNCE));
        d.submitted = Some(d.generation);
        assert!(!d.ready(changed + DEBOUNCE));
        let previous_generation = d.generation;
        d.changed();
        assert!(d.generation > previous_generation);
        for n in 0..12 {
            d.accept_font(FontInfo {
                family: format!("font{n}"),
                subfamily: "Regular".into(),
                identity: editor_service::FontIdentity {
                    path: format!("font{n}.ttf"),
                    sha256: "0".repeat(64),
                    face_index: 0,
                    license_status: "test identity".into(),
                    redistribution_allowed: false,
                },
            });
        }
        assert_eq!(d.recent.len(), 8);
        assert_eq!(d.recent[0].family, "font11");
        d.cancel();
        assert!(d.text.is_empty() && d.preview.is_none() && d.changed_at.is_none());
    }
}
