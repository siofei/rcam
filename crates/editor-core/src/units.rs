//! Display conversion only. Manufacturing coordinates always remain f64 mm.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayUnit {
    #[default]
    Millimeter,
    Inch,
    Mil,
    Micrometer,
}
impl DisplayUnit {
    pub const ALL: [Self; 4] = [Self::Millimeter, Self::Inch, Self::Mil, Self::Micrometer];
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Millimeter => "mm",
            Self::Inch => "inch",
            Self::Mil => "mil",
            Self::Micrometer => "µm",
        }
    }
    pub fn mm_per_unit(self) -> f64 {
        match self {
            Self::Millimeter => 1.,
            Self::Inch => 25.4,
            Self::Mil => 0.0254,
            Self::Micrometer => 0.001,
        }
    }
    pub fn mm_to_display(self, value: f64) -> f64 {
        value / self.mm_per_unit()
    }
    pub fn display_to_mm(self, value: f64) -> f64 {
        value * self.mm_per_unit()
    }
    pub fn digits(self, resolution_mm: f64) -> usize {
        (-self.mm_to_display(resolution_mm).log10())
            .ceil()
            .clamp(0., 15.) as usize
    }
    pub fn format_length(self, mm: f64, resolution_mm: f64) -> String {
        format!(
            "{:.*} {}",
            self.digits(resolution_mm),
            self.mm_to_display(mm),
            self.suffix()
        )
    }
    pub fn format_area(self, mm2: f64, resolution_mm: f64) -> String {
        format!(
            "{:.*} {}²",
            (2 * self.digits(resolution_mm)).min(15),
            mm2 / self.mm_per_unit().powi(2),
            self.suffix()
        )
    }
    /// Suffix wins over the active unit. Never accept nonfinite/overflow input.
    pub fn parse_length(self, input: &str) -> Result<f64, String> {
        let input = input.trim();
        let (number, unit) = [
            ("inch", Self::Inch),
            ("mil", Self::Mil),
            ("mm", Self::Millimeter),
            ("in", Self::Inch),
            ("um", Self::Micrometer),
            ("µm", Self::Micrometer),
            ("μm", Self::Micrometer),
        ]
        .into_iter()
        .find_map(|(suffix, unit)| input.strip_suffix(suffix).map(|s| (s.trim(), unit)))
        .unwrap_or((input, self));
        let value = number
            .parse::<f64>()
            .map_err(|_| "请输入有限长度，可附 mm/inch/mil/µm".to_string())?;
        let mm = unit.display_to_mm(value);
        if !value.is_finite() || !mm.is_finite() {
            return Err("长度溢出或不是有限值".into());
        }
        Ok(mm)
    }
    /// Retained input drafts use roundtrip digits, never display rounding.
    pub fn input(self, mm: f64) -> String {
        self.mm_to_display(mm).to_string()
    }
    pub fn point_label(self, p: crate::MmPoint, resolution_mm: f64) -> String {
        format!(
            "X {}  Y {}",
            self.format_length(p.x_mm, resolution_mm),
            self.format_length(p.y_mm, resolution_mm)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManufacturingPrecision {
    pub resolution_mm: f64,
}
impl Default for ManufacturingPrecision {
    fn default() -> Self {
        Self {
            resolution_mm: 0.0001,
        }
    }
}
impl ManufacturingPrecision {
    pub fn validate(self) -> Result<Self, String> {
        // FS 6.6 cannot encode a finer grid or fractional micro-nanometres.
        let ticks = self.resolution_mm * 1e6;
        if !self.resolution_mm.is_finite()
            || !(1e-6..=1.).contains(&self.resolution_mm)
            || (ticks - ticks.round()).abs() > 1e-8
        {
            return Err("制造分辨率须为 0.001–1000 µm，且为 0.001 µm 的整数倍（FS 6.6）".into());
        }
        Ok(self)
    }
    pub fn text_tolerance_mm(self) -> f64 {
        // Resolution selects the output lattice, not the Bezier approximation
        // error. Keep the certified historical ceiling; finer grids refine it.
        (self.resolution_mm * 2.5).clamp(0.00001, 0.00025)
    }
}
/// Nearest grid point, ties away from zero; reject an unrepresentable grid index.
pub fn quantize_mm(value: f64, resolution_mm: f64) -> Result<f64, String> {
    if !value.is_finite() || !resolution_mm.is_finite() || resolution_mm <= 0. {
        return Err("invalid quantization input".into());
    }
    let index = value / resolution_mm;
    if !index.is_finite() || index.abs() >= 2f64.powi(52) {
        return Err("quantization index overflow".into());
    }
    let result = index.round() * resolution_mm;
    if !result.is_finite() {
        return Err("quantization overflow".into());
    }
    Ok(if result == 0. { 0. } else { result })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_parser_formatter_and_quantization() {
        for unit in DisplayUnit::ALL {
            for value in [-25.4, 0., 0.0001, 25.4, 1e200] {
                let result = unit.display_to_mm(unit.mm_to_display(value));
                assert!((result - value).abs() <= value.abs() * 3e-16);
            }
            assert_eq!(unit.parse_length("1inch").unwrap(), 25.4);
            assert_eq!(unit.parse_length("-10mil").unwrap(), -0.254);
            assert_eq!(unit.parse_length("100µm").unwrap(), 0.1);
            assert_eq!(unit.parse_length("10").unwrap(), unit.display_to_mm(10.));
            assert!(unit.parse_length("NaN").is_err());
            assert!(
                unit.format_area(1., 0.0001)
                    .ends_with(&format!("{}²", unit.suffix()))
            );
        }
        assert_eq!(DisplayUnit::ALL.map(|u| u.digits(0.0001)), [4, 6, 3, 1]);
        for resolution in [0.0001, 0.0005, 0.001, 0.002] {
            assert!(
                ManufacturingPrecision {
                    resolution_mm: resolution
                }
                .validate()
                .is_ok()
            );
            for index in [0., 0.5, 1.5, 12345.678] {
                let positive = quantize_mm(index * resolution, resolution).unwrap();
                assert_eq!(
                    quantize_mm(-index * resolution, resolution).unwrap(),
                    -positive
                );
                assert_eq!(quantize_mm(positive, resolution).unwrap(), positive);
            }
        }
        assert_eq!(quantize_mm(0.5, 1.).unwrap(), 1.);
        assert!(quantize_mm(f64::MAX, 0.0001).is_err());
        for r in [0., -1., f64::NAN, 0.0000001] {
            assert!(
                ManufacturingPrecision { resolution_mm: r }
                    .validate()
                    .is_err()
            );
        }
    }
}
