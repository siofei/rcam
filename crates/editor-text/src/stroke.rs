//! Original centerline glyphs, emitted directly as manufacturing Lines.
use super::*;
pub const STROKE_SOURCE: &str = include_str!("stroke_font.txt");
pub fn generate_stroke(layout: &Layout) -> Result<Vec<SemanticGeometry>, TextError> {
    validate_layout(layout)?;
    let width = layout.stroke_width_mm;
    if !width.is_finite()
        || width <= 0.
        || width >= layout.height_mm
        || layout.outline_offset_mm != 0.
    {
        return Err(TextError::InvalidArgument);
    }
    let scale = (layout.height_mm - width) / 7.;
    let spacing = if layout.baseline_spacing_mm == 0. {
        layout.height_mm * 1.3
    } else {
        layout.baseline_spacing_mm
    };
    let (mut x, mut y) = (0., 0.);
    let mut geometries = Vec::new();
    for ch in layout.text.chars() {
        if ch == '\n' {
            x = 0.;
            y -= spacing;
            continue;
        }
        if ch == ' ' {
            x += 6. * scale + layout.tracking_mm;
            continue;
        }
        if !ch.is_ascii_graphic() {
            return Err(TextError::MissingGlyph(ch));
        }
        let path = STROKE_SOURCE
            .lines()
            .find(|s| {
                s.as_bytes().first() == Some(&(ch as u8)) && s.as_bytes().get(1) == Some(&b':')
            })
            .ok_or(TextError::MissingGlyph(ch))?;
        for pen in path[2..].split_whitespace() {
            let bytes = pen.as_bytes();
            if bytes.len() % 2 != 0 || !bytes.iter().all(u8::is_ascii_digit) {
                return Err(TextError::InvalidFont);
            }
            let points: Vec<_> = bytes
                .chunks_exact(2)
                .map(|p| {
                    MmPoint::new(
                        x + f64::from(p[0] - b'0') * scale,
                        y + f64::from(p[1] - b'0') * scale,
                    )
                })
                .collect();
            for pair in points.windows(2) {
                if pair[0] != pair[1] {
                    geometries.push(SemanticGeometry::Line {
                        start: pair[0],
                        end: pair[1],
                        width_mm: width,
                    });
                }
            }
        }
        x += 6. * scale + layout.tracking_mm;
    }
    let b = editor_core::geometries_bounds(&geometries, &[])
        .map_err(|_| TextError::InvalidTopology)?
        .ok_or(TextError::InvalidArgument)?;
    let ax = match layout.h_align {
        HorizontalAlign::Left => b.min_x_mm,
        HorizontalAlign::Center => (b.min_x_mm + b.max_x_mm) * 0.5,
        HorizontalAlign::Right => b.max_x_mm,
    };
    let ay = match layout.v_align {
        VerticalAlign::Baseline => 0.,
        VerticalAlign::Bottom => b.min_y_mm,
        VerticalAlign::Middle => (b.min_y_mm + b.max_y_mm) * 0.5,
        VerticalAlign::Top => b.max_y_mm,
    };
    let (sin, cos) = layout.rotation_deg.to_radians().sin_cos();
    let transform = |p: &mut MmPoint| {
        let x = p.x_mm - ax;
        let y = p.y_mm - ay;
        *p = MmPoint::new(
            x * cos - y * sin + layout.x_mm,
            x * sin + y * cos + layout.y_mm,
        );
    };
    for g in &mut geometries {
        if let SemanticGeometry::Line { start, end, .. } = g {
            transform(start);
            transform(end);
        }
    }
    Ok(geometries)
}
