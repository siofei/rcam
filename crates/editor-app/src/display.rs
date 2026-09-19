//! Display-only analytic primitives and adaptive Region contours. Never writer input.
use editor_core::*;
use editor_service::{LayerInfo, RenderSnapshot};
use std::collections::HashMap;
use std::f64::consts::TAU;

const MAX_ITEMS: usize = 200_000;
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Object {
    pub meta: [u32; 4],
    pub bounds: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Primitive {
    pub meta: [u32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
}
#[derive(Clone)]
pub struct Scene {
    pub serial: u64,
    pub anchor: MmPoint,
    pub objects: Vec<Object>,
    pub primitives: Vec<Primitive>,
    pub points: Vec<[f32; 2]>,
    pub ids: Vec<String>,
    pub ppm: f64,
}
fn rotate(p: MmPoint, deg: f64) -> MmPoint {
    let (s, c) = deg.to_radians().sin_cos();
    MmPoint::new(c * p.x_mm - s * p.y_mm, s * p.x_mm + c * p.y_mm)
}
fn transform(p: MmPoint, center: MmPoint, t: LocalTransform) -> MmPoint {
    let mut p = p;
    if matches!(t.mirror, Mirror::X | Mirror::Xy) {
        p.x_mm = -p.x_mm;
    }
    if matches!(t.mirror, Mirror::Y | Mirror::Xy) {
        p.y_mm = -p.y_mm;
    }
    let p = rotate(p, t.rotation_deg);
    MmPoint::new(
        center.x_mm + p.x_mm * t.scale,
        center.y_mm + p.y_mm * t.scale,
    )
}
fn rectangle(c: MmPoint, w: f64, h: f64) -> Vec<MmPoint> {
    vec![
        MmPoint::new(c.x_mm - w / 2., c.y_mm - h / 2.),
        MmPoint::new(c.x_mm + w / 2., c.y_mm - h / 2.),
        MmPoint::new(c.x_mm + w / 2., c.y_mm + h / 2.),
        MmPoint::new(c.x_mm - w / 2., c.y_mm + h / 2.),
    ]
}
impl Scene {
    pub fn build(
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
        anchor: MmPoint,
        ppm: f64,
        serial: u64,
    ) -> Result<Self, String> {
        if !ppm.is_finite() || ppm <= 0. {
            return Err("VALIDATION_FAILED: invalid display scale".into());
        }
        let mut scene = Self {
            serial,
            anchor,
            objects: vec![],
            primitives: vec![],
            points: vec![],
            ids: vec![],
            ppm,
        };
        let apertures: HashMap<_, _> = snapshot
            .apertures
            .iter()
            .map(|a| (&a.id, &a.shape))
            .collect();
        for (layer_index, layer) in snapshot.layers.iter().enumerate() {
            let ws = layers
                .iter()
                .find(|l| l.layer_id == layer.id)
                .ok_or("NOT_FOUND: layer workspace")?;
            // Validate hidden layers too: hiding cannot bypass display support checks.
            for object in &layer.objects {
                let start = scene.primitives.len();
                match &object.geometry {
                    SemanticGeometry::Flash {
                        center,
                        aperture_id,
                        transform: t,
                    } => {
                        let shape = apertures
                            .get(aperture_id)
                            .ok_or("NOT_FOUND: display aperture")?;
                        scene.flash(shape, *center, *t)?;
                    }
                    SemanticGeometry::Line {
                        start,
                        end,
                        width_mm,
                    } => scene.capsule(*start, *end, *width_mm / 2., Exposure::Dark)?,
                    SemanticGeometry::RectangularSweep {
                        start,
                        end,
                        width_mm,
                        height_mm,
                    } => {
                        if start.x_mm != end.x_mm && start.y_mm != end.y_mm {
                            return Err(
                                "UNSUPPORTED_FEATURE: oblique rectangular display sweep".into()
                            );
                        }
                        let center = MmPoint::new(
                            (start.x_mm + end.x_mm) / 2.,
                            (start.y_mm + end.y_mm) / 2.,
                        );
                        scene.polygon(
                            &rectangle(
                                center,
                                (end.x_mm - start.x_mm).abs() + width_mm,
                                (end.y_mm - start.y_mm).abs() + height_mm,
                            ),
                            Exposure::Dark,
                            true,
                        )?;
                    }
                    SemanticGeometry::Arc { path, width_mm } => {
                        if !path.is_valid() {
                            return Err("VALIDATION_FAILED: display arc".into());
                        }
                        if path.zero_sweep() {
                            scene.capsule(path.start, path.end, *width_mm / 2., Exposure::Dark)?;
                        } else {
                            let c = path.canonical_circle();
                            scene.capsule(path.start, c.start, *width_mm / 2., Exposure::Dark)?;
                            scene.capsule(c.end, path.end, *width_mm / 2., Exposure::Dark)?;
                            let center = scene.point(c.center)?;
                            let angle =
                                (c.start.y_mm - c.center.y_mm).atan2(c.start.x_mm - c.center.x_mm);
                            let sweep = c.sweep_radians().ok_or("VALIDATION_FAILED: arc sweep")?;
                            scene.primitives.push(Primitive {
                                meta: [2, 1, 0, 0],
                                a: [
                                    center[0],
                                    center[1],
                                    scene.scalar(c.radius())?,
                                    scene.scalar(*width_mm / 2.)?,
                                ],
                                b: [
                                    angle as f32,
                                    sweep as f32,
                                    if c.direction == ArcDirection::Clockwise {
                                        -1.
                                    } else {
                                        1.
                                    },
                                    0.,
                                ],
                            });
                        }
                    }
                    SemanticGeometry::Region { contours } => {
                        for contour in contours {
                            let contour = canonical_region_contour(contour)
                                .map_err(|e| format!("VALIDATION_FAILED: {e}"))?;
                            let mut points = Vec::new();
                            for edge in &contour.edges {
                                match edge {
                                    RegionEdge::Line { start, .. } => points.push(*start),
                                    RegionEdge::Arc(a) => {
                                        let sweep = a
                                            .sweep_radians()
                                            .ok_or("VALIDATION_FAILED: Region sweep")?;
                                        // Stable sagitta formula; <= 0.20 physical px, leaving conversion margin.
                                        let step = 4.
                                            * (0.20 / ppm / (2. * a.radius()))
                                                .min(1.)
                                                .sqrt()
                                                .asin();
                                        let count = (sweep / step).ceil().max(1.);
                                        if !count.is_finite() || count > MAX_ITEMS as f64 {
                                            return Err(
                                                "RESOURCE_LIMIT: Region display segments".into()
                                            );
                                        }
                                        let angle = (a.start.y_mm - a.center.y_mm)
                                            .atan2(a.start.x_mm - a.center.x_mm);
                                        let sign = if a.direction == ArcDirection::Clockwise {
                                            -1.
                                        } else {
                                            1.
                                        };
                                        for i in 0..count as usize {
                                            let t = angle + sign * sweep * i as f64 / count;
                                            points.push(MmPoint::new(
                                                a.center.x_mm + a.radius() * t.cos(),
                                                a.center.y_mm + a.radius() * t.sin(),
                                            ));
                                        }
                                    }
                                }
                                if points.len() > MAX_ITEMS {
                                    return Err("RESOURCE_LIMIT: Region display points".into());
                                }
                            }
                            scene.polygon(&points, Exposure::Dark, true)?;
                        }
                    }
                }
                if scene.primitives.len() + scene.points.len() > MAX_ITEMS {
                    return Err("RESOURCE_LIMIT: display items (200000)".into());
                }
                let end = scene.primitives.len();
                let bounds = scene.primitive_bounds(start, end);
                scene.objects.push(Object {
                    meta: [
                        start as u32,
                        end as u32,
                        u32::from(object.exposure == Exposure::Dark),
                        if ws.visible {
                            layer_index as u32 + 1
                        } else {
                            0
                        },
                    ],
                    bounds,
                });
                scene.ids.push(object.object_id.clone());
            }
        }
        Ok(scene)
    }
    pub fn scalar(&self, n: f64) -> Result<f32, String> {
        let f = n as f32;
        if !n.is_finite()
            || !f.is_finite()
            || (n != 0. && f == 0.)
            || ((f64::from(f) - n).abs() + n.abs() * f64::from(f32::EPSILON) * 4.) * self.ppm > 0.10
        {
            Err("UNSUPPORTED_FEATURE: display precision cannot preserve this view; zoom out".into())
        } else {
            Ok(f)
        }
    }
    fn check_budget(&self, extra: usize) -> Result<(), String> {
        if self
            .points
            .len()
            .saturating_add(self.primitives.len())
            .saturating_add(extra)
            > MAX_ITEMS
        {
            return Err("RESOURCE_LIMIT: display items (200000)".into());
        }
        Ok(())
    }
    fn point(&self, p: MmPoint) -> Result<[f32; 2], String> {
        Ok([
            self.scalar(p.x_mm - self.anchor.x_mm)?,
            self.scalar(p.y_mm - self.anchor.y_mm)?,
        ])
    }
    fn capsule(&mut self, a: MmPoint, b: MmPoint, r: f64, e: Exposure) -> Result<(), String> {
        self.check_budget(1)?;
        if !r.is_finite() || r <= 0. {
            return Err("VALIDATION_FAILED: display radius".into());
        }
        let a = self.point(a)?;
        let b = self.point(b)?;
        self.primitives.push(Primitive {
            meta: [0, u32::from(e == Exposure::Dark), 0, 0],
            a: [a[0], a[1], b[0], b[1]],
            b: [self.scalar(r)?, 0., 0., 0.],
        });
        Ok(())
    }
    fn polygon(&mut self, points: &[MmPoint], e: Exposure, winding: bool) -> Result<(), String> {
        self.check_budget(points.len().saturating_add(1))?;
        if points.len() < 3 {
            return Err("VALIDATION_FAILED: incomplete display polygon".into());
        }
        if self.points.len() + points.len() > MAX_ITEMS {
            return Err("RESOURCE_LIMIT: display points".into());
        }
        let start = self.points.len();
        for p in points {
            self.points.push(self.point(*p)?);
        }
        self.primitives.push(Primitive {
            meta: [
                1,
                u32::from(e == Exposure::Dark),
                start as u32,
                points.len() as u32,
            ],
            a: [u32::from(winding) as f32, 0., 0., 0.],
            b: [0.; 4],
        });
        Ok(())
    }
    fn flash(
        &mut self,
        shape: &ApertureShape,
        c: MmPoint,
        t: LocalTransform,
    ) -> Result<(), String> {
        if !t.scale.is_finite() || t.scale <= 0. {
            return Err("VALIDATION_FAILED: display transform".into());
        }
        let map = |p| transform(p, c, t);
        let poly = |s: &mut Self, p: Vec<MmPoint>, e| {
            s.polygon(&p.into_iter().map(map).collect::<Vec<_>>(), e, false)
        };
        let hole = match shape {
            ApertureShape::Circle {
                diameter_mm,
                hole_diameter_mm,
            } => {
                self.capsule(c, c, diameter_mm * t.scale / 2., Exposure::Dark)?;
                *hole_diameter_mm
            }
            ApertureShape::Rectangle {
                width_mm,
                height_mm,
                hole_diameter_mm,
            } => {
                poly(
                    self,
                    rectangle(MmPoint::new(0., 0.), *width_mm, *height_mm),
                    Exposure::Dark,
                )?;
                *hole_diameter_mm
            }
            ApertureShape::Obround {
                width_mm,
                height_mm,
                hole_diameter_mm,
            } => {
                let half = (width_mm - height_mm).abs() / 2.;
                let a = if width_mm >= height_mm {
                    MmPoint::new(half, 0.)
                } else {
                    MmPoint::new(0., half)
                };
                self.capsule(
                    map(a),
                    map(MmPoint::new(-a.x_mm, -a.y_mm)),
                    width_mm.min(*height_mm) * t.scale / 2.,
                    Exposure::Dark,
                )?;
                *hole_diameter_mm
            }
            ApertureShape::Polygon {
                diameter_mm,
                vertices,
                rotation_deg,
                hole_diameter_mm,
            } => {
                poly(
                    self,
                    (0..*vertices)
                        .map(|i| {
                            let a = TAU * f64::from(i) / f64::from(*vertices)
                                + rotation_deg.to_radians();
                            MmPoint::new(diameter_mm * a.cos() / 2., diameter_mm * a.sin() / 2.)
                        })
                        .collect(),
                    Exposure::Dark,
                )?;
                *hole_diameter_mm
            }
            ApertureShape::Macro { primitives } => {
                for p in primitives {
                    match p {
                        MacroPrimitive::Circle {
                            exposure,
                            diameter_mm,
                            center,
                            rotation_deg,
                        } => {
                            let c = map(rotate(*center, *rotation_deg));
                            self.capsule(c, c, diameter_mm * t.scale / 2., *exposure)?;
                        }
                        MacroPrimitive::CenterLine {
                            exposure,
                            width_mm,
                            height_mm,
                            center,
                            rotation_deg,
                        } => poly(
                            self,
                            rectangle(*center, *width_mm, *height_mm)
                                .into_iter()
                                .map(|p| rotate(p, *rotation_deg))
                                .collect(),
                            *exposure,
                        )?,
                        MacroPrimitive::Outline {
                            exposure,
                            points,
                            rotation_deg,
                        } => poly(
                            self,
                            points.iter().map(|p| rotate(*p, *rotation_deg)).collect(),
                            *exposure,
                        )?,
                    }
                }
                None
            }
        };
        if let Some(h) = hole {
            self.capsule(c, c, h * t.scale / 2., Exposure::Clear)?;
        }
        Ok(())
    }
    fn primitive_bounds(&self, start: usize, end: usize) -> [f32; 4] {
        let mut b = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        let mut add = |x: f32, y: f32| {
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x);
            b[3] = b[3].max(y);
        };
        for p in &self.primitives[start..end] {
            match p.meta[0] {
                0 => {
                    let r = p.b[0];
                    add(p.a[0] - r, p.a[1] - r);
                    add(p.a[0] + r, p.a[1] + r);
                    add(p.a[2] - r, p.a[3] - r);
                    add(p.a[2] + r, p.a[3] + r);
                }
                1 => {
                    for v in &self.points[p.meta[2] as usize..(p.meta[2] + p.meta[3]) as usize] {
                        add(v[0], v[1]);
                    }
                }
                _ => {
                    let r = p.a[2] + p.a[3];
                    add(p.a[0] - r, p.a[1] - r);
                    add(p.a[0] + r, p.a[1] + r);
                }
            }
        }
        b
    }
}
