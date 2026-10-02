//! Display-only analytic primitives and adaptive Region contours. Never writer input.
use editor_core::workspace::{LayerDisplayMode, aperture_shape_map, classify_object};
use editor_core::*;
use editor_service::{LayerInfo, RenderSnapshot};
use std::collections::HashMap;
use std::f64::consts::TAU;

// One 230k-flash real stencil needs roughly 1.2M polygon vertices/primitives.
// This remains a display-only budget; the manufacturing object limit is separate.
const ACCELERATION_TARGET_ITEMS: usize = 2_000_000;
const POLYGON_BIN_THRESHOLD: usize = 16;
const POLYGON_BIN_TARGET_EDGES: usize = 1;
const POLYGON_BIN_MAX_COUNT: usize = 4096;
const POLYGON_BIN_MAX_STORAGE_MULTIPLIER: usize = 64;
type PolygonEdge = ([f32; 2], [f32; 2]);
type PolygonBins = (Vec<Vec<PolygonEdge>>, f32, f32, usize);
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Object {
    pub meta: [u32; 4],
    pub bounds: [f32; 4],
    /// View style: `[0x00RRGGBB, mode, 0, 0]`; mode 0 = Filled, 1 = boundary
    /// hairline, 2 = centre-line hairline. Never manufacturing data.
    pub style: [u32; 4],
}

/// Style mode of an object in the shader.
pub const MODE_FILLED: u32 = 0;
pub const MODE_EDGE: u32 = 1;
pub const MODE_CENTERLINE: u32 = 2;
/// Marker in `Primitive::meta[3]`: draw this stroke as a screen-stable hairline.
pub const HAIRLINE: u32 = 1;
/// Screen-stable hairline used for boundary and centre-line display, in physical pixels.
pub const HAIRLINE_PX: f64 = 1.0;
/// A scene stays valid while the view zoom is within `[render_ppm / LOD_MAX_ZOOM_OUT,
/// render_ppm]`; outside that range the display is rebuilt. Hairline bounds must cover
/// the coarsest supported zoom, otherwise zooming out clips the outer half of the line.
pub const LOD_MAX_ZOOM_OUT: f64 = 4.;

/// A refused request also covers its attempted camera range. A new pan/LOD
/// can recover; an unchanged camera must settle instead of scheduling a loop.
pub fn covers_view(bounds: BoundsMm, scale: f64, lo: MmPoint, hi: MmPoint, ppm: f64) -> bool {
    lo.x_mm >= bounds.min_x_mm
        && lo.y_mm >= bounds.min_y_mm
        && hi.x_mm <= bounds.max_x_mm
        && hi.y_mm <= bounds.max_y_mm
        && ppm <= scale
        && ppm >= scale / LOD_MAX_ZOOM_OUT
}

/// Geometry-only display semantics for `LayerDisplayMode::ZeroWidth`.
/// Category policy remains owned by the outer `SemanticObject`; in particular,
/// resolved Block primitives still use `DisplayClass::BlockInstance` for
/// colour, visibility, selection and locking.
fn is_stroke_geometry(geometry: &SemanticGeometry) -> bool {
    matches!(
        geometry,
        SemanticGeometry::Line { .. }
            | SemanticGeometry::RectangularSweep { .. }
            | SemanticGeometry::Arc { .. }
    )
}

pub fn pack_color(color: editor_core::workspace::Color) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Primitive {
    pub meta: [u32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub bounds: [f32; 4],
}
// Deterministic unit-test injection after real scene objects have been built.
// Thread-local and absent from every product/internal-evidence binary.
#[cfg(test)]
thread_local! { pub(crate) static CANCEL_SCENE_AFTER_OBJECTS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) }; }
#[derive(Clone)]
pub struct Scene {
    pub serial: u64,
    pub index: std::sync::Arc<crate::render_index::RenderIndex>,
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
    #[cfg(test)]
    pub fn build(
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
        anchor: MmPoint,
        ppm: f64,
        serial: u64,
    ) -> Result<Self, String> {
        Self::build_cached(
            snapshot,
            layers,
            anchor,
            ppm,
            serial,
            None,
            &mut crate::block_display::BlockDisplayCache::default(),
        )
    }
    pub fn build_cached(
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
        anchor: MmPoint,
        ppm: f64,
        serial: u64,
        previous: Option<&Scene>,
        block_cache: &mut crate::block_display::BlockDisplayCache,
    ) -> Result<Self, String> {
        Self::build_cached_with_cancel(
            snapshot,
            layers,
            anchor,
            ppm,
            serial,
            previous,
            block_cache,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn build_cached_with_cancel(
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
        anchor: MmPoint,
        ppm: f64,
        serial: u64,
        previous: Option<&Scene>,
        block_cache: &mut crate::block_display::BlockDisplayCache,
        cancel: Option<&editor_service::task::CancellationToken>,
    ) -> Result<Self, String> {
        if !ppm.is_finite() || ppm <= 0. {
            return Err("VALIDATION_FAILED: invalid display scale".into());
        }
        let mut scene = Self {
            index: Default::default(),
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
        let shape_map = aperture_shape_map(&snapshot.apertures);
        for (layer_index, layer) in snapshot.layers.iter().enumerate() {
            let ws = layers
                .iter()
                .find(|l| l.layer_id == layer.id)
                .ok_or("NOT_FOUND: layer workspace")?;
            let layer_visible = ws.visible && ws.effective_visible;
            let zero_width = ws.display_mode == LayerDisplayMode::ZeroWidth;
            // Validate hidden layers and hidden categories too: hiding cannot bypass display support checks.
            for object in &layer.objects {
                if let Some(cancel) = cancel {
                    cancel.checkpoint().map_err(|e| e.code)?;
                }
                scene.push_object(
                    object,
                    layer_index,
                    ws,
                    layer_visible,
                    zero_width,
                    &apertures,
                    &shape_map,
                    &snapshot.block_definitions,
                    block_cache,
                )?;
                #[cfg(test)]
                if let Some(cancel) = cancel
                    && CANCEL_SCENE_AFTER_OBJECTS
                        .get()
                        .is_some_and(|n| scene.objects.len() >= n)
                {
                    CANCEL_SCENE_AFTER_OBJECTS.set(None);
                    cancel.cancel();
                }
            }
        }
        if let Some(cancel) = cancel {
            cancel.checkpoint().map_err(|e| e.code)?;
        }
        scene.accelerate_polygons()?;
        scene.index = if let Some(old) = previous.filter(|old| {
            old.anchor == scene.anchor
                && old.ids == scene.ids
                && old.objects.len() == scene.objects.len()
                && old
                    .objects
                    .iter()
                    .zip(&scene.objects)
                    .all(|(a, b)| a.bounds == b.bounds && a.meta[3] == b.meta[3])
        }) {
            old.index.clone()
        } else {
            std::sync::Arc::new(crate::render_index::RenderIndex::build(
                &scene.objects,
                &[],
                [0.; 2],
            )?)
        };
        Ok(scene)
    }
    /// Reconvert only changed objects whose primitive/contour storage layout
    /// remains identical. All pointers and exposure positions stay in place;
    /// accelerated polygons and structural changes use the full build.
    #[allow(clippy::too_many_arguments)]
    pub fn patch(
        previous: &Scene,
        filtered: &RenderSnapshot,
        full: &RenderSnapshot,
        changed: &[(usize, usize)],
        layers: &[LayerInfo],
        anchor: MmPoint,
        ppm: f64,
        serial: u64,
    ) -> Option<Result<Self, String>> {
        if previous.anchor != anchor
            || previous.ppm != ppm
            || !full.block_definitions.is_empty()
            || previous.objects.len() != previous.ids.len()
            || !filtered
                .layers
                .iter()
                .flat_map(|l| &l.objects)
                .map(|o| &o.object_id)
                .eq(previous.ids.iter())
        {
            return None;
        }
        let changed_ids: std::collections::HashSet<_> = changed
            .iter()
            .map(|&(l, o)| full.layers[l].objects[o].object_id.as_str())
            .collect();
        let subset = RenderSnapshot {
            document_id: filtered.document_id.clone(),
            revision: filtered.revision.clone(),
            workspace_revision: filtered.workspace_revision.clone(),
            styles: filtered.styles.clone(),
            apertures: filtered.apertures.clone(),
            block_definitions: vec![],
            layers: filtered
                .layers
                .iter()
                .map(|l| SemanticLayer {
                    id: l.id.clone(),
                    objects: l
                        .objects
                        .iter()
                        .filter(|o| changed_ids.contains(o.object_id.as_str()))
                        .cloned()
                        .collect(),
                })
                .collect(),
        };
        let fresh = match Self::build_cached(
            &subset,
            layers,
            anchor,
            ppm,
            serial,
            None,
            &mut crate::block_display::BlockDisplayCache::default(),
        ) {
            Ok(s) => s,
            Err(e) => return Some(Err(e)),
        };
        let indices: HashMap<_, _> = previous
            .ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        // Preflight before copying: offsets can be reused only with the same raw
        // contour length. No unreferenced append-only storage accumulates.
        for (i, id) in fresh.ids.iter().enumerate() {
            let old = &previous.objects[indices[id.as_str()]];
            let next = &fresh.objects[i];
            if old.meta[1] - old.meta[0] != next.meta[1] - next.meta[0] {
                return None;
            }
            for (a, b) in previous.primitives[old.meta[0] as usize..old.meta[1] as usize]
                .iter()
                .zip(&fresh.primitives[next.meta[0] as usize..next.meta[1] as usize])
            {
                if a.meta[0] != b.meta[0]
                    || a.meta[0] == 3
                    || (a.meta[0] == 1 && a.meta[3] != b.meta[3])
                {
                    return None;
                }
            }
        }
        let mut result = previous.clone();
        result.serial = serial;
        let mut index_changed = false;
        for (i, id) in fresh.ids.iter().enumerate() {
            let index = indices[id.as_str()];
            let old = previous.objects[index];
            let mut next = fresh.objects[i];
            next.meta[0] = old.meta[0];
            next.meta[1] = old.meta[1];
            index_changed |= old.bounds != next.bounds || old.meta[3] != next.meta[3];
            result.objects[index] = next;
            for (j, b) in fresh.primitives
                [fresh.objects[i].meta[0] as usize..fresh.objects[i].meta[1] as usize]
                .iter()
                .enumerate()
            {
                let offset = old.meta[0] as usize + j;
                let a = previous.primitives[offset];
                let mut primitive = *b;
                if b.meta[0] == 1 {
                    let length = b.meta[3] as usize;
                    result.points[a.meta[2] as usize..a.meta[2] as usize + length].copy_from_slice(
                        &fresh.points[b.meta[2] as usize..b.meta[2] as usize + length],
                    );
                    primitive.meta[2] = a.meta[2];
                }
                result.primitives[offset] = primitive;
            }
        }
        if index_changed {
            result.index =
                match crate::render_index::RenderIndex::build(&result.objects, &[], [0.; 2]) {
                    Ok(index) => std::sync::Arc::new(index),
                    Err(e) => return Some(Err(e)),
                };
        }
        Some(Ok(result))
    }

    /// Push one manufacturing object. Category Visible/Color comes from the
    /// outer object, so a `BlockInstance` remains one atomic category. The
    /// ZeroWidth geometry mode is evaluated for every resolved primitive.
    #[allow(clippy::too_many_arguments)]
    fn push_object(
        &mut self,
        object: &SemanticObject,
        layer_index: usize,
        ws: &LayerInfo,
        layer_visible: bool,
        zero_width: bool,
        apertures: &HashMap<&String, &ApertureShape>,
        shape_map: &HashMap<&str, &ApertureShape>,
        block_definitions: &[editor_core::block::BlockDefinition],
        block_cache: &mut crate::block_display::BlockDisplayCache,
    ) -> Result<(), String> {
        let class = classify_object(object, shape_map);
        let class_style = ws.classes.iter().find(|c| c.class == class);
        let visible = layer_visible && class_style.is_none_or(|c| c.visible);
        let color = class_style.map_or(ws.base_color, |c| c.effective_color);
        let hairline = zero_width && is_stroke_geometry(&object.geometry);
        if let SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } = &object.geometry
        {
            let definition = block_definitions
                .iter()
                .find(|d| &d.id == definition_id)
                .ok_or("NOT_FOUND: block definition")?;
            let resolved = block_cache.resolve(definition, transform)?;
            for primitive in &resolved {
                let primitive_hairline = zero_width && is_stroke_geometry(&primitive.geometry);
                self.push_primitive_object(
                    &primitive.geometry,
                    primitive.exposure,
                    &object.object_id,
                    layer_index,
                    visible,
                    color,
                    primitive_hairline,
                    ws.display_mode,
                    apertures,
                )?;
            }
            return Ok(());
        }
        self.push_primitive_object(
            &object.geometry,
            object.exposure,
            &object.object_id,
            layer_index,
            visible,
            color,
            hairline,
            ws.display_mode,
            apertures,
        )
    }
    /// Push the primitives of one Flash/Line/RectangularSweep/Arc/Region and
    /// its one `Object` entry. Called once per ordinary manufacturing object,
    /// and once per resolved primitive of a `BlockInstance` (sharing that
    /// instance's `object_id`, so its selection halo covers every resolved
    /// primitive and `gpu::selection_flags` — which matches by id, not by
    /// index — flags every one of them without any special-casing).
    #[allow(clippy::too_many_arguments)]
    fn push_primitive_object(
        &mut self,
        geometry: &SemanticGeometry,
        exposure: Exposure,
        object_id: &str,
        layer_index: usize,
        visible: bool,
        color: editor_core::workspace::Color,
        hairline: bool,
        display_mode: LayerDisplayMode,
        apertures: &HashMap<&String, &ApertureShape>,
    ) -> Result<(), String> {
        let start = self.primitives.len();
        match geometry {
            SemanticGeometry::Flash {
                center,
                aperture_id,
                transform: t,
            } => {
                let shape = apertures
                    .get(aperture_id)
                    .ok_or("NOT_FOUND: display aperture")?;
                self.flash(shape, *center, *t)?;
            }
            SemanticGeometry::Line {
                start,
                end,
                width_mm,
            } => self.stroke(*start, *end, *width_mm / 2., hairline)?,
            SemanticGeometry::RectangularSweep {
                start,
                end,
                width_mm,
                height_mm,
            } => {
                if start.x_mm != end.x_mm && start.y_mm != end.y_mm {
                    return Err("UNSUPPORTED_FEATURE: oblique rectangular display sweep".into());
                }
                if hairline {
                    self.stroke(*start, *end, 1e-3, true)?;
                } else {
                    let center =
                        MmPoint::new((start.x_mm + end.x_mm) / 2., (start.y_mm + end.y_mm) / 2.);
                    self.polygon(
                        &rectangle(
                            center,
                            (end.x_mm - start.x_mm).abs() + width_mm,
                            (end.y_mm - start.y_mm).abs() + height_mm,
                        ),
                        Exposure::Dark,
                        true,
                    )?;
                }
            }
            SemanticGeometry::Arc { path, width_mm } => {
                if !path.is_valid() {
                    return Err("VALIDATION_FAILED: display arc".into());
                }
                if path.zero_sweep() {
                    self.stroke(path.start, path.end, *width_mm / 2., hairline)?;
                } else {
                    let c = path.canonical_circle();
                    self.stroke(path.start, c.start, *width_mm / 2., hairline)?;
                    self.stroke(c.end, path.end, *width_mm / 2., hairline)?;
                    let center = self.point(c.center)?;
                    let angle = (c.start.y_mm - c.center.y_mm).atan2(c.start.x_mm - c.center.x_mm);
                    let sweep = c.sweep_radians().ok_or("VALIDATION_FAILED: arc sweep")?;
                    self.primitives.push(Primitive {
                        meta: [2, 1, 0, if hairline { HAIRLINE } else { 0 }],
                        a: [
                            center[0],
                            center[1],
                            self.scalar(c.radius())?,
                            self.scalar(*width_mm / 2.)?,
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
                        bounds: [0.; 4],
                    });
                }
            }
            SemanticGeometry::Region { contours } => {
                for contour in contours {
                    let contour = derived_region_contour(contour)
                        .map_err(|e| format!("VALIDATION_FAILED: {e}"))?;
                    let mut points = Vec::new();
                    for edge in &contour.edges {
                        match edge {
                            RegionEdge::Line { start, .. } => points.push(*start),
                            RegionEdge::Arc(a) => {
                                let sweep =
                                    a.sweep_radians().ok_or("VALIDATION_FAILED: Region sweep")?;
                                // Stable sagitta formula; <= 0.20 physical px, leaving conversion margin.
                                let step = 4.
                                    * (0.20 / self.ppm / (2. * a.radius())).min(1.).sqrt().asin();
                                // Subpixel curved contours must still enclose
                                // an area: a circle cannot collapse to one point,
                                // nor an arc plus its closing line to two.
                                let count = (sweep / step.min(std::f64::consts::FRAC_PI_2))
                                    .ceil()
                                    .max(if contour.edges.len() < 3 { 2. } else { 1. });
                                if !count.is_finite() || count >= u32::MAX as f64 {
                                    return Err("DISPLAY_PRECISION: Region segment index is not representable".into());
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
                    }
                    self.polygon(&points, Exposure::Dark, true)?;
                }
            }
            // `resolve_instance` converts from `BlockObjectGeometry`, a
            // strictly smaller enum with no `BlockInstance` case (nesting is
            // rejected at the type level) — a resolved primitive can never
            // be a `BlockInstance`, and `push_object` never calls this
            // function directly with one either.
            SemanticGeometry::BlockInstance { .. } => {
                return Err("BUG: nested BlockInstance reached push_primitive_object".into());
            }
        }
        self.check_budget(0)?;
        let end = self.primitives.len();
        let mut bounds = self.primitive_bounds(start, end);
        // Use actual arc sweeps rather than the full-circle envelope of the
        // shader primitive. Short imported arcs otherwise occupy entire
        // index cells and can falsely exhaust the viewport work budget.
        if matches!(
            geometry,
            SemanticGeometry::Region { .. } | SemanticGeometry::Arc { .. }
        ) {
            let exact = geometries_bounds([geometry], &[])
                .map_err(|e| format!("VALIDATION_FAILED: {e}"))?
                .ok_or("VALIDATION_FAILED: empty arc/Region bounds")?;
            let lo = self.point(MmPoint::new(exact.min_x_mm, exact.min_y_mm))?;
            let hi = self.point(MmPoint::new(exact.max_x_mm, exact.max_y_mm))?;
            let pad = if matches!(geometry, SemanticGeometry::Arc { .. }) {
                f32::EPSILON * 16. * lo.into_iter().chain(hi).map(f32::abs).fold(1., f32::max)
            } else {
                0.
            };
            bounds = [
                lo[0].next_down() - pad,
                lo[1].next_down() - pad,
                hi[0].next_up() + pad,
                hi[1].next_up() + pad,
            ];
        }
        let mode = match (display_mode, hairline) {
            (LayerDisplayMode::Filled, _) => MODE_FILLED,
            (LayerDisplayMode::ZeroWidth, true) => MODE_CENTERLINE,
            _ => MODE_EDGE,
        };
        if mode != MODE_FILLED {
            // Hairlines extend about a pixel beyond the exact geometry.
            let pad = (2. * HAIRLINE_PX * LOD_MAX_ZOOM_OUT / self.ppm) as f32;
            bounds = [
                bounds[0] - pad,
                bounds[1] - pad,
                bounds[2] + pad,
                bounds[3] + pad,
            ];
        }
        self.objects.push(Object {
            meta: [
                start as u32,
                end as u32,
                u32::from(exposure == Exposure::Dark),
                if visible { layer_index as u32 + 1 } else { 0 },
            ],
            bounds,
            style: [pack_color(color), mode, 0, 0],
        });
        self.ids.push(object_id.into());
        Ok(())
    }
    pub fn scalar(&self, n: f64) -> Result<f32, String> {
        let f = n as f32;
        if !n.is_finite()
            || !f.is_finite()
            || (n != 0. && f == 0.)
            || ((f64::from(f) - n).abs() + n.abs() * f64::from(f32::EPSILON) * 4.) * self.ppm > 0.10
        {
            Err("DISPLAY_PRECISION: local coordinate error exceeds 0.1 physical pixel".into())
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
            >= (1 << 24)
        {
            return Err(
                "DISPLAY_PRECISION: display storage index is not exactly representable".into(),
            );
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
        self.capsule_marked(a, b, r, e, 0)
    }
    /// A stroke: its swept capsule, or in ZeroWidth a screen-stable centre-line
    /// hairline. Both validate the true width so display support checks are identical.
    fn stroke(&mut self, a: MmPoint, b: MmPoint, r: f64, hairline: bool) -> Result<(), String> {
        if hairline {
            if !r.is_finite() || r <= 0. {
                return Err("VALIDATION_FAILED: display radius".into());
            }
            // The radius is replaced by the shader; keep it representable.
            self.capsule_marked(a, b, r.min(1e-3), Exposure::Dark, HAIRLINE)
        } else {
            self.capsule(a, b, r, Exposure::Dark)
        }
    }
    fn capsule_marked(
        &mut self,
        a: MmPoint,
        b: MmPoint,
        r: f64,
        e: Exposure,
        mark: u32,
    ) -> Result<(), String> {
        self.check_budget(1)?;
        if !r.is_finite() || r <= 0. {
            return Err("VALIDATION_FAILED: display radius".into());
        }
        let a = self.point(a)?;
        let b = self.point(b)?;
        self.primitives.push(Primitive {
            meta: [0, u32::from(e == Exposure::Dark), 0, mark],
            a: [a[0], a[1], b[0], b[1]],
            b: [self.scalar(r)?, 0., 0., 0.],
            bounds: [0.; 4],
        });
        Ok(())
    }
    fn polygon(&mut self, points: &[MmPoint], e: Exposure, winding: bool) -> Result<(), String> {
        self.check_budget(points.len().saturating_add(1))?;
        if points.len() < 3 {
            return Err("VALIDATION_FAILED: incomplete display polygon".into());
        }
        let local = points
            .iter()
            .map(|point| self.point(*point))
            .collect::<Result<Vec<_>, _>>()?;
        let start = self.points.len();
        self.points.extend_from_slice(&local);
        self.primitives.push(Primitive {
            meta: [
                1,
                u32::from(e == Exposure::Dark),
                start as u32,
                points.len() as u32,
            ],
            a: [u32::from(winding) as f32, 0., 0., 0.],
            b: [0.; 4],
            bounds: std::array::from_fn(|k| {
                let axis = k % 2;
                if k < 2 {
                    local
                        .iter()
                        .map(|p| p[axis])
                        .fold(f32::INFINITY, f32::min)
                        .next_down()
                } else {
                    local
                        .iter()
                        .map(|p| p[axis])
                        .fold(f32::NEG_INFINITY, f32::max)
                        .next_up()
                }
            }),
        });
        Ok(())
    }
    /// Reserve every raw contour before spending any memory on acceleration.
    /// Share the remaining fixed budget by area times edge count, so early layers
    /// cannot consume storage required by later layers. No vertices are dropped.
    fn accelerate_polygons(&mut self) -> Result<(), String> {
        let _timing = rcam_diagnostics::Timing::start("scene.accelerate_polygons");
        let weight = |p: &Primitive, points: &[[f32; 2]]| {
            if p.meta[0] != 1 || (p.meta[3] as usize) < POLYGON_BIN_THRESHOLD {
                return 0.;
            }
            let vertices = &points[p.meta[2] as usize..(p.meta[2] + p.meta[3]) as usize];
            let bounds = vertices.iter().fold(
                [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ],
                |mut b, v| {
                    for k in 0..2 {
                        b[k] = b[k].min(f64::from(v[k]));
                        b[k + 2] = b[k + 2].max(f64::from(v[k]));
                    }
                    b
                },
            );
            (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]) * f64::from(p.meta[3])
        };
        let total: f64 = self
            .primitives
            .iter()
            .map(|p| weight(p, &self.points))
            .sum();
        if total <= 0. {
            return Ok(());
        }
        let extra =
            ACCELERATION_TARGET_ITEMS.saturating_sub(self.points.len() + self.primitives.len());
        let raw = std::mem::take(&mut self.points);
        for primitive in &mut self.primitives {
            if primitive.meta[0] != 1 {
                continue;
            }
            let local =
                &raw[primitive.meta[2] as usize..(primitive.meta[2] + primitive.meta[3]) as usize];
            let budget = (extra as f64 * weight(primitive, &raw) / total).floor() as usize;
            let start = self.points.len();
            primitive.meta[2] = start as u32;
            self.points.extend_from_slice(local);
            let Some((bins, y_min, inverse_height, max_edges)) = Self::polygon_bins(local, budget)
            else {
                continue;
            };
            let x_min = local.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
            let x_max = local.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
            let headers = self.points.len();
            let left_headers = headers + bins.len();
            self.points.resize(headers + bins.len() * 2, [0.; 2]);
            for (index, mut bin) in bins.into_iter().enumerate() {
                bin.sort_by(|(a, b), (c, d)| c[0].max(d[0]).total_cmp(&a[0].max(b[0])));
                self.points[headers + index] = [self.points.len() as f32, bin.len() as f32];
                for &(a, b) in &bin {
                    self.points.push(a);
                    self.points.push(b);
                }
                bin.sort_by(|(a, b), (c, d)| a[0].min(b[0]).total_cmp(&c[0].min(d[0])));
                self.points[left_headers + index] = [self.points.len() as f32, bin.len() as f32];
                for (a, b) in bin {
                    self.points.push(a);
                    self.points.push(b);
                }
            }
            primitive.meta[0] = 3;
            primitive.a[1] = y_min;
            primitive.a[2] = inverse_height;
            primitive.b = [
                (left_headers - headers) as f32,
                max_edges as f32,
                x_min + (x_max - x_min) * 0.5,
                left_headers as f32,
            ];
        }
        self.check_budget(0)
    }
    /// Exact point-in-polygon acceleration for display only. A horizontal ray
    /// can only cross edges whose y-range contains the sample, so duplicating
    /// those edges into bounded y bins preserves winding/even-odd answers while
    /// avoiding a full glyph-contour scan for every screen sample.
    fn polygon_bins(points: &[[f32; 2]], available: usize) -> Option<PolygonBins> {
        if points.len() < POLYGON_BIN_THRESHOLD {
            return None;
        }
        let y_min = points
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let y_max = points
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let height = y_max - y_min;
        if !height.is_finite() || height <= 0. {
            return None;
        }
        let mut count = points
            .len()
            .div_ceil(POLYGON_BIN_TARGET_EDGES)
            .clamp(2, POLYGON_BIN_MAX_COUNT);
        loop {
            let inverse_height = count as f32 / height;
            let mut bins = vec![Vec::new(); count];
            for index in 0..points.len() {
                let a = points[index];
                let b = points[(index + 1) % points.len()];
                if a[1] == b[1] {
                    continue;
                }
                let bin = |y: f32| {
                    (((y - y_min) * inverse_height).floor().max(0.) as usize).min(count - 1)
                };
                let first = bin(a[1].min(b[1]));
                let last = bin(a[1].max(b[1]));
                for edges in &mut bins[first..=last] {
                    edges.push((a, b));
                }
            }
            let references = bins.iter().map(Vec::len).sum::<usize>();
            let storage = count
                .saturating_mul(2)
                .saturating_add(references.saturating_mul(4));
            let max_edges = bins.iter().map(Vec::len).max().unwrap_or(0);
            if storage
                <= points
                    .len()
                    .saturating_mul(POLYGON_BIN_MAX_STORAGE_MULTIPLIER)
                && storage <= available
                && max_edges < points.len()
            {
                return Some((bins, y_min, inverse_height, max_edges));
            }
            if count <= 2 {
                return None;
            }
            count = count.div_ceil(2);
        }
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
                1 | 3 => {
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

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn raw_display_storage_above_old_budget_is_accepted() {
        let mut scene = Scene {
            serial: 1,
            index: Default::default(),
            anchor: MmPoint::new(0., 0.),
            objects: vec![],
            primitives: vec![],
            points: vec![[0.; 2]; ACCELERATION_TARGET_ITEMS + 1],
            ids: vec![],
            ppm: 20.,
        };
        scene
            .polygon(
                &rectangle(MmPoint::new(0., 0.), 10., 10.),
                Exposure::Dark,
                true,
            )
            .unwrap();
        assert_eq!(scene.points.len(), ACCELERATION_TARGET_ITEMS + 5);
        assert_eq!(scene.primitives.len(), 1);
    }
}
