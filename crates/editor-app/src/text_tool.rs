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
    pub(crate) unit_cache: Vec<(String, f64)>,
    pub display_unit: editor_core::units::DisplayUnit,
    pub floating: Option<MmPoint>,
    pub context: Option<(String, String, String, String)>,
    pub text: String,
    pub height: String,
    pub baseline_spacing: String,
    pub stroke_width: String,
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
            unit_cache: Vec::new(),
            display_unit: Default::default(),
            floating: None,
            context: None,
            text: String::new(),
            height: "3".into(),
            baseline_spacing: "0".into(),
            stroke_width: "0.15".into(),
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
            font: Some(editor_service::builtin_stroke_font()),
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
            status: "选择字体并输入文字".into(),
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
    pub fn change_unit(&mut self, unit: editor_core::units::DisplayUnit) -> Result<(), String> {
        if unit == self.display_unit {
            return Ok(());
        }
        let values = [
            &self.height,
            &self.baseline_spacing,
            &self.stroke_width,
            &self.offset,
            &self.tracking,
            &self.x,
            &self.y,
            &self.rx,
            &self.ry,
            &self.dx,
            &self.dy,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, v)| {
            self.unit_cache
                .get(index)
                .filter(|(text, _)| text == v)
                .map_or_else(|| self.display_unit.parse_length(v), |(_, mm)| Ok(*mm))
        })
        .collect::<Result<Vec<_>, _>>()?;
        self.unit_cache.clear();
        for (field, mm) in [
            &mut self.height,
            &mut self.baseline_spacing,
            &mut self.stroke_width,
            &mut self.offset,
            &mut self.tracking,
            &mut self.x,
            &mut self.y,
            &mut self.rx,
            &mut self.ry,
            &mut self.dx,
            &mut self.dy,
        ]
        .into_iter()
        .zip(values)
        {
            *field = unit.input(mm);
            self.unit_cache.push((field.clone(), mm));
        }
        self.display_unit = unit;
        Ok(())
    }
    fn length(&self, index: usize, text: &str) -> Result<f64, String> {
        self.unit_cache
            .get(index)
            .filter(|(shown, _)| shown == text)
            .map_or_else(|| self.display_unit.parse_length(text), |(_, mm)| Ok(*mm))
    }
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
    pub fn resume_dialog(&mut self) {
        if std::env::var_os("RCAM_INTERACTION_LOG").is_some() {
            eprintln!("text_placement_resume generation={}", self.generation);
        }
        self.floating = None;
        self.status = "已返回文字设置；草稿保留，制造内容未改变".into();
    }
    pub fn cancel(&mut self) {
        if self.floating.is_some() && std::env::var_os("RCAM_INTERACTION_LOG").is_some() {
            eprintln!("text_placement_cancel generation={}", self.generation);
        }

        self.changed();
        self.font_generation += 1;
        // Retain user input and formatting across placement, cancel and reopening.
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
                self.length(7, &self.rx)? + self.length(9, &self.dx)?,
                self.length(8, &self.ry)? + self.length(10, &self.dy)?,
            )
        } else {
            (self.length(5, &self.x)?, self.length(6, &self.y)?)
        };
        if !x.is_finite() || !y.is_finite() {
            return Err("最终坐标溢出".into());
        }
        Ok(MmPoint::new(x, y))
    }
    pub fn set_mode(&mut self, mode: Placement) {
        if let Ok(p) = self.anchor() {
            self.x = self.display_unit.input(p.x_mm);
            self.y = self.display_unit.input(p.y_mm);
            if mode == Placement::Relative
                && self.has_reference
                && let (Ok(rx), Ok(ry)) = (self.length(7, &self.rx), self.length(8, &self.ry))
            {
                self.dx = self.display_unit.input(p.x_mm - rx);
                self.dy = self.display_unit.input(p.y_mm - ry);
            }
        }
        self.placement = mode;
        self.pick_reference = false;
        self.changed();
    }
    pub fn canvas_click(&mut self, p: MmPoint) {
        if self.placement == Placement::Relative && self.pick_reference {
            self.rx = self.display_unit.input(p.x_mm);
            self.ry = self.display_unit.input(p.y_mm);
            self.has_reference = true;
            self.pick_reference = false;
        } else if self.placement == Placement::Mouse {
            self.x = self.display_unit.input(p.x_mm);
            self.y = self.display_unit.input(p.y_mm);
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
                height_mm: self.length(0, &self.height)?,
                baseline_spacing_mm: self.length(1, &self.baseline_spacing)?,
                stroke_width_mm: self.length(2, &self.stroke_width)?,
                outline_offset_mm: self.length(3, &self.offset)?,
                tracking_mm: self.length(4, &self.tracking)?,
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
                .any(|l| l.layer_id == r.params.layer_id && crate::state::text_target_ok(l))
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
            if let SemanticGeometry::Line {
                start,
                end,
                width_mm,
            } = geometry
            {
                self.candidates += 1;
                painter.line_segment(
                    [screen(*start), screen(*end)],
                    egui::Stroke::new((*width_mm * camera.scale) as f32, stroke.color),
                );
                continue;
            }
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
        let retained = d.text.clone();
        d.cancel();
        assert_eq!(d.text, retained);
        assert!(d.preview.is_none() && d.changed_at.is_none());
    }
}
