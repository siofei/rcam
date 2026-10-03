//! Transient world-point drafts. Formatting is never a geometry round trip.
use crate::state::{View, selection_geometry_identity};
use editor_core::{MmPoint, snap::SnapResolution};
use editor_service::{CompositeMaterial, SelectionMaterialResult, task::TaskVersion};

#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub version: TaskVersion,
    pub selection_epoch: u64,
}
impl Context {
    pub fn capture(view: &View) -> Self {
        Self {
            version: TaskVersion::capture(
                view.info.as_ref(),
                view.task_generation,
                view.rule_revision,
            ),
            selection_epoch: view.selection_epoch,
        }
    }
    pub fn valid(&self, view: &View) -> bool {
        *self == Self::capture(view)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Numeric,
    BoundingCenter,
    AreaCentroid { error_mm: f64 },
    Feature(editor_core::snap::SnapKind),
    Grid,
    Raw,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub world_mm: MmPoint,
    pub source: Source,
}
#[derive(Clone)]
pub struct Draft {
    pub x: String,
    pub y: String,
    pub value: Point,
    shown: (String, String),
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            x: "0".into(),
            y: "0".into(),
            shown: ("0".into(), "0".into()),
            value: Point {
                world_mm: MmPoint::new(0., 0.),
                source: Source::Numeric,
            },
        }
    }
}
impl Draft {
    pub fn set(&mut self, value: Point, unit: editor_core::units::DisplayUnit) {
        self.x = unit.input(value.world_mm.x_mm);
        self.y = unit.input(value.world_mm.y_mm);
        self.shown = (self.x.clone(), self.y.clone());
        self.value = value;
    }
    pub fn resolve(&self, unit: editor_core::units::DisplayUnit) -> Result<Point, String> {
        if self.shown == (self.x.clone(), self.y.clone()) {
            return Ok(self.value.clone());
        }
        let world_mm = MmPoint::new(unit.parse_length(&self.x)?, unit.parse_length(&self.y)?);
        if !world_mm.is_valid_geometry() {
            return Err("坐标必须有限且在制造范围内".into());
        }
        Ok(Point {
            world_mm,
            source: Source::Numeric,
        })
    }
}
pub fn center(view: &View, area: bool) -> Result<Point, String> {
    if view.selection_geometry_identity != selection_geometry_identity(view) {
        return Err("正在等待当前选区中心；旧结果不可用".into());
    }
    let info = view.info.as_ref().ok_or("没有工程")?;
    let result = view.selection_geometry.as_ref().ok_or_else(|| {
        view.selection_geometry_error
            .clone()
            .unwrap_or("正在计算选区中心".into())
    })?;
    if result.document_id != info.document_id
        || result.computed_revision != info.revision
        || result.resolution_mm != info.manufacturing_precision.resolution_mm
    {
        return Err("选区中心已过期".into());
    }
    from_centers(result, area)
}
pub fn from_centers(
    result: &editor_service::SelectionCentersResult,
    area: bool,
) -> Result<Point, String> {
    if area {
        match &result.material {
            SelectionMaterialResult::Computed {
                value:
                    CompositeMaterial::Ready {
                        centroid_mm,
                        centroid_error_mm,
                        ..
                    },
            } => Ok(Point {
                world_mm: *centroid_mm,
                source: Source::AreaCentroid {
                    error_mm: *centroid_error_mm,
                },
            }),
            SelectionMaterialResult::Computed {
                value: CompositeMaterial::ZeroArea,
            } => Err("选中材料面积为零；面积中心不可用".into()),
            SelectionMaterialResult::Unavailable { error } => {
                Err(format!("面积中心不可用：{error:?}"))
            }
        }
    } else {
        Ok(Point {
            world_mm: result.bounding_center_mm.ok_or("没有有效制造边界")?,
            source: Source::BoundingCenter,
        })
    }
}
pub fn snapped(value: &SnapResolution) -> Point {
    Point {
        world_mm: value.point,
        source: value
            .kind
            .map(Source::Feature)
            .unwrap_or(if value.from_grid {
                Source::Grid
            } else {
                Source::Raw
            }),
    }
}
pub enum Event {
    None,
    Pick,
}
pub fn controls_tagged(
    ui: &mut eframe::egui::Ui,
    draft: &mut Draft,
    view: &View,
    unit: editor_core::units::DisplayUnit,
    tag: &str,
) -> Event {
    let event = controls_with_centers_tagged(
        ui,
        draft,
        unit,
        center(view, false),
        center(view, true),
        tag,
    );
    ui.small("中心仅计算选中对象；各层按原 Dark/Clear 合成，跨层按面积加权");
    event
}
pub fn controls_with_centers(
    ui: &mut eframe::egui::Ui,
    draft: &mut Draft,
    unit: editor_core::units::DisplayUnit,
    bounds: Result<Point, String>,
    area: Result<Point, String>,
) -> Event {
    controls_with_centers_tagged(ui, draft, unit, bounds, area, "adapter")
}
pub fn controls_with_centers_tagged(
    ui: &mut eframe::egui::Ui,
    draft: &mut Draft,
    unit: editor_core::units::DisplayUnit,
    bounds: Result<Point, String>,
    area: Result<Point, String>,
    _tag: &str,
) -> Event {
    for (label, text) in [("X", &mut draft.x), ("Y", &mut draft.y)] {
        ui.horizontal(|ui| {
            ui.label(format!("{label} {}", unit.suffix()));
            let _response = ui
                .add(eframe::egui::TextEdit::singleline(text).desired_width(ui.available_width()));
            #[cfg(feature = "internal-evidence")]
            crate::native_i1::widget(&format!("{_tag}-{label}"), &_response);
        });
    }
    let mut event = Event::None;
    ui.horizontal_wrapped(|ui| {
        let pick = ui.button("画布拾取");
        #[cfg(feature = "internal-evidence")]
        crate::native_i1::widget(&format!("{_tag}-pick"), &pick);
        if pick.clicked() {
            event = Event::Pick;
        }
        for (point, label) in [(bounds, "几何中心"), (area, "面积中心")] {
            let response = ui.add_enabled(point.is_ok(), eframe::egui::Button::new(label));
            #[cfg(feature = "internal-evidence")]
            crate::native_i1::widget(
                &format!(
                    "{_tag}-{}",
                    if label == "面积中心" {
                        "area"
                    } else {
                        "bounds"
                    }
                ),
                &response,
            );
            if response.clicked() {
                draft.set(point.unwrap(), unit);
            } else if let Err(error) = point {
                response.on_hover_text(error);
            }
        }
    });
    match draft.resolve(unit) {
        Ok(point) => {
            let source = match point.source {
                Source::Numeric => "数值输入".into(),
                Source::BoundingCenter => "制造边界框中心".into(),
                Source::AreaCentroid { error_mm } => {
                    format!("材料面积中心（误差界 ≤ {error_mm:.6} mm）")
                }
                Source::Feature(kind) => format!("制造轮廓特征 {kind:?}"),
                Source::Grid => "网格".into(),
                Source::Raw => "画布坐标".into(),
            };
            ui.label(format!("来源：{source}"));
        }
        Err(error) => {
            ui.colored_label(eframe::egui::Color32::YELLOW, error);
        }
    }
    event
}
