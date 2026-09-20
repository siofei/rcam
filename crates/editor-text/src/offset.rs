//! Bounded material offset. Integer clipping is local to the glyph, before rotation.
use super::{MAX_EDGES, Point, TextError};
use clipper2_rust::{
    ClipperOffset, EndType, FillRule, JoinType, Paths64, Point64, area, union_subjects_64,
};
const SCALE: f64 = 100_000_000.;
const ARC_ERROR_MM: f64 = 0.0001;

pub(super) fn material(contours: &[Vec<Point>], delta: f64) -> Result<Vec<Vec<Point>>, TextError> {
    let count: usize = contours.iter().map(Vec::len).sum();
    if count > MAX_EDGES {
        return Err(TextError::ResourceLimit);
    }
    let mut paths: Paths64 = Vec::new();
    for contour in contours {
        if contour.len() < 4 || contour.first() != contour.last() {
            return Err(TextError::InvalidTopology);
        }
        let mut path = Vec::new();
        for p in &contour[..contour.len() - 1] {
            if p.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
                return Err(TextError::InvalidTopology);
            }
            path.push(Point64::new(
                (p[0] * SCALE).round() as i64,
                (p[1] * SCALE).round() as i64,
            ));
        }
        paths.push(path);
    }
    let normalized = union_subjects_64(&paths, FillRule::NonZero);
    if normalized.is_empty() {
        return Err(TextError::InvalidTopology);
    }
    let signature = |paths: &Paths64| {
        (
            paths.iter().filter(|p| area(p) > 0.).count(),
            paths.iter().filter(|p| area(p) < 0.).count(),
        )
    };
    let mut offset = ClipperOffset::new(2., ARC_ERROR_MM * SCALE, false, false);
    offset.add_paths(&normalized, JoinType::Round, EndType::Polygon);
    let mut result = Vec::new();
    offset.execute(delta * SCALE, &mut result);
    if offset.error_code() != 0
        || result.is_empty()
        || (delta < 0. && signature(&result) != signature(&normalized))
    {
        return Err(TextError::InvalidTopology);
    }
    if result.iter().map(Vec::len).sum::<usize>() > MAX_EDGES {
        return Err(TextError::ResourceLimit);
    }
    // Refuse erosion that loses a narrow protrusion: all original boundary
    // vertices must remain within the offset distance of a retained boundary.
    if delta < 0. {
        let mut work = 0usize;
        for path in &normalized {
            for (vertex, p) in path.iter().enumerate() {
                let a = path[(vertex + path.len() - 1) % path.len()];
                let b = path[(vertex + 1) % path.len()];
                let u = [(a.x - p.x) as f64, (a.y - p.y) as f64];
                let v = [(b.x - p.x) as f64, (b.y - p.y) as f64];
                let cosine = ((u[0] * v[0] + u[1] * v[1]) / (u[0].hypot(u[1]) * v[0].hypot(v[1])))
                    .clamp(-1., 1.);
                let half_sine = ((1. - cosine) * 0.5).sqrt();
                if !half_sine.is_finite() || half_sine < 0.1 {
                    return Err(TextError::InvalidTopology);
                }
                let mut nearest = f64::INFINITY;
                for path in &result {
                    for i in 0..path.len() {
                        work += 1;
                        if work > super::MAX_WORK {
                            return Err(TextError::ResourceLimit);
                        }
                        let a = path[i];
                        let b = path[(i + 1) % path.len()];
                        nearest = nearest.min(super::segment_distance(
                            [p.x as f64 / SCALE, p.y as f64 / SCALE],
                            [a.x as f64 / SCALE, a.y as f64 / SCALE],
                            [b.x as f64 / SCALE, b.y as f64 / SCALE],
                        ));
                    }
                }
                // At an interior corner theta, the exact eroded join can retreat
                // delta/sin(theta/2). A fixed sqrt(2) allowance wrongly rejects
                // ordinary non-right-angle font corners (e.g. Arial Unicode 回).
                if nearest > delta.abs() / half_sine + ARC_ERROR_MM + 0.0000001 {
                    return Err(TextError::InvalidTopology);
                }
            }
        }
    }
    Ok(result
        .into_iter()
        .map(|p| {
            let mut points: Vec<_> = p
                .into_iter()
                .map(|p| [p.x as f64 / SCALE, p.y as f64 / SCALE])
                .collect();
            points.push(points[0]);
            points
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn square_hole() -> Vec<Vec<Point>> {
        vec![
            vec![[0., 0.], [4., 0.], [4., 4.], [0., 4.], [0., 0.]],
            vec![[1., 1.], [1., 3.], [3., 3.], [3., 1.], [1., 1.]],
        ]
    }
    fn material_area(contours: &[Vec<Point>]) -> f64 {
        super::super::decompose(contours, &mut 0)
            .unwrap()
            .iter()
            .map(|p| {
                p.iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let b = p[(i + 1) % p.len()];
                        a[0] * b[1] - a[1] * b[0]
                    })
                    .sum::<f64>()
                    .abs()
                    * 0.5
            })
            .sum()
    }
    #[test]
    fn positive_offset_resolves_hole_disappearance_and_crossing_contours() {
        let closed = material(&square_hole(), 1.1).unwrap();
        assert_eq!(closed.len(), 1);
        assert!(material_area(&closed) > 16.);
        let crossing = vec![vec![[0., 0.], [2., 2.], [0., 2.], [2., 0.], [0., 0.]]];
        let resolved = material(&crossing, 0.1).unwrap();
        assert!(material_area(&resolved) > 2.);
    }
    #[test]
    fn material_area_and_holes_change_in_opposite_directions() {
        let original = square_hole();
        let bigger = material(&original, 0.1).unwrap();
        let smaller = material(&original, -0.1).unwrap();
        assert!(material_area(&bigger) > 12.);
        assert!(material_area(&smaller) < 12.);
        for (cs, expected_outer, expected_hole) in [(bigger, 4.2, 1.8), (smaller, 3.8, 2.2)] {
            let mut widths: Vec<_> = cs
                .iter()
                .map(|p| {
                    p.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max)
                        - p.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min)
                })
                .collect();
            widths.sort_by(f64::total_cmp);
            assert!((widths[0] - expected_hole).abs() < 0.00001);
            assert!((widths[1] - expected_outer).abs() < 0.00001);
        }
        assert_eq!(material(&original, -0.6), Err(TextError::InvalidTopology));
    }
}
