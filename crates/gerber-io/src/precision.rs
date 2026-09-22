//! Normalize a private export snapshot; never touch imported or editable state.
use editor_core::{
    units::{ManufacturingPrecision, quantize_mm},
    *,
};
pub fn normalize_manufacturing(
    document: &SemanticDocument,
    precision: ManufacturingPrecision,
) -> Result<SemanticDocument, String> {
    precision.validate()?;
    document.validate().map_err(|e| format!("{e:?}"))?;
    let q = |v: &mut f64| -> Result<(), String> {
        *v = quantize_mm(*v, precision.resolution_mm)?;
        Ok(())
    };
    let point = |p: &mut MmPoint| -> Result<(), String> {
        q(&mut p.x_mm)?;
        q(&mut p.y_mm)
    };
    let arc = |a: &mut ArcGeometry| -> Result<(), String> {
        let old = *a;
        point(&mut a.start)?;
        point(&mut a.end)?;
        point(&mut a.center)?;
        if a.zero_sweep() != old.zero_sweep()
            || a.full_circle != old.full_circle
            || a.has_nonsensical_center()
        {
            return Err("quantization changes arc topology".into());
        }
        // Retain the source declaration. The ordinary semantic/writer validators
        // must accept this arc; a coarse policy never loosens their thresholds.
        Ok(())
    };
    let mut result = document.clone();
    let normalize_shape = |shape: &mut ApertureShape, scale: f64| -> Result<(), String> {
        let q = |v: &mut f64| -> Result<(), String> {
            *v = quantize_mm(*v * scale, precision.resolution_mm)?;
            Ok(())
        };
        let point = |p: &mut MmPoint| -> Result<(), String> {
            q(&mut p.x_mm)?;
            q(&mut p.y_mm)
        };
        use ApertureShape::*;
        let hole = match shape {
            Circle {
                diameter_mm,
                hole_diameter_mm,
            }
            | Polygon {
                diameter_mm,
                hole_diameter_mm,
                ..
            } => {
                q(diameter_mm)?;
                hole_diameter_mm.as_mut()
            }
            Rectangle {
                width_mm,
                height_mm,
                hole_diameter_mm,
            }
            | Obround {
                width_mm,
                height_mm,
                hole_diameter_mm,
            } => {
                q(width_mm)?;
                q(height_mm)?;
                hole_diameter_mm.as_mut()
            }
            Macro { primitives } => {
                for primitive in primitives {
                    match primitive {
                        MacroPrimitive::Circle {
                            diameter_mm,
                            center,
                            ..
                        } => {
                            q(diameter_mm)?;
                            point(center)?;
                        }
                        MacroPrimitive::CenterLine {
                            width_mm,
                            height_mm,
                            center,
                            ..
                        } => {
                            q(width_mm)?;
                            q(height_mm)?;
                            point(center)?;
                        }
                        MacroPrimitive::Outline { points, .. } => {
                            for p in points {
                                point(p)?;
                            }
                        }
                    }
                }
                None
            }
        };
        if let Some(hole) = hole {
            q(hole)?;
        }
        Ok(())
    };
    for aperture in &mut result.apertures {
        normalize_shape(&mut aperture.shape, 1.)?;
    }
    let originals: std::collections::HashMap<_, _> = document
        .apertures
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    let mut scaled: std::collections::HashMap<(String, u64), String> =
        std::collections::HashMap::new();
    let mut next_dcode = result
        .apertures
        .iter()
        .map(|a| a.source_dcode)
        .max()
        .unwrap_or(9);
    for layer in &mut result.layers {
        for object in &mut layer.objects {
            match &mut object.geometry {
                SemanticGeometry::Flash {
                    center,
                    aperture_id,
                    transform,
                } => {
                    point(center)?;
                    if transform.scale != 1. {
                        let key = (aperture_id.clone(), transform.scale.to_bits());
                        let id = if let Some(id) = scaled.get(&key) {
                            id.clone()
                        } else {
                            let mut aperture = (*originals
                                .get(aperture_id.as_str())
                                .ok_or("missing aperture")?)
                            .clone();
                            normalize_shape(&mut aperture.shape, transform.scale)?;
                            let mut id = format!("{}-precision-{}", aperture_id, scaled.len());
                            while result.apertures.iter().any(|a| a.id == id) {
                                id.push('_');
                            }
                            aperture.id = id.clone();
                            next_dcode = next_dcode
                                .checked_add(1)
                                .ok_or("aperture DCode exhausted")?;
                            aperture.source_dcode = next_dcode;
                            result.apertures.push(aperture);
                            scaled.insert(key, id.clone());
                            id
                        };
                        *aperture_id = id;
                        transform.scale = 1.;
                    }
                }
                SemanticGeometry::Line {
                    start,
                    end,
                    width_mm,
                } => {
                    point(start)?;
                    point(end)?;
                    q(width_mm)?;
                }
                SemanticGeometry::RectangularSweep {
                    start,
                    end,
                    width_mm,
                    height_mm,
                } => {
                    point(start)?;
                    point(end)?;
                    q(width_mm)?;
                    q(height_mm)?;
                }
                SemanticGeometry::Arc { path, width_mm } => {
                    arc(path)?;
                    q(width_mm)?;
                }
                SemanticGeometry::Region { contours } => {
                    for contour in contours {
                        for edge in &mut contour.edges {
                            match edge {
                                RegionEdge::Line { start, end } => {
                                    let distinct = start != end;
                                    point(start)?;
                                    point(end)?;
                                    if distinct && start == end {
                                        return Err("quantization collapses a Region edge".into());
                                    }
                                }
                                RegionEdge::Arc(a) => arc(a)?,
                            }
                        }
                    }
                }
                // The instance's own placement quantizes like a Flash center.
                // `block_definitions` geometry is not re-quantized here: it is
                // captured from already-quantized world objects when a
                // definition is created (`blocks.create_definition_from_objects`),
                // so it only drifts if the project's resolution changes after
                // the fact — a known S4-B2 scope note (`docs/S4_B2_REVIEW.md`).
                SemanticGeometry::BlockInstance { transform, .. } => {
                    point(&mut transform.translation)?;
                }
            }
        }
    }
    result
        .validate()
        .map_err(|e| format!("quantized geometry rejected: {e:?}"))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_geometry_stays_exact_export_grid_is_idempotent_and_safe() {
        let input =
            b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.234567*%\nD10*\nX1234567Y-1234567D03*\nM02*\n";
        let original = crate::parse_s1(input, "precision").unwrap().document;
        for resolution in [0.0001, 0.0005, 0.001, 0.002] {
            let p = ManufacturingPrecision {
                resolution_mm: resolution,
            };
            let normalized = normalize_manufacturing(&original, p).unwrap();
            assert_eq!(normalize_manufacturing(&normalized, p).unwrap(), normalized);
            let SemanticGeometry::Flash { center, .. } = normalized.layers[0].objects[0].geometry
            else {
                panic!()
            };
            assert!((center.x_mm - 1.234567).abs() <= resolution / 2.);
            assert_eq!(center.y_mm, -center.x_mm);
            let bytes = crate::write_s1(&normalized).unwrap();
            let reparsed = crate::parse_s1(&bytes, "reopen").unwrap();
            let SemanticGeometry::Flash { center: actual, .. } =
                reparsed.document.layers[0].objects[0].geometry
            else {
                panic!()
            };
            assert!((actual.x_mm - center.x_mm).abs() < 1e-12);
        }
        let SemanticGeometry::Flash { center, .. } = original.layers[0].objects[0].geometry else {
            panic!()
        };
        assert_eq!(center.x_mm, 1.234567);
        let arc = b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX1000000Y0D02*\nG75*\nG03X0Y1000000I-1000000J0D01*\nM02*\n";
        let arc = crate::parse_s1(arc, "arc").unwrap().document;
        let q = normalize_manufacturing(&arc, ManufacturingPrecision::default()).unwrap();
        crate::write_s1(&q).unwrap();
        let mut tiny = arc.clone();
        tiny.layers[0].objects[0].geometry = SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: vec![
                    RegionEdge::Line {
                        start: MmPoint::new(0., 0.),
                        end: MmPoint::new(0.0002, 0.),
                    },
                    RegionEdge::Line {
                        start: MmPoint::new(0.0002, 0.),
                        end: MmPoint::new(0.0002, 1.),
                    },
                    RegionEdge::Line {
                        start: MmPoint::new(0.0002, 1.),
                        end: MmPoint::new(0., 1.),
                    },
                    RegionEdge::Line {
                        start: MmPoint::new(0., 1.),
                        end: MmPoint::new(0., 0.),
                    },
                ],
            }],
        };
        tiny.validate().unwrap();
        assert!(
            normalize_manufacturing(
                &tiny,
                ManufacturingPrecision {
                    resolution_mm: 0.001
                }
            )
            .is_err()
        );
    }
}
