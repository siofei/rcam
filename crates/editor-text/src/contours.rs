//! Local holes use Gerber cut-ins. Circular fitting is bounded against every
//! polygon segment, never a finite sampling claim about an unchecked curve.
use super::*;
use editor_core::{ArcDirection, ArcGeometry};
fn area(p: &[Point]) -> f64 {
    p.windows(2)
        .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
        .sum::<f64>()
        * 0.5
}
fn inside(p: Point, ring: &[Point]) -> bool {
    ring.windows(2)
        .filter(|w| {
            (w[0][1] > p[1]) != (w[1][1] > p[1])
                && p[0] < w[0][0] + (p[1] - w[0][1]) * (w[1][0] - w[0][0]) / (w[1][1] - w[0][1])
        })
        .count()
        % 2
        == 1
}
/// Clipper's normalized outer boundaries are positive; holes are negative.
/// Attach holes left-to-right using horizontal visibility rays, including earlier
/// holes. This keeps all retraced connectors parallel as required by the core.
pub(super) fn join(contours: Vec<Vec<Point>>) -> Result<Vec<Vec<Point>>, TextError> {
    let mut solids: Vec<_> = contours.iter().filter(|p| area(p) > 0.).cloned().collect();
    let mut holes: Vec<_> = contours.into_iter().filter(|p| area(p) < 0.).collect();
    let mut used_y: Vec<f64> = solids
        .iter()
        .chain(holes.iter())
        .flat_map(|p| p.iter().map(|p| p[1]))
        .collect();
    for h in &mut holes {
        let edge = h
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0][1] != w[1][1])
            .min_by(|(_, a), (_, b)| a[0][0].min(a[1][0]).total_cmp(&b[0][0].min(b[1][0])))
            .map(|(i, _)| i)
            .ok_or(TextError::InvalidTopology)?;
        let mut fraction = 0.381966011250105;
        let y = loop {
            let y = h[edge][1] + fraction * (h[edge + 1][1] - h[edge][1]);
            if !used_y.iter().any(|a| (a - y).abs() < 1e-8) {
                break y;
            }
            fraction *= 0.731;
            if fraction < 0.01 {
                return Err(TextError::InvalidTopology);
            }
        };
        used_y.push(y);
        let (at, p) = h
            .windows(2)
            .enumerate()
            .filter(|(_, w)| (w[0][1] > y) != (w[1][1] > y))
            .map(|(i, w)| {
                (
                    i,
                    [
                        w[0][0] + (y - w[0][1]) * (w[1][0] - w[0][0]) / (w[1][1] - w[0][1]),
                        y,
                    ],
                )
            })
            .min_by(|a, b| a.1[0].total_cmp(&b.1[0]))
            .ok_or(TextError::InvalidTopology)?;
        h.insert(at + 1, p);
        h.pop();
        h.rotate_left(at + 1);
        h.push(h[0]);
    }
    holes.sort_by(|a, b| {
        a[0][0]
            .total_cmp(&b[0][0])
            .then(a[0][1].total_cmp(&b[0][1]))
    });
    for h in holes {
        let p = h[0];
        let parent = solids
            .iter()
            .enumerate()
            .filter(|(_, s)| inside(p, s))
            .min_by(|(_, a), (_, b)| area(a).abs().total_cmp(&area(b).abs()))
            .map(|(i, _)| i)
            .ok_or(TextError::InvalidTopology)?;
        let s = &mut solids[parent];
        let hit = s
            .windows(2)
            .enumerate()
            .filter_map(|(i, w)| {
                if (w[0][1] > p[1]) == (w[1][1] > p[1]) {
                    return None;
                }
                let x = w[0][0] + (p[1] - w[0][1]) * (w[1][0] - w[0][0]) / (w[1][1] - w[0][1]);
                (x < p[0]).then_some((i, [x, p[1]]))
            })
            .max_by(|a, b| a.1[0].total_cmp(&b.1[0]))
            .ok_or(TextError::InvalidTopology)?;
        let (i, q) = hit;
        let mut insert = vec![q];
        insert.extend(h);
        insert.push(q);
        s.splice(i + 1..i + 1, insert);
        s.dedup();
    }
    Ok(solids)
}
fn mm(p: Point) -> MmPoint {
    MmPoint::new(p[0], p[1])
}
fn arc(points: &[Point], tolerance: f64, quantum: f64) -> Option<RegionEdge> {
    if points.len() < 4 {
        return None;
    }
    let a = points[0];
    let b = points[points.len() / 2];
    let c = *points.last()?;
    let u = [b[0] - a[0], b[1] - a[1]];
    let v = [c[0] - a[0], c[1] - a[1]];
    let cross = u[0] * v[1] - u[1] * v[0];
    if cross.abs() < 1e-16 {
        return None;
    }
    let uu = u[0] * u[0] + u[1] * u[1];
    let vv = v[0] * v[0] + v[1] * v[1];
    let center = [
        a[0] + (uu * v[1] - vv * u[1]) / (2. * cross),
        a[1] + (u[0] * vv - v[0] * uu) / (2. * cross),
    ];
    let base = [
        (center[0] / quantum).round() * quantum,
        (center[1] / quantum).round() * quantum,
    ];
    let center = (-3..=3)
        .flat_map(|x| {
            (-3..=3).map(move |y| [base[0] + x as f64 * quantum, base[1] + y as f64 * quantum])
        })
        .min_by(|a0, b0| {
            (distance(a, *a0) - distance(c, *a0))
                .abs()
                .total_cmp(&(distance(a, *b0) - distance(c, *b0)).abs())
        })?;
    if (distance(a, center) - distance(c, center)).abs() > 0.0000001 {
        return None;
    }
    let radius = distance(a, center);
    if !radius.is_finite() || radius > 1e6 {
        return None;
    }
    let sign = cross.signum();
    let mut sweep = 0.;
    for w in points.windows(2) {
        let u = [w[0][0] - center[0], w[0][1] - center[1]];
        let v = [w[1][0] - center[0], w[1][1] - center[1]];
        let angle = (sign * (u[0] * v[1] - u[1] * v[0])).atan2(u[0] * v[0] + u[1] * v[1]);
        if angle <= 0. || angle > 0.3 {
            return None;
        }
        sweep += angle;
        // Norm along a segment has its minimum at the projection of the center
        // and its maximum at an endpoint. Monotone angle proves reverse coverage.
        let lo = segment_distance(center, w[0], w[1]);
        let hi = distance(center, w[0]).max(distance(center, w[1]));
        if (radius - lo).abs().max((hi - radius).abs()) > tolerance {
            return None;
        }
    }
    if sweep > std::f64::consts::PI {
        return None;
    }
    // Do not smooth a corner or a curvature sign change (including a cut-in).
    if points.windows(3).any(|w| {
        let u = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
        let v = [w[2][0] - w[1][0], w[2][1] - w[1][1]];
        let turn = (sign * (u[0] * v[1] - u[1] * v[0])).atan2(u[0] * v[0] + u[1] * v[1]);
        if quantum <= 1e-6 {
            turn <= 1e-10 || turn > 0.3
        } else {
            turn.abs() > 0.3
        }
    }) {
        return None;
    }
    let geometry = ArcGeometry {
        start: mm(a),
        end: mm(c),
        center: mm(center),
        direction: if sign > 0. {
            ArcDirection::CounterClockwise
        } else {
            ArcDirection::Clockwise
        },
        full_circle: false,
        source: None,
    };
    geometry.is_valid().then_some(RegionEdge::Arc(geometry))
}
#[cfg(test)]
pub(super) fn fit(
    p: &[Point],
    tolerance: f64,
    work: &mut usize,
) -> Result<Vec<RegionEdge>, TextError> {
    fit_on_grid(p, tolerance, work, 1e-6)
}
pub(super) fn fit_on_grid(
    p: &[Point],
    tolerance: f64,
    work: &mut usize,
    quantum: f64,
) -> Result<Vec<RegionEdge>, TextError> {
    let mut result = vec![];
    let mut counts = std::collections::HashMap::new();
    for p in p {
        *counts
            .entry((p[0].to_bits(), p[1].to_bits()))
            .or_insert(0usize) += 1;
    }
    let repeated: Vec<_> = p
        .iter()
        .map(|p| counts[&(p[0].to_bits(), p[1].to_bits())] > 1)
        .collect();
    let mut i = 0;
    while i + 1 < p.len() {
        let mut end = i + 1;
        let stop = (i + 1..p.len() - 1)
            .find(|&j| repeated[j])
            .unwrap_or(p.len() - 1);
        // Exact collinear reduction only. No unconstrained RDP topology change.
        while end < stop
            && distance(p[i], p[end + 1]) > distance(p[i], p[end])
            && p[i + 1..=end]
                .iter()
                .all(|q| segment_distance(*q, p[i], p[end + 1]) < 1e-12)
        {
            end += 1;
        }
        let mut edge = RegionEdge::Line {
            start: mm(p[i]),
            end: mm(p[end]),
        };
        if end == i + 1 && !matches!(result.last(), Some(RegionEdge::Arc(_))) {
            for next in i + 3..=stop {
                if quantum > 1e-6 && next > i + 64 {
                    break;
                }
                *work += next - i + 1;
                if *work > MAX_WORK {
                    return Err(TextError::ResourceLimit);
                }
                if let Some(candidate) = arc(&p[i..=next], tolerance, quantum) {
                    let previous = if i > 0 {
                        Some(RegionEdge::Line {
                            start: mm(p[i - 1]),
                            end: mm(p[i]),
                        })
                    } else {
                        None
                    };
                    let following = p.get(next + 1).map(|q| RegionEdge::Line {
                        start: mm(p[next]),
                        end: mm(*q),
                    });
                    if previous
                        .as_ref()
                        .is_some_and(|e| !editor_core::region_join_is_simple(e, &candidate))
                        || following
                            .as_ref()
                            .is_some_and(|e| !editor_core::region_join_is_simple(&candidate, e))
                    {
                        if quantum > 1e-6 {
                            continue;
                        }
                        break;
                    }
                    edge = candidate;
                    end = next;
                } else if quantum <= 1e-6 {
                    break;
                }
            }
        }
        result.push(edge);
        i = end;
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circle_refit_and_line_preservation() {
        let p: Vec<_> = (0..=100)
            .map(|i| {
                let a = i as f64 / 100.;
                [a.cos(), a.sin()]
            })
            .collect();
        let edges = fit(&p, 0.0001, &mut 0).unwrap();
        assert_eq!(edges.len(), 1);
        assert!(matches!(edges[0], RegionEdge::Arc(_)));
        let p = vec![[0., 0.], [1., 0.], [2., 0.], [2., 1.]];
        assert_eq!(fit(&p, 0.0001, &mut 0).unwrap().len(), 2);
    }
}

#[cfg(test)]
mod certification_tests {
    use super::*;
    #[test]
    fn quadratic_cubic_inflection_and_near_line_dense_independent_bounds() {
        for controls in [
            vec![[0., 0.], [1., 2.], [3., 0.]],
            vec![[1., 0.], [1., 0.55228475], [0.55228475, 1.], [0., 1.]],
            vec![[0., 0.], [1., 2.], [2., -2.], [3., 0.]],
            vec![[0., 0.], [1., 0.0000001], [2., 0.], [3., 0.]],
        ] {
            let mut outline = Outline::new(TOLERANCE_MM * 0.4);
            outline.contours.push(vec![controls[0]]);
            outline.curve(&controls, 0);
            let points: Vec<_> = outline.contours[0]
                .iter()
                .map(|p| [(p[0] * 1e6).round() / 1e6, (p[1] * 1e6).round() / 1e6])
                .collect();
            let edges = fit(&points, TOLERANCE_MM * 0.5, &mut 0).unwrap();
            for i in 0..=10000 {
                let t = i as f64 / 10000.;
                let mut p = controls.clone();
                while p.len() > 1 {
                    p = p
                        .windows(2)
                        .map(|w| {
                            [
                                w[0][0] * (1. - t) + w[1][0] * t,
                                w[0][1] * (1. - t) + w[1][1] * t,
                            ]
                        })
                        .collect();
                }
                let error = edges
                    .iter()
                    .map(|e| crate::tests::edge_distance(mm(p[0]), e))
                    .fold(f64::INFINITY, f64::min);
                assert!(error <= TOLERANCE_MM, "certified curve error {error}");
            }
            if controls[1][1] == 0.0000001 {
                assert!(edges.iter().all(|e| matches!(e, RegionEdge::Line { .. })));
            } else {
                assert!(edges.iter().any(|e| matches!(e, RegionEdge::Arc(_))));
            }
        }
    }
    #[test]
    fn local_hole_component_boundary_metrics_exclude_connectors() {
        let outer = vec![[0., 0.], [4., 0.], [4., 4.], [0., 4.], [0., 0.]];
        let hole = vec![[1., 1.], [1., 3.], [3., 3.], [3., 1.], [1., 1.]];
        let rings = join(vec![outer, hole]).unwrap();
        assert_eq!(rings.len(), 1);
        let c = RegionContour {
            role: RegionRole::Solid,
            edges: fit(&rings[0], TOLERANCE_MM, &mut 0).unwrap(),
        };
        editor_core::validate_region_contour(&c).unwrap();
        let doc = editor_core::SemanticDocument {
            id: "test".into(),
            unit: "mm".into(),
            format: editor_core::SemanticFormat {
                integer: 6,
                decimal: 6,
                absolute: true,
                leading_zero_omission: true,
            },
            layers: vec![],
            apertures: vec![],
            source: Default::default(),
            block_definitions: vec![],
        };
        let m = editor_core::metrics::calculate(
            &doc,
            &SemanticGeometry::Region { contours: vec![c] },
            &mut 0,
        )
        .unwrap();
        assert!((m.area_mm2 - 12.).abs() < 1e-10);
        assert!((m.perimeter_mm - 24.).abs() < 1e-10);
    }
}
