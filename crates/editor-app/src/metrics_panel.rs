use crate::state::View;
use editor_service::MetricValue;
/// Presentation only; the worker queries ApplicationService when selection or
/// manufacturing identity changes. The UI never computes manufacturing metrics.
pub fn lines(view: &View, unit: crate::tools::DisplayUnit, resolution: f64) -> Vec<String> {
    let count = view.selected.ordered.len();
    if count == 0 {
        return vec![];
    }
    if let Some(error) = &view.metrics_error {
        return vec!["面积/周长：暂不可精确计算".into(), format!("原因：{error}")];
    }
    if view.metrics.len() != count {
        return vec!["面积/周长：计算中…".into()];
    }
    if count == 1 {
        return match &view.metrics[0].value {
            MetricValue::Exact {
                area_mm2,
                perimeter_mm,
            } => vec![
                format!("面积：{}", unit.format_area(*area_mm2, resolution)),
                format!("周长：{}", unit.format_length(*perimeter_mm, resolution)),
            ],
            MetricValue::Unsupported { reason } => vec![
                "面积/周长：暂不可精确计算".into(),
                format!("原因：{reason}"),
            ],
        };
    }
    let (mut exact, mut area, mut perimeter) = (0, 0., 0.);
    for item in &view.metrics {
        if let MetricValue::Exact {
            area_mm2,
            perimeter_mm,
        } = item.value
        {
            exact += 1;
            area += area_mm2;
            perimeter += perimeter_mm;
        }
    }
    let suffix = if exact < count {
        "（已精确项）"
    } else {
        ""
    };
    let mut lines = vec![
        format!("已精确：{exact} / {count}"),
        format!(
            "对象面积合计{suffix}：{}",
            unit.format_area(area, resolution)
        ),
        format!(
            "对象制造边界周长合计{suffix}（含孔边）：{}",
            unit.format_length(perimeter, resolution)
        ),
    ];
    if exact < count {
        lines.push(format!("{} 个对象暂不可计算", count - exact));
    }
    lines.push("对象独立指标合计，未进行图层曝光布尔去重".into());
    lines
}
