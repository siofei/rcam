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
    pub floating: Option<MmPoint>,
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
            floating: None,
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
        self.floating = None;
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
        if self.floating.is_some() && std::env::var_os("RCAM_INTERACTION_LOG").is_some() {
            eprintln!("text_placement_cancel generation={}", self.generation);
        }

        self.changed();
        self.font_generation += 1;
        self.text.clear();
        self.pending_apply = None;
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
    pub fn start_placement(&mut self) -> bool {
        let Some(p) = &self.preview else {
            return false;
        };
        if std::env::var_os("RCAM_INTERACTION_LOG").is_some() {
            eprintln!(
                "text_placement_begin generation={} objects={}",
                self.generation,
                p.geometries.len()
            );
        }
        self.submitted = Some(self.generation);
        self.changed_at = None;
        self.floating = Some(MmPoint::new(p.params.layout.x_mm, p.params.layout.y_mm));
        true
    }
    pub fn placement_request(&self) -> Option<Request> {
        let p = self.preview.as_ref()?;
        let mut params = p.params.clone();
        if let Some(anchor) = self.floating {
            params.layout.x_mm = anchor.x_mm;
            params.layout.y_mm = anchor.y_mm;
        }
        Some(Request {
            generation: self.generation,
            document: p.document_id.clone(),
            revision: p.revision.clone(),
            params,
        })
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
        let origin = MmPoint::new(preview.params.layout.x_mm, preview.params.layout.y_mm);
        let anchor = self.floating.unwrap_or(origin);
        let screen = |p: MmPoint| {
            camera.screen(
                MmPoint::new(
                    p.x_mm + anchor.x_mm - origin.x_mm,
                    p.y_mm + anchor.y_mm - origin.y_mm,
                ),
                rect,
            )
        };
        let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(70, 240, 180));
        for geometry in &preview.geometries {
            let SemanticGeometry::Region { contours } = geometry else {
                continue;
            };
            for contour in contours {
                self.candidates += 1;
                for edge in &contour.edges {
                    match edge {
                        RegionEdge::Line { start, end } => {
                            if contour.edges.iter().any(|other| matches!(other,RegionEdge::Line{start:a,end:b} if a==end && b==start)) {continue;}
                            painter.line_segment([screen(*start), screen(*end)], stroke);
                        }
                        RegionEdge::Arc(arc) => {
                            let sweep = arc.sweep_radians().unwrap_or(0.);
                            let step = 4.
                                * (0.2 / (camera.scale * 2. * arc.radius()))
                                    .min(1.)
                                    .sqrt()
                                    .asin();
                            let count = (sweep / step).ceil().clamp(1., 4096.) as usize;
                            let start = (arc.start.y_mm - arc.center.y_mm)
                                .atan2(arc.start.x_mm - arc.center.x_mm);
                            let direction = if arc.direction == editor_core::ArcDirection::Clockwise
                            {
                                -1.
                            } else {
                                1.
                            };
                            let points = (0..=count)
                                .map(|i| {
                                    let a = start + direction * sweep * i as f64 / count as f64;
                                    screen(MmPoint::new(
                                        arc.center.x_mm + arc.radius() * a.cos(),
                                        arc.center.y_mm + arc.radius() * a.sin(),
                                    ))
                                })
                                .collect();
                            painter.add(egui::Shape::line(points, stroke));
                        }
                    }
                }
            }
        }
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
