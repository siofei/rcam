//! Bounded font outlines -> f64 manufacturing Regions, independent of display.
mod offset;
use editor_core::{MmPoint, RegionContour, RegionEdge, RegionRole, SemanticGeometry};
use serde::{Deserialize, Serialize};
use ttf_parser::{Face, OutlineBuilder};

pub const TOLERANCE_MM: f64 = 0.00025;
/// Maximum custom flattening error within the 0.001 mm total manufacturing budget.
pub const MAX_TOLERANCE_MM: f64 = TOLERANCE_MM;
/// Smaller requests cannot be certified against cleanup and writer resolution.
pub const MIN_TOLERANCE_MM: f64 = 0.00001;
fn default_tolerance_mm() -> f64 {
    TOLERANCE_MM
}
pub const MAX_CHARACTERS: usize = 128;
const MAX_EDGES: usize = 4096;
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

pub fn generate_timed(
    bytes: &[u8],
    face_index: u32,
    layout: &Layout,
) -> Result<(Vec<SemanticGeometry>, GenerationTimings), TextError> {
    let started = std::time::Instant::now();
    let mut timings = GenerationTimings::default();
    let values = [
        layout.x_mm,
        layout.y_mm,
        layout.height_mm,
        layout.tracking_mm,
        layout.rotation_deg,
        layout.curve_tolerance_mm,
        layout.outline_offset_mm,
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
        || layout.text.chars().any(char::is_control)
    {
        return Err(TextError::InvalidArgument);
    }
    if layout.curve_tolerance_mm < MIN_TOLERANCE_MM || layout.text.chars().count() > MAX_CHARACTERS
    {
        return Err(TextError::ResourceLimit);
    }
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
    let mut outlines = Vec::new();
    let mut pen = 0.;
    for (id, advance, visible) in glyphs {
        if visible {
            let mut outline = Outline::new(layout.curve_tolerance_mm / scale);
            face.outline_glyph(id, &mut outline)
                .ok_or(TextError::UnsupportedOutline)?;
            if outline.failed {
                return Err(TextError::ResourceLimit);
            }
            for contour in &mut outline.contours {
                for p in contour {
                    p[0] = p[0] * scale + pen;
                    p[1] *= scale;
                }
            }
            outlines.push(outline.contours);
        }
        pen += advance * scale + layout.tracking_mm;
    }
    let offset_started = std::time::Instant::now();
    if layout.outline_offset_mm != 0. {
        // Erosion does not distribute over overlapping glyph unions. Refuse
        // ambiguous overlapping bounding intervals rather than erode separately.
        if layout.outline_offset_mm < 0. {
            let mut intervals: Vec<_> = outlines
                .iter()
                .map(|cs| {
                    cs.iter()
                        .flatten()
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                            (lo.min(p[0]), hi.max(p[0]))
                        })
                })
                .collect();
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            if intervals.windows(2).any(|w| w[0].1 >= w[1].0) {
                return Err(TextError::InvalidTopology);
            }
        }
        outlines = outlines
            .iter()
            .map(|c| offset::material(c, layout.outline_offset_mm))
            .collect::<Result<_, _>>()?;
    }
    timings.offset_ms = offset_started.elapsed().as_secs_f64() * 1000.;
    let all: Vec<_> = outlines.iter().flatten().flatten().copied().collect();
    let xmin = all.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let xmax = all.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let low = all.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let high = all.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    // Erosion at an acute extremum can retreat by more than delta. Never
    // advertise the requested visible height when that normalization is lost.
    if layout.outline_offset_mm != 0. && ((high - low) - layout.height_mm).abs() > 0.00008 {
        return Err(TextError::InvalidTopology);
    }
    let anchor_x = match layout.h_align {
        HorizontalAlign::Left => xmin,
        HorizontalAlign::Center => (xmin + xmax) * 0.5,
        HorizontalAlign::Right => xmax,
    };
    let anchor_y = match layout.v_align {
        VerticalAlign::Baseline => 0.,
        VerticalAlign::Bottom => low,
        VerticalAlign::Middle => (low + high) * 0.5,
        VerticalAlign::Top => high,
    };
    let (sin, cos) = layout.rotation_deg.to_radians().sin_cos();
    let transform = |p: Point| {
        let x = p[0] - anchor_x;
        let y = p[1] - anchor_y;
        MmPoint::new(
            layout.x_mm + x * cos - y * sin,
            layout.y_mm + x * sin + y * cos,
        )
    };
    let mut geometries = Vec::new();
    let mut collapsed = Vec::new();
    let mut work = 0;
    for contours in outlines {
        for polygon in decompose(&contours, &mut work)? {
            let mut points: Vec<_> = polygon.into_iter().map(transform).collect();
            let original = points.clone();
            points.dedup_by(|a, b| a.distance_mm(*b) <= 0.000004);
            if points
                .first()
                .zip(points.last())
                .is_some_and(|(a, b)| a.distance_mm(*b) <= 0.000004)
            {
                points.pop();
            }
            // A slab may be long but sub-resolution in its perpendicular
            // direction. Endpoint deduplication alone does not detect it.
            // Only omit it if the existing retained-boundary certificate below
            // proves its entire convex hull remains within the 4 nm budget.
            let thin = points
                .iter()
                .enumerate()
                .flat_map(|(i, a)| points[i + 1..].iter().map(move |b| (*a, *b)))
                .max_by(|(a, b), (c, d)| a.distance_mm(*b).total_cmp(&c.distance_mm(*d)))
                .is_some_and(|(a, b)| {
                    points.iter().all(|p| {
                        segment_distance([p.x_mm, p.y_mm], [a.x_mm, a.y_mm], [b.x_mm, b.y_mm])
                            <= 0.000001
                    })
                });
            if points.len() < 3 || thin {
                collapsed.push(original);
                continue;
            }
            if points.iter().any(|p| {
                !p.x_mm.is_finite()
                    || !p.y_mm.is_finite()
                    || p.x_mm.abs() > 1e6
                    || p.y_mm.abs() > 1e6
            }) {
                return Err(TextError::UnsupportedOutline);
            }
            let edges = (0..points.len())
                .map(|i| RegionEdge::Line {
                    start: points[i],
                    end: points[(i + 1) % points.len()],
                })
                .collect();
            geometries.push(SemanticGeometry::Region {
                contours: vec![RegionContour {
                    role: RegionRole::Solid,
                    edges,
                }],
            });
            if geometries.len() > MAX_REGIONS {
                return Err(TextError::ResourceLimit);
            }
        }
    }
    // Never silently remove an isolated thin feature. Certify that each
    // collapsed convex slab lies within 4 nm of ONE retained boundary segment.
    // Convexity then bounds every interior point, not just sampled vertices.
    for polygon in collapsed {
        let mut certified = false;
        'search: for geometry in &geometries {
            let SemanticGeometry::Region { contours } = geometry else {
                unreachable!()
            };
            for edge in contours.iter().flat_map(|c| &c.edges) {
                work += 1;
                if work > MAX_WORK {
                    return Err(TextError::ResourceLimit);
                }
                let RegionEdge::Line { start, end } = edge else {
                    unreachable!()
                };
                if polygon.iter().all(|p| {
                    segment_distance(
                        [p.x_mm, p.y_mm],
                        [start.x_mm, start.y_mm],
                        [end.x_mm, end.y_mm],
                    ) <= 0.000004 + 1e-10
                }) {
                    certified = true;
                    break 'search;
                }
            }
        }
        if !certified {
            return Err(TextError::UnsupportedOutline);
        }
    }
    if geometries.is_empty() {
        return Err(TextError::UnsupportedOutline);
    }
    timings.generation_ms = started.elapsed().as_secs_f64() * 1000.;
    Ok((geometries, timings))
}

// Vertical slab decomposition uses the font's nonzero winding rule. Every
// output is a local solid polygon; holes produce no exposure of either polarity.
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
            outline_offset_mm: 0.,
        }
    }
    fn points(g: &[SemanticGeometry]) -> Vec<MmPoint> {
        g.iter()
            .flat_map(|g| match g {
                SemanticGeometry::Region { contours } => contours
                    .iter()
                    .flat_map(|c| &c.edges)
                    .map(|e| match e {
                        RegionEdge::Line { start, .. } => *start,
                        _ => panic!(),
                    })
                    .collect::<Vec<_>>(),
                _ => panic!(),
            })
            .collect()
    }
    fn bounds(g: &[SemanticGeometry]) -> [f64; 4] {
        points(g).iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |b, p| {
                [
                    b[0].min(p.x_mm),
                    b[1].min(p.y_mm),
                    b[2].max(p.x_mm),
                    b[3].max(p.y_mm),
                ]
            },
        )
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
            for (p, q) in points(&g).iter().zip(points(&rotated)) {
                assert!(
                    q.distance_mm(MmPoint::new(
                        10. + p.x_mm * c - p.y_mm * s,
                        20. + p.x_mm * s + p.y_mm * c
                    )) < 1e-10
                );
            }
        }
    }
    #[test]
    fn invalid_input_is_rejected() {
        let bytes = font();
        for text in ["", "  ", "A\nB", "A\tB"] {
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
