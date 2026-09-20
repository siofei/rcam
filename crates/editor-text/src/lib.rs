//! Bounded font outlines -> f64 manufacturing Regions, independent of display.
mod contours;
mod offset;
mod stroke;
use editor_core::{MmPoint, RegionContour, RegionEdge, RegionRole, SemanticGeometry};
use serde::{Deserialize, Serialize};
pub use stroke::{STROKE_SOURCE, generate_stroke};
use ttf_parser::{Face, OutlineBuilder};

pub const TOLERANCE_MM: f64 = 0.00025;
/// Maximum custom flattening error within the 0.001 mm total manufacturing budget.
pub const MAX_TOLERANCE_MM: f64 = TOLERANCE_MM;
/// Smaller requests cannot be certified against cleanup and writer resolution.
pub const MIN_TOLERANCE_MM: f64 = 0.00001;
fn default_stroke_width() -> f64 {
    0.15
}
fn default_tolerance_mm() -> f64 {
    TOLERANCE_MM
}
pub const MAX_CHARACTERS: usize = 128;
const MAX_EDGES: usize = 4096;
#[cfg(test)]
const MAX_REGIONS: usize = 10000;
const MAX_WORK: usize = 8_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    Baseline,
    Bottom,
    Middle,
    Top,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default)]
    pub baseline_spacing_mm: f64,
    #[serde(default = "default_stroke_width")]
    pub stroke_width_mm: f64,
    pub text: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub height_mm: f64,
    pub tracking_mm: f64,
    pub h_align: HorizontalAlign,
    pub v_align: VerticalAlign,
    pub rotation_deg: f64,
    #[serde(default = "default_tolerance_mm")]
    pub curve_tolerance_mm: f64,
    #[serde(default)]
    pub outline_offset_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    InvalidArgument,
    InvalidTopology,
    InvalidFont,
    MissingGlyph(char),
    UnsupportedOutline,
    ResourceLimit,
}

type Point = [f64; 2];
fn midpoint(a: Point, b: Point) -> Point {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}
fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let n = d[0] * d[0] + d[1] * d[1];
    if n == 0. {
        return distance(p, a);
    }
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / n).clamp(0., 1.);
    distance(p, [a[0] + t * d[0], a[1] + t * d[1]])
}

struct Outline {
    contours: Vec<Vec<Point>>,
    tolerance: f64,
    count: usize,
    failed: bool,
}
impl Outline {
    fn new(tolerance: f64) -> Self {
        Self {
            contours: vec![],
            tolerance,
            count: 0,
            failed: false,
        }
    }
    fn point(&mut self, p: Point) {
        if self.failed {
            return;
        }
        self.count += 1;
        if self.count > MAX_EDGES || !p.iter().all(|v| v.is_finite() && v.abs() <= 65536.) {
            self.failed = true;
            return;
        }
        if let Some(c) = self.contours.last_mut() {
            if c.last() != Some(&p) {
                c.push(p);
            }
        } else {
            self.failed = true;
        }
    }
    fn curve(&mut self, points: &[Point], depth: usize) {
        if self.failed {
            return;
        }
        let end = points[points.len() - 1];
        // Convex hull bound to the chord segment, also handles cusps/backtracking.
        if points[1..points.len() - 1]
            .iter()
            .all(|p| segment_distance(*p, points[0], end) <= self.tolerance)
        {
            self.point(end);
            return;
        }
        if depth == 24 {
            self.failed = true;
            return;
        }
        let mut row = points.to_vec();
        let mut left = vec![row[0]];
        let mut right = vec![end];
        while row.len() > 1 {
            row = row.windows(2).map(|p| midpoint(p[0], p[1])).collect();
            left.push(row[0]);
            right.push(*row.last().unwrap());
        }
        right.reverse();
        self.curve(&left, depth + 1);
        self.curve(&right, depth + 1);
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.failed {
            self.contours.push(vec![]);
            self.point([x as f64, y as f64]);
        }
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.point([x as f64, y as f64]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        if let Some(p) = self.contours.last().and_then(|c| c.last()).copied() {
            self.curve(&[p, [x1 as f64, y1 as f64], [x as f64, y as f64]], 0);
        } else {
            self.failed = true;
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if let Some(p) = self.contours.last().and_then(|c| c.last()).copied() {
            self.curve(
                &[
                    p,
                    [x1 as f64, y1 as f64],
                    [x2 as f64, y2 as f64],
                    [x as f64, y as f64],
                ],
                0,
            );
        } else {
            self.failed = true;
        }
    }
    fn close(&mut self) {
        if let Some(p) = self.contours.last().and_then(|c| c.first()).copied() {
            self.point(p);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerationTimings {
    pub parse_ms: f64,
    pub offset_ms: f64,
    pub generation_ms: f64,
}
/// Resolve a system PostScript name to the actual TTC/OTC face index.
pub fn font_face_index(bytes: &[u8], postscript: &str) -> Result<u32, TextError> {
    let count = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
    if count > 1024 {
        return Err(TextError::ResourceLimit);
    }
    for index in 0..count {
        let face = Face::parse(bytes, index).map_err(|_| TextError::InvalidFont)?;
        if face.names().into_iter().any(|n| {
            n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                && (n.to_string().as_deref() == Some(postscript)
                    || (n.platform_id == ttf_parser::PlatformId::Macintosh
                        && n.encoding_id == 0
                        && n.name.is_ascii()
                        && n.name == postscript.as_bytes()))
        }) {
            return Ok(index);
        }
    }
    Err(TextError::InvalidFont)
}
pub fn font_names(bytes: &[u8], face_index: u32) -> Result<(String, String), TextError> {
    let face = Face::parse(bytes, face_index).map_err(|_| TextError::InvalidFont)?;
    if face.is_variable() {
        return Err(TextError::UnsupportedOutline);
    }
    let name = |id| {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id)
            .find_map(|n| {
                n.to_string().or_else(|| {
                    (n.platform_id == ttf_parser::PlatformId::Macintosh
                        && n.encoding_id == 0
                        && n.name.is_ascii())
                    .then(|| String::from_utf8_lossy(n.name).into_owned())
                })
            })
            .unwrap_or_else(|| "Unknown".into())
    };
    Ok((
        name(ttf_parser::name_id::FAMILY),
        name(ttf_parser::name_id::SUBFAMILY),
    ))
}

pub fn generate(
    bytes: &[u8],
    face_index: u32,
    layout: &Layout,
) -> Result<Vec<SemanticGeometry>, TextError> {
    generate_timed(bytes, face_index, layout).map(|(geometry, _)| geometry)
}

fn validate_layout(layout: &Layout) -> Result<(), TextError> {
    let values = [
        layout.x_mm,
        layout.y_mm,
        layout.height_mm,
        layout.tracking_mm,
        layout.rotation_deg,
        layout.curve_tolerance_mm,
        layout.outline_offset_mm,
        layout.baseline_spacing_mm,
    ];
    if !values.iter().all(|v| v.is_finite())
        || layout.curve_tolerance_mm <= 0.
        || layout.curve_tolerance_mm > MAX_TOLERANCE_MM
        || layout.outline_offset_mm.abs() > 2.
        || layout.outline_offset_mm.abs() > layout.height_mm * 0.25
        || layout.height_mm <= 0.
        || layout.height_mm > 1000.
        || layout.tracking_mm.abs() > 1000.
        || layout.rotation_deg.abs() > 1e9
        || layout.text.trim().is_empty()
        || layout.baseline_spacing_mm < 0.
        || layout.baseline_spacing_mm > 10000.
        || layout.text.chars().any(|c| c.is_control() && c != '\n')
    {
        return Err(TextError::InvalidArgument);
    }
    if layout.curve_tolerance_mm < MIN_TOLERANCE_MM || layout.text.chars().count() > MAX_CHARACTERS
    {
        return Err(TextError::ResourceLimit);
    }
    Ok(())
}

pub fn generate_timed(
    bytes: &[u8],
    face_index: u32,
    layout: &Layout,
) -> Result<(Vec<SemanticGeometry>, GenerationTimings), TextError> {
    generate_timed_on_grid(bytes, face_index, layout, 1e-6)
}
/// Manufacture text directly on the selected relative lattice before fitting.
/// Existing public generation retains its FS-sized default for API compatibility.
pub fn generate_timed_on_grid(
    bytes: &[u8],
    face_index: u32,
    layout: &Layout,
    resolution_mm: f64,
) -> Result<(Vec<SemanticGeometry>, GenerationTimings), TextError> {
    editor_core::units::ManufacturingPrecision { resolution_mm }
        .validate()
        .map_err(|_| TextError::UnsupportedOutline)?;
    validate_layout(layout)?;
    let started = std::time::Instant::now();
    let mut timings = GenerationTimings::default();
    validate_layout(layout)?;
    let parse_started = std::time::Instant::now();
    let face = Face::parse(bytes, face_index).map_err(|_| TextError::InvalidFont)?;
    timings.parse_ms = parse_started.elapsed().as_secs_f64() * 1000.;
    if face.is_variable() {
        return Err(TextError::UnsupportedOutline);
    }
    let mut glyphs = Vec::new();
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    // First pass establishes visible outline height; no screen/font raster metrics.
    for ch in layout.text.chars() {
        if ch == '\n' {
            glyphs.push((ttf_parser::GlyphId(0), 0., false));
            continue;
        }
        if !(ch.is_ascii()
            || ('\u{3000}'..='\u{303f}').contains(&ch)
            || ('\u{3400}'..='\u{9fff}').contains(&ch)
            || ('\u{ff01}'..='\u{ff60}').contains(&ch))
        {
            return Err(TextError::UnsupportedOutline);
        }
        let id = face
            .glyph_index(ch)
            .filter(|id| id.0 != 0)
            .ok_or(TextError::MissingGlyph(ch))?;
        let advance = face
            .glyph_hor_advance(id)
            .ok_or(TextError::UnsupportedOutline)? as f64;
        if ch == ' ' || ch == '\u{3000}' {
            glyphs.push((id, advance, false));
            continue;
        }
        let mut outline = Outline::new(0.01);
        face.outline_glyph(id, &mut outline)
            .ok_or(TextError::UnsupportedOutline)?;
        if outline.failed {
            return Err(TextError::ResourceLimit);
        }
        for p in outline.contours.iter().flatten() {
            ymin = ymin.min(p[1]);
            ymax = ymax.max(p[1]);
        }
        glyphs.push((id, advance, true));
    }
    let scale = (layout.height_mm - 2. * layout.outline_offset_mm) / (ymax - ymin);
    // ttf-parser supplies f32 font coordinates. Reserve a conservative conversion
    // allowance in addition to flattening; reject scales that cannot certify it.
    if !scale.is_finite() || scale <= 0. || scale * 65536. * f32::EPSILON as f64 * 4. > 0.00025 {
        return Err(TextError::UnsupportedOutline);
    }
    let conversion_allowance = scale * 65536. * f32::EPSILON as f64 * 4.;
    if conversion_allowance
        + layout.curve_tolerance_mm * 0.9
        + resolution_mm / std::f64::consts::SQRT_2
        > 0.001
    {
        return Err(TextError::UnsupportedOutline);
    }
    let mut outlines = Vec::new();
    let mut outline_baselines = Vec::new();
    let mut pen = 0.;
    let mut baseline = 0.;
    let spacing = if layout.baseline_spacing_mm == 0. {
        layout.height_mm * 1.3
    } else {
        layout.baseline_spacing_mm
    };
    for (id, advance, visible) in glyphs {
        if id.0 == 0 {
            pen = 0.;
            baseline -= spacing;
            continue;
        }
        if visible {
            let mut outline = Outline::new(layout.curve_tolerance_mm * 0.4 / scale);
            face.outline_glyph(id, &mut outline)
                .ok_or(TextError::UnsupportedOutline)?;
            if outline.failed {
                return Err(TextError::ResourceLimit);
            }
            for contour in &mut outline.contours {
                for p in contour {
                    p[0] = p[0] * scale + pen;
                    p[1] = p[1] * scale + baseline;
                }
            }
            outlines.push(outline.contours);
            outline_baselines.push(baseline);
        }
        pen += advance * scale + layout.tracking_mm;
    }
    let offset_started = std::time::Instant::now();
    if layout.outline_offset_mm != 0. {
        // Erosion does not distribute over overlapping glyph unions. Refuse
        // ambiguous overlapping bounding intervals rather than erode separately.
        if layout.outline_offset_mm < 0. {
            let boxes: Vec<_> = outlines
                .iter()
                .map(|cs| {
                    cs.iter().flatten().fold(
                        (
                            f64::INFINITY,
                            f64::NEG_INFINITY,
                            f64::INFINITY,
                            f64::NEG_INFINITY,
                        ),
                        |(x0, x1, y0, y1), p| {
                            (x0.min(p[0]), x1.max(p[0]), y0.min(p[1]), y1.max(p[1]))
                        },
                    )
                })
                .collect();
            for (i, a) in boxes.iter().enumerate() {
                if boxes[i + 1..]
                    .iter()
                    .any(|b| a.1 >= b.0 && b.1 >= a.0 && a.3 >= b.2 && b.3 >= a.2)
                {
                    return Err(TextError::InvalidTopology);
                }
            }
        }
        outlines = outlines
            .iter()
            .map(|c| offset::material(c, layout.outline_offset_mm))
            .collect::<Result<_, _>>()?;
    }
    timings.offset_ms = offset_started.elapsed().as_secs_f64() * 1000.;
    let outlines = outlines
        .iter()
        .map(|c| offset::material(c, 0.))
        .collect::<Result<Vec<_>, _>>()?;
    let all: Vec<_> = outlines.iter().flatten().flatten().copied().collect();
    let xmin = all.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let xmax = all.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let ymin = all.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let ymax = all.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    let (glyph_min, glyph_max) = outlines
        .iter()
        .zip(&outline_baselines)
        .flat_map(|(cs, b)| cs.iter().flatten().map(move |p| p[1] - b))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| {
            (lo.min(y), hi.max(y))
        });
    if layout.outline_offset_mm != 0.
        && ((glyph_max - glyph_min) - layout.height_mm).abs() > 0.00008
    {
        return Err(TextError::InvalidTopology);
    }
    let anchor_x = match layout.h_align {
        HorizontalAlign::Left => xmin,
        HorizontalAlign::Center => (xmin + xmax) * 0.5,
        HorizontalAlign::Right => xmax,
    };
    let anchor_y = match layout.v_align {
        VerticalAlign::Baseline => 0.,
        VerticalAlign::Bottom => ymin,
        VerticalAlign::Middle => (ymin + ymax) * 0.5,
        VerticalAlign::Top => ymax,
    };
    let (sin, cos) = layout.rotation_deg.to_radians().sin_cos();
    // Rotation is frozen before fitting. Fit on the document manufacturing lattice,
    // then apply the freely translated anchor once. Pointer motion cannot refit.
    let local = |p: Point| {
        let x = p[0] - anchor_x;
        let y = p[1] - anchor_y;
        [x * cos - y * sin, x * sin + y * cos]
    };
    let mut geometries = Vec::new();
    let mut work = 0;
    for outline in outlines {
        let rotated: Vec<_> = outline
            .into_iter()
            .map(|c| c.into_iter().map(local).collect())
            .collect();
        let normalized = offset::material(&rotated, 0.)?;
        for polygon in contours::join(normalized)? {
            let mut points: Vec<_> = polygon
                .into_iter()
                .map(|p| {
                    [
                        (p[0] / resolution_mm).round() * resolution_mm,
                        (p[1] / resolution_mm).round() * resolution_mm,
                    ]
                })
                .collect();
            points.dedup();
            let edges = contours::fit_on_grid(
                &points,
                layout.curve_tolerance_mm * 0.5,
                &mut work,
                resolution_mm,
            )?;
            let contour = RegionContour {
                role: RegionRole::Solid,
                edges,
            };
            editor_core::validate_region_contour(&contour).map_err(|e| {
                eprintln!("text topology: {e}");
                TextError::InvalidTopology
            })?;
            geometries.push(SemanticGeometry::Region {
                contours: vec![contour],
            });
        }
    }
    if geometries.is_empty() {
        return Err(TextError::UnsupportedOutline);
    }
    // Restore exact alignment after bounded fitting when axes are unrotated.
    // This is one common translation, preserving the writer lattice differences.
    let bounds = editor_core::geometries_bounds(&geometries, &[])
        .map_err(|_| TextError::InvalidTopology)?
        .ok_or(TextError::InvalidTopology)?;
    let (shift_x, shift_y) = if layout.rotation_deg == 0. {
        (
            match layout.h_align {
                HorizontalAlign::Left => bounds.min_x_mm,
                HorizontalAlign::Center => (bounds.min_x_mm + bounds.max_x_mm) * 0.5,
                HorizontalAlign::Right => bounds.max_x_mm,
            },
            match layout.v_align {
                VerticalAlign::Baseline => 0.,
                VerticalAlign::Bottom => bounds.min_y_mm,
                VerticalAlign::Middle => (bounds.min_y_mm + bounds.max_y_mm) * 0.5,
                VerticalAlign::Top => bounds.max_y_mm,
            },
        )
    } else {
        (0., 0.)
    };
    let translate = |p: &mut MmPoint| {
        p.x_mm += layout.x_mm - shift_x;
        p.y_mm += layout.y_mm - shift_y;
    };
    for g in &mut geometries {
        let SemanticGeometry::Region { contours } = g else {
            unreachable!()
        };
        for c in contours {
            for e in &mut c.edges {
                match e {
                    RegionEdge::Line { start, end } => {
                        translate(start);
                        translate(end);
                    }
                    RegionEdge::Arc(a) => {
                        translate(&mut a.start);
                        translate(&mut a.end);
                        translate(&mut a.center);
                    }
                }
            }
            editor_core::validate_region_contour(c).map_err(|_| TextError::InvalidTopology)?;
        }
    }
    timings.generation_ms = started.elapsed().as_secs_f64() * 1000.;
    Ok((geometries, timings))
}

// Vertical slab decomposition uses the font's nonzero winding rule. Every
// output is a local solid polygon; holes produce no exposure of either polarity.
#[cfg(test)]
fn decompose(contours: &[Vec<Point>], work: &mut usize) -> Result<Vec<Vec<Point>>, TextError> {
    let mut edges = Vec::new();
    let mut levels = Vec::new();
    for c in contours {
        if c.len() < 4 || c.first() != c.last() {
            return Err(TextError::UnsupportedOutline);
        }
        for pair in c.windows(2) {
            levels.push(pair[0][1]);
            if pair[0][1] != pair[1][1] {
                edges.push((pair[0], pair[1]));
            }
        }
    }
    // Split at edge intersections too: winding order cannot change inside a slab.
    for (i, &(a, b)) in edges.iter().enumerate() {
        for &(c, d) in &edges[i + 1..] {
            *work += 1;
            if *work > MAX_WORK {
                return Err(TextError::ResourceLimit);
            }
            let u = [b[0] - a[0], b[1] - a[1]];
            let v = [d[0] - c[0], d[1] - c[1]];
            let cross = u[0] * v[1] - u[1] * v[0];
            if cross != 0. {
                let w = [c[0] - a[0], c[1] - a[1]];
                let t = (w[0] * v[1] - w[1] * v[0]) / cross;
                let s = (w[0] * u[1] - w[1] * u[0]) / cross;
                if t > 0. && t < 1. && s > 0. && s < 1. {
                    levels.push(a[1] + t * u[1]);
                }
            }
        }
    }
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let x_at = |(a, b): (Point, Point), y: f64| a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
    let mut output = Vec::new();
    for ys in levels.windows(2) {
        let mid = (ys[0] + ys[1]) * 0.5;
        *work += edges.len();
        if *work > MAX_WORK {
            return Err(TextError::ResourceLimit);
        }
        let mut active: Vec<_> = edges
            .iter()
            .copied()
            .filter(|(a, b)| mid > a[1].min(b[1]) && mid < a[1].max(b[1]))
            .collect();
        active.sort_by(|&a, &b| x_at(a, mid).total_cmp(&x_at(b, mid)));
        let mut winding = 0;
        let mut left = None;
        for edge in active {
            let previous = winding;
            winding += if edge.1[1] > edge.0[1] { 1 } else { -1 };
            if previous == 0 && winding != 0 {
                left = Some(edge);
            }
            if previous != 0 && winding == 0 {
                let l = left.take().ok_or(TextError::UnsupportedOutline)?;
                if x_at(edge, mid) > x_at(l, mid) {
                    output.push(vec![
                        [x_at(l, ys[0]), ys[0]],
                        [x_at(edge, ys[0]), ys[0]],
                        [x_at(edge, ys[1]), ys[1]],
                        [x_at(l, ys[1]), ys[1]],
                    ]);
                    if output.len() > MAX_REGIONS {
                        return Err(TextError::ResourceLimit);
                    }
                }
            }
        }
        if winding != 0 {
            return Err(TextError::UnsupportedOutline);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn font() -> Vec<u8> {
        std::fs::read(
            std::env::var("RCAM_TEXT_FONT")
                .unwrap_or_else(|_| "/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into()),
        )
        .expect("set RCAM_TEXT_FONT to an authorized static CJK outline font")
    }
    fn layout(text: &str) -> Layout {
        Layout {
            text: text.into(),
            x_mm: 0.,
            y_mm: 0.,
            height_mm: 3.,
            tracking_mm: 0.,
            h_align: HorizontalAlign::Left,
            v_align: VerticalAlign::Bottom,
            rotation_deg: 0.,
            curve_tolerance_mm: TOLERANCE_MM,
            baseline_spacing_mm: 0.,
            stroke_width_mm: 0.15,
            outline_offset_mm: 0.,
        }
    }
    #[test]
    fn builtin_ascii_multiline_geometry_and_limits() {
        let mut l = layout("A\nA");
        l.baseline_spacing_mm = 4.5;
        let g = generate_stroke(&l).unwrap();
        assert!(
            g.iter()
                .all(|g| matches!(g, SemanticGeometry::Line {width_mm,..} if *width_mm==0.15))
        );
        let b = editor_core::geometries_bounds(&g, &[]).unwrap().unwrap();
        assert!(((b.max_y_mm - b.min_y_mm) - 7.5).abs() < 1e-10);
        l.rotation_deg = 90.;
        let r = editor_core::geometries_bounds(&generate_stroke(&l).unwrap(), &[])
            .unwrap()
            .unwrap();
        assert!(((r.max_x_mm - r.min_x_mm) - 7.5).abs() < 1e-10);
        for c in '!'..='~' {
            l.text = c.to_string();
            assert!(
                !generate_stroke(&l)
                    .unwrap_or_else(|e| panic!("{c}: {e:?}"))
                    .is_empty()
            );
        }
        l.text = "中".into();
        assert_eq!(generate_stroke(&l), Err(TextError::MissingGlyph('中')));
        l.text = "A".repeat(129);
        assert_eq!(generate_stroke(&l), Err(TextError::ResourceLimit));
        l.text = "A".into();
        l.stroke_width_mm = 0.;
        assert_eq!(generate_stroke(&l), Err(TextError::InvalidArgument));
    }
    #[test]
    fn outline_multiline_uses_shared_scale_and_preserves_offset_gate() {
        let bytes = font();
        let mut l = layout("中\n中");
        l.baseline_spacing_mm = 4.5;
        for offset in [0., 0.01, -0.01] {
            l.outline_offset_mm = offset;
            let g = generate(&bytes, 0, &l).unwrap();
            let b = editor_core::geometries_bounds(&g, &[]).unwrap().unwrap();
            assert!(((b.max_y_mm - b.min_y_mm) - 7.5).abs() < 0.00008);
        }
    }
    fn boundary_edges(g: &[SemanticGeometry]) -> Vec<RegionEdge> {
        g.iter().flat_map(|g|{let SemanticGeometry::Region{contours}=g else{panic!()};
            contours.iter().flat_map(|c|c.edges.iter().filter(|e|!matches!(e,RegionEdge::Line{start,end} if c.edges.iter().any(|o|matches!(o,RegionEdge::Line{start:a,end:b} if a==end && b==start)))).cloned()).collect::<Vec<_>>()}).collect()
    }
    fn edge_sample(edge: &RegionEdge, t: f64) -> MmPoint {
        match edge {
            RegionEdge::Line { start, end } => MmPoint::new(
                start.x_mm + (end.x_mm - start.x_mm) * t,
                start.y_mm + (end.y_mm - start.y_mm) * t,
            ),
            RegionEdge::Arc(a) => {
                let a = editor_core::canonical_region_contour(&RegionContour {
                    role: RegionRole::Solid,
                    edges: vec![RegionEdge::Arc(*a)],
                })
                .unwrap();
                let RegionEdge::Arc(a) = a.edges[0] else {
                    panic!()
                };
                let start = (a.start.y_mm - a.center.y_mm).atan2(a.start.x_mm - a.center.x_mm);
                let sign = if a.direction == editor_core::ArcDirection::Clockwise {
                    -1.
                } else {
                    1.
                };
                let angle = start + sign * a.sweep_radians().unwrap() * t;
                MmPoint::new(
                    a.center.x_mm + a.radius() * angle.cos(),
                    a.center.y_mm + a.radius() * angle.sin(),
                )
            }
        }
    }
    pub(super) fn edge_distance(p: MmPoint, e: &RegionEdge) -> f64 {
        match e {
            RegionEdge::Line { start, end } => segment_distance(
                [p.x_mm, p.y_mm],
                [start.x_mm, start.y_mm],
                [end.x_mm, end.y_mm],
            ),
            RegionEdge::Arc(a) => {
                let from = (a.start.y_mm - a.center.y_mm).atan2(a.start.x_mm - a.center.x_mm);
                let to = (p.y_mm - a.center.y_mm).atan2(p.x_mm - a.center.x_mm);
                let d = if a.direction == editor_core::ArcDirection::Clockwise {
                    (from - to).rem_euclid(std::f64::consts::TAU)
                } else {
                    (to - from).rem_euclid(std::f64::consts::TAU)
                };
                if d <= a.sweep_radians().unwrap() {
                    (p.distance_mm(a.center) - a.radius()).abs()
                } else {
                    p.distance_mm(a.start).min(p.distance_mm(a.end))
                }
            }
        }
    }
    fn bounds(g: &[SemanticGeometry]) -> [f64; 4] {
        let b = editor_core::geometries_bounds(g, &[]).unwrap().unwrap();
        [b.min_x_mm, b.min_y_mm, b.max_x_mm, b.max_y_mm]
    }
    #[test]
    fn ascii_cjk_holes_height_and_alignment() {
        let bytes = font();
        for text in [
            "W1234567",
            "8B",
            "钢网测试口回",
            "中文ABC123",
            "O田中回",
            "A P 8",
        ] {
            let mut l = layout(text);
            let g = generate(&bytes, 0, &l).unwrap_or_else(|e| panic!("{text}: {e:?}"));
            let b = bounds(&g);
            assert!((b[3] - b[1] - 3.).abs() < 0.001, "{text}: {b:?}");
            assert!(b[0].abs() < 1e-10 && b[1].abs() < 1e-10);
            for h in [HorizontalAlign::Center, HorizontalAlign::Right] {
                l.h_align = h;
                let b = bounds(&generate(&bytes, 0, &l).unwrap());
                assert!(
                    (if h == HorizontalAlign::Center {
                        b[0] + b[2]
                    } else {
                        b[2]
                    })
                    .abs()
                        < 1e-9
                );
            }
            for v in [
                VerticalAlign::Middle,
                VerticalAlign::Top,
                VerticalAlign::Baseline,
            ] {
                l.v_align = v;
                let b = bounds(&generate(&bytes, 0, &l).unwrap());
                if v != VerticalAlign::Baseline {
                    assert!(
                        (if v == VerticalAlign::Middle {
                            b[1] + b[3]
                        } else {
                            b[3]
                        })
                        .abs()
                            < 1e-9
                    );
                }
            }
        }
    }
    #[test]
    fn tracking_and_rigid_rotation() {
        let bytes = font();
        let mut l = layout("AA");
        let g = generate(&bytes, 0, &l).unwrap();
        let width = bounds(&g)[2];
        l.tracking_mm = 0.7;
        assert!((bounds(&generate(&bytes, 0, &l).unwrap())[2] - width - 0.7).abs() < 1e-9);
        l.tracking_mm = 0.;
        for angle in [90_f64, 37.25, -145.] {
            l.rotation_deg = angle;
            l.x_mm = 10.;
            l.y_mm = 20.;
            let rotated = generate(&bytes, 0, &l).unwrap();
            let (s, c) = angle.to_radians().sin_cos();
            // Cut-ins and fitted subdivisions are representation details and
            // can differ after rotation. Compare the actual material boundary
            // bidirectionally against the unchanged 0.001 mm manufacturing gate.
            let source = boundary_edges(&g);
            let target = boundary_edges(&rotated);
            for (edges, other, inverse) in [(&source, &target, false), (&target, &source, true)] {
                for edge in edges {
                    for i in 0..=32 {
                        let p = edge_sample(edge, i as f64 / 32.);
                        let q = if inverse {
                            let x = p.x_mm - 10.;
                            let y = p.y_mm - 20.;
                            MmPoint::new(x * c + y * s, -x * s + y * c)
                        } else {
                            MmPoint::new(
                                10. + p.x_mm * c - p.y_mm * s,
                                20. + p.x_mm * s + p.y_mm * c,
                            )
                        };
                        let error = other
                            .iter()
                            .map(|e| edge_distance(q, e))
                            .fold(f64::INFINITY, f64::min);
                        assert!(error < 0.001, "rotated boundary error {error}");
                    }
                }
            }
        }
    }
    #[test]
    fn invalid_input_is_rejected() {
        let bytes = font();
        for text in ["", "  ", "A\tB"] {
            assert!(generate(&bytes, 0, &layout(text)).is_err());
        }
        assert_eq!(
            generate(b"invalid", 0, &layout("A")),
            Err(TextError::InvalidFont)
        );
        assert_eq!(
            generate(&bytes, u32::MAX, &layout("A")),
            Err(TextError::InvalidFont)
        );
        assert!(generate(&bytes, 0, &layout("\u{9fff}")).is_err());
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for field in 0..5 {
                let mut l = layout("A");
                match field {
                    0 => l.x_mm = invalid,
                    1 => l.y_mm = invalid,
                    2 => l.height_mm = invalid,
                    3 => l.tracking_mm = invalid,
                    _ => l.rotation_deg = invalid,
                };
                assert!(generate(&bytes, 0, &l).is_err());
            }
        }
        assert_eq!(
            generate(&bytes, 0, &layout(&"A".repeat(129))),
            Err(TextError::ResourceLimit)
        );
    }
    #[test]
    fn bezier_flattening_independent_dense_distance_check() {
        for controls in [
            vec![[0., 0.], [1., 3.], [4., 0.]],
            vec![[0., 0.], [4., 6.], [-4., 6.], [0., 0.]],
            vec![[0., 0.], [4., 0.], [-4., 0.], [1., 0.]],
        ] {
            let mut o = Outline::new(TOLERANCE_MM);
            o.contours.push(vec![controls[0]]);
            o.curve(&controls, 0);
            assert!(!o.failed);
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
                let error = o.contours[0]
                    .windows(2)
                    .map(|w| segment_distance(p[0], w[0], w[1]))
                    .fold(f64::INFINITY, f64::min);
                assert!(error <= TOLERANCE_MM + 1e-12, "{error}");
            }
        }
    }
    #[test]
    fn custom_tolerance_bounds_have_independent_dense_distance_checks() {
        let bytes = font();
        // Service tests exercise JSON defaults; here test the independent curve bound
        // at every preset and both accepted endpoints without altering core thresholds.
        for tolerance in [MIN_TOLERANCE_MM, 0.0000625, 0.000125, MAX_TOLERANCE_MM] {
            let mut o = Outline::new(tolerance);
            o.contours.push(vec![[0., 0.]]);
            o.curve(&[[0., 0.], [1., 3.], [4., 0.]], 0);
            assert!(!o.failed);
            for i in 0..=10000 {
                let t = i as f64 / 10000.;
                let point = [2. * (1. - t) * t + 4. * t * t, 6. * (1. - t) * t];
                let error = o.contours[0]
                    .windows(2)
                    .map(|w| segment_distance(point, w[0], w[1]))
                    .fold(f64::INFINITY, f64::min);
                assert!(error <= tolerance + 1e-12);
            }
        }
        let mut explicit = layout("口");
        explicit.curve_tolerance_mm = MIN_TOLERANCE_MM;
        // Rectilinear geometry demonstrates that the minimum is accepted;
        // curve/resource/semantic limits still apply independently.
        assert!(generate(&bytes, 0, &explicit).is_ok());
    }

    #[test]
    fn nonzero_winding_preserves_holes_and_overlapping_solids() {
        let outer = vec![[0., 0.], [4., 0.], [4., 4.], [0., 4.], [0., 0.]];
        let hole = vec![[1., 1.], [1., 3.], [3., 3.], [3., 1.], [1., 1.]];
        let area = |polygons: Vec<Vec<Point>>| {
            polygons
                .iter()
                .map(|p| {
                    let mut a = 0.;
                    for i in 0..p.len() {
                        let j = (i + 1) % p.len();
                        a += p[i][0] * p[j][1] - p[j][0] * p[i][1];
                    }
                    a.abs() * 0.5
                })
                .sum::<f64>()
        };
        assert_eq!(
            area(decompose(&[outer.clone(), hole.clone()], &mut 0).unwrap()),
            12.
        );
        let mut same = hole;
        same.reverse();
        assert_eq!(area(decompose(&[outer, same], &mut 0).unwrap()), 16.);
    }
}
