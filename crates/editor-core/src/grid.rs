//! Pure manufacturing-mm grid arithmetic; no document or view state.
use crate::CoreError;

/// Round to nearest grid point; exact half indices round away from the origin.
/// Reject scales where f64 cannot distinguish adjacent integer grid indices.
pub fn snap_scalar(value_mm: f64, spacing_mm: f64, origin_mm: f64) -> Result<f64, CoreError> {
    let invalid =
        || CoreError::InvalidGeometry("grid coordinate/spacing is invalid or unresolvable");
    if !value_mm.is_finite()
        || !origin_mm.is_finite()
        || !spacing_mm.is_finite()
        || spacing_mm <= 0.
    {
        return Err(invalid());
    }
    let index = (value_mm - origin_mm) / spacing_mm;
    if !index.is_finite() || index.abs() >= 2f64.powi(52) {
        return Err(invalid());
    }
    let result = index.round() * spacing_mm + origin_mm;
    if !result.is_finite() || result + spacing_mm == result || result - spacing_mm == result {
        return Err(invalid());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_origin_and_halfway() {
        for (value, spacing, origin, expected) in [
            (1.24, 0.5, 0., 1.),
            (-1.24, 0.5, 0., -1.),
            (1.25, 0.5, 0., 1.5),
            (-1.25, 0.5, 0., -1.5),
            (9.75, 0.5, 10., 9.5),
            (10.25, 0.5, 10., 10.5),
            (1e9 + 0.25, 0.5, 0., 1e9 + 0.5),
            (2e-200, 1e-200, 0., 2e-200),
        ] {
            assert_eq!(snap_scalar(value, spacing, origin).unwrap(), expected);
        }
    }
    #[test]
    fn fail_closed_boundaries() {
        for spacing in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(snap_scalar(1., spacing, 0.).is_err());
        }
        for (v, s, o) in [
            (f64::NAN, 1., 0.),
            (1., 1., f64::INFINITY),
            (f64::MAX, 1., -f64::MAX),
            (1e9, 1e-12, 0.),
            (1., f64::MIN_POSITIVE, 0.),
            (1e20, 1., 1e20),
        ] {
            assert!(snap_scalar(v, s, o).is_err());
        }
    }
}
