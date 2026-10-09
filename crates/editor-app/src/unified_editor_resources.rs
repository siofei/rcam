//! Separate host admission; not a process/GPU RSS promise.
use editor_core::*;
use editor_service::{RenderSnapshot, ServiceError};
use std::mem::size_of;
pub const HOST_BYTES: usize = 128 * 1024 * 1024; // Existing point-preview temporary contract.
pub const DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
pub fn refuse(message: &str) -> ServiceError {
    ServiceError {
        code: "RESOURCE_LIMIT".into(),
        message: message.into(),
        details: serde_json::json!({"host_temporary_limit_bytes":HOST_BYTES}),
    }
}
pub fn add(a: usize, b: usize) -> Result<usize, ServiceError> {
    a.checked_add(b)
        .ok_or_else(|| refuse("编辑会话资源计数溢出"))
}
pub fn admit(n: usize) -> Result<(), ServiceError> {
    if n > HOST_BYTES {
        Err(refuse("编辑会话显示/Snap资源超限；工作和历史已保留"))
    } else {
        Ok(())
    }
}
fn vec_cost<T>(v: &Vec<T>) -> usize {
    v.capacity()
        .saturating_mul(size_of::<T>())
        .saturating_add(64)
}
fn contours_cost(cs: &Vec<RegionContour>) -> usize {
    vec_cost(cs) + cs.iter().map(|c| vec_cost(&c.edges)).sum::<usize>()
}
pub fn geometry_heap(g: &SemanticGeometry) -> usize {
    match g {
        SemanticGeometry::Flash { aperture_id, .. } => aperture_id.capacity() + 64,
        SemanticGeometry::BlockInstance { definition_id, .. } => definition_id.0.capacity() + 64,
        SemanticGeometry::Region { contours } => contours_cost(contours),
        _ => 0,
    }
}
fn block_geometry_heap(g: &block::BlockObjectGeometry) -> usize {
    match g {
        block::BlockObjectGeometry::Flash { aperture_id, .. } => aperture_id.capacity() + 64,
        block::BlockObjectGeometry::Region { contours } => contours_cost(contours),
        _ => 0,
    }
}
pub fn definition_cost(d: &block::BlockDefinition) -> usize {
    size_of::<block::BlockDefinition>()
        + d.id.0.capacity()
        + d.name.capacity()
        + vec_cost(&d.objects)
        + d.objects
            .iter()
            .map(|o| block_geometry_heap(&o.geometry))
            .sum::<usize>()
}
pub fn origin_cost(origin: &ObjectOrigin) -> usize {
    match origin {
        ObjectOrigin::Imported { .. } => 0,
        ObjectOrigin::Generated { operation_id } | ObjectOrigin::GeneratedText { operation_id } => {
            operation_id.capacity()
        }
    }
}
pub fn selection_cost(selected: &Vec<editor_service::ObjectInfo>) -> Result<usize, ServiceError> {
    let mut n = vec_cost(selected);
    for o in selected {
        n = add(n, o.layer_id.capacity())?;
        n = add(n, o.object.object_id.capacity())?;
        n = add(n, origin_cost(&o.object.origin))?;
        n = add(n, geometry_heap(&o.object.geometry))?;
    }
    Ok(n)
}
pub fn snapshot_cost(s: &RenderSnapshot) -> Result<usize, ServiceError> {
    let mut n = size_of::<RenderSnapshot>()
        + s.document_id.capacity()
        + s.revision.capacity()
        + s.workspace_revision.capacity()
        + 256;
    n = add(
        n,
        vec_cost(&s.layers)
            + vec_cost(&s.apertures)
            + vec_cost(&s.styles)
            + vec_cost(&s.block_definitions),
    )?;
    for l in &s.layers {
        n = add(n, l.id.capacity() + vec_cost(&l.objects))?;
        for o in &l.objects {
            n = add(
                n,
                o.object_id.capacity() + origin_cost(&o.origin) + 64 + geometry_heap(&o.geometry),
            )?;
        }
    }
    for a in &s.apertures {
        n = add(n, a.id.capacity() + 64)?;
        if let ApertureShape::Macro { primitives } = &a.shape {
            n = add(n, vec_cost(primitives))?;
            for p in primitives {
                if let MacroPrimitive::Outline { points, .. } = p {
                    n = add(n, vec_cost(points))?;
                }
            }
        }
    }
    for d in &s.block_definitions {
        n = add(n, definition_cost(d))?;
    }
    for st in &s.styles {
        n = add(n, st.layer_id.capacity() + vec_cost(&st.classes))?;
    }
    Ok(n)
}
// Conservative raw primitive/vertex plan for rigid operations. Canonical Region arcs
// can add endpoint connectors; full-circle counts and a twofold margin cover them.
fn region_plan(cs: &[RegionContour], ppm: f64) -> Result<(usize, usize), ServiceError> {
    let mut p = 0usize;
    for c in cs {
        for e in &c.edges {
            let n = match e {
                RegionEdge::Line { .. } => 3,
                RegionEdge::Arc(a) => {
                    let step = 4. * (0.20 / ppm / (2. * a.radius())).min(1.).sqrt().asin();
                    let n = (std::f64::consts::TAU / step.min(std::f64::consts::FRAC_PI_2)).ceil();
                    if !n.is_finite() || n >= (1 << 24) as f64 {
                        return Err(refuse("入口轮廓显示细分超限"));
                    }
                    n as usize * 2 + 4
                }
            };
            p = add(p, n)?;
        }
    }
    Ok((cs.len(), p))
}
fn shape_plan(a: &ApertureShape) -> (usize, usize) {
    match a {
        ApertureShape::Circle { .. } | ApertureShape::Obround { .. } => (2, 0),
        ApertureShape::Rectangle { .. } => (2, 4),
        ApertureShape::Polygon { vertices, .. } => (2, *vertices as usize),
        ApertureShape::Macro { primitives } => (
            primitives.len(),
            primitives
                .iter()
                .map(|p| match p {
                    MacroPrimitive::Circle { .. } => 0,
                    MacroPrimitive::CenterLine { .. } => 4,
                    MacroPrimitive::Outline { points, .. } => points.len(),
                })
                .sum(),
        ),
    }
}
pub fn geometry_plan(
    g: &SemanticGeometry,
    s: &RenderSnapshot,
    ppm: f64,
) -> Result<(usize, usize), ServiceError> {
    match g {
        SemanticGeometry::Region { contours } => region_plan(contours, ppm),
        SemanticGeometry::Flash { aperture_id, .. } => s
            .apertures
            .iter()
            .find(|a| &a.id == aperture_id)
            .map(|a| shape_plan(&a.shape))
            .ok_or_else(|| refuse("显示光圈未找到")),
        SemanticGeometry::BlockInstance { definition_id, .. } => {
            let d = s
                .block_definitions
                .iter()
                .find(|d| &d.id == definition_id)
                .ok_or_else(|| refuse("显示Block未找到"))?;
            let mut result = (0, 0);
            for o in &d.objects {
                let a = match &o.geometry {
                    block::BlockObjectGeometry::Region { contours } => region_plan(contours, ppm)?,
                    block::BlockObjectGeometry::Flash { aperture_id, .. } => s
                        .apertures
                        .iter()
                        .find(|a| &a.id == aperture_id)
                        .map(|a| shape_plan(&a.shape))
                        .ok_or_else(|| refuse("Block光圈未找到"))?,
                    _ => (4, 4),
                };
                result.0 = add(result.0, a.0)?;
                result.1 = add(result.1, a.1)?;
            }
            Ok(result)
        }
        _ => Ok((4, 4)),
    }
}
pub fn candidate_reserve(
    s: &RenderSnapshot,
    ppm: f64,
    check: &mut impl FnMut() -> Result<(), ServiceError>,
) -> Result<usize, ServiceError> {
    if !ppm.is_finite() || ppm <= 0. {
        return Err(refuse("显示缩放无效"));
    }
    let source = snapshot_cost(s)?;
    let mut primitives = 0usize;
    let mut points = 0usize;
    let mut objects = 0usize;
    let mut blocks = 0usize;
    let mut ids = 0usize;
    for o in s.layers.iter().flat_map(|l| &l.objects) {
        check()?;
        let (p, v) = geometry_plan(&o.geometry, s, ppm)?;
        primitives = add(primitives, p)?;
        points = add(points, v)?;
        ids = add(ids, o.object_id.len() * p.max(1))?;
        if let SemanticGeometry::BlockInstance { definition_id, .. } = &o.geometry {
            let d = s
                .block_definitions
                .iter()
                .find(|d| &d.id == definition_id)
                .ok_or_else(|| refuse("Block未找到"))?;
            objects = add(objects, d.objects.len())?;
            blocks = add(blocks, definition_cost(d) * 4)?;
        } else {
            objects = add(objects, 1)?;
        }
    }
    // Raw vectors and derivation scratch, plus count-first optional acceleration's
    // existing 2M target slots (new points, bins, stable sort and raw coexistence).
    let raw = objects
        .checked_mul(2 * (size_of::<crate::display::Object>() + size_of::<String>()))
        .and_then(|n| n.checked_add(primitives * 2 * size_of::<crate::display::Primitive>()))
        .and_then(|n| n.checked_add(points * 4 * size_of::<MmPoint>()))
        .ok_or_else(|| refuse("显示缓冲计数溢出"))?;
    let index = (crate::render_index::MAX_GRID_CELLS * 16)
        + (crate::render_index::MAX_CELL_REFERENCES.max(objects * 4) * 4)
        + objects * 384;
    let acceleration = 2_000_000usize * 32; // Derived from existing display target, not a new setting.
    let selection_and_reference = 200_000usize * size_of::<MmPoint>() * 2 + objects * 256;
    let mut n = source;
    for extra in [
        source * 4,
        raw,
        ids,
        index,
        blocks,
        acceleration,
        selection_and_reference,
        4096 * 32,
    ] {
        n = add(n, extra)?;
    }
    admit(n)?;
    Ok(n)
}
