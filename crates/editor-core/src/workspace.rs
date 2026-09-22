//! Multi-layer Workspace and view-style model (S4-B1).
//!
//! Everything here is *workspace / view state*: colours, display mode,
//! visibility, selectability and locking never enter `SemanticDocument`, never
//! change a Gerber writer byte and never reach manufacturing Undo history.
//! Classification (`DisplayClass`) is derived from semantic geometry, aperture
//! definition kind and object origin - never from GPU meshes or pixels.

use crate::{ApertureShape, ObjectOrigin, SemanticGeometry, SemanticLayer, SemanticObject};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hasher;

/// Manufacturing layer kind. The Workspace is deliberately not "Layer == Gerber".
/// `Drill` is reserved; the service refuses to create one until Drill import exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Gerber,
    Drill,
}

/// 8-bit sRGB colour serialised as `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.is_ascii() {
            return None;
        }
        let value = u32::from_str_radix(digits, 16).ok()?;
        Some(Self::rgb(
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ))
    }

    pub fn to_f32(self) -> [f32; 3] {
        [
            f32::from(self.r) / 255.,
            f32::from(self.g) / 255.,
            f32::from(self.b) / 255.,
        ]
    }

    /// WCAG relative luminance.
    pub fn relative_luminance(self) -> f64 {
        fn channel(v: u8) -> f64 {
            let c = f64::from(v) / 255.;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG contrast ratio against another colour.
    pub fn contrast_ratio(self, other: Self) -> f64 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// CIE76 colour distance in L*a*b* (D65).
    pub fn delta_e(self, other: Self) -> f64 {
        let (a, b) = (self.lab(), other.lab());
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    fn lab(self) -> [f64; 3] {
        fn lin(v: u8) -> f64 {
            let c = f64::from(v) / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        let (r, g, b) = (lin(self.r), lin(self.g), lin(self.b));
        let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
        let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
        let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
        fn f(t: f64) -> f64 {
            if t > 216. / 24389. {
                t.cbrt()
            } else {
                (24389. / 27. * t + 16.) / 116.
            }
        }
        let (fx, fy, fz) = (f(x), f(y), f(z));
        [116. * fy - 16., 500. * (fx - fy), 200. * (fy - fz)]
    }

    fn from_hsl(h: f64, s: f64, l: f64) -> Self {
        let h = h.rem_euclid(360.);
        let c = (1. - (2. * l - 1.).abs()) * s;
        let x = c * (1. - ((h / 60.) % 2. - 1.).abs());
        let m = l - c / 2.;
        let (r, g, b) = match (h / 60.) as u32 {
            0 => (c, x, 0.),
            1 => (x, c, 0.),
            2 => (0., c, x),
            3 => (0., x, c),
            4 => (x, 0., c),
            _ => (c, 0., x),
        };
        let q = |v: f64| ((v + m).clamp(0., 1.) * 255.).round() as u8;
        Self::rgb(q(r), q(g), q(b))
    }

    fn to_hsl(self) -> (f64, f64, f64) {
        let (r, g, b) = (
            f64::from(self.r) / 255.,
            f64::from(self.g) / 255.,
            f64::from(self.b) / 255.,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.;
        let d = max - min;
        if d == 0. {
            return (0., 0., l);
        }
        let s = d / (1. - (2. * l - 1.).abs());
        let h = if max == r {
            60. * (((g - b) / d) % 6.)
        } else if max == g {
            60. * ((b - r) / d + 2.)
        } else {
            60. * ((r - g) / d + 4.)
        };
        (h.rem_euclid(360.), s, l)
    }
}

impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Color::from_hex(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid colour {text:?}; use #rrggbb"))
        })
    }
}

/// Canvas clear colour used by the production and reference renderers.
pub const CANVAS_BACKGROUND: Color = Color::rgb(14, 18, 22);

/// Curated auto-colour table: furthest-point sampling in L*a*b* so consecutive
/// layers differ strongly, all readable (contrast >= 5.5) on `CANVAS_BACKGROUND`.
const AUTO_PALETTE: [Color; 16] = [
    Color::rgb(108, 229, 197),
    Color::rgb(239, 57, 239),
    Color::rgb(239, 93, 57),
    Color::rgb(239, 239, 57),
    Color::rgb(57, 142, 239),
    Color::rgb(235, 142, 186),
    Color::rgb(57, 239, 93),
    Color::rgb(235, 198, 142),
    Color::rgb(152, 196, 225),
    Color::rgb(200, 132, 245),
    Color::rgb(165, 216, 121),
    Color::rgb(239, 154, 57),
    Color::rgb(245, 147, 132),
    Color::rgb(242, 95, 193),
    Color::rgb(57, 239, 154),
    Color::rgb(166, 239, 57),
];

/// Deterministic auto colour for the `index`-th layer ever created in a document.
/// The table wraps after 16 layers; colours stay user-editable per layer.
pub fn auto_layer_color(index: usize) -> Color {
    AUTO_PALETTE[index % AUTO_PALETTE.len()]
}

/// Whether layer colours or per-category colours drive rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    #[default]
    LayerColor,
    CategoryColor,
}

/// Display-only rendering mode. Diagnostic modes never alter manufacturing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerDisplayMode {
    /// Real manufacturing composite (Dark/Clear, sweeps, holes).
    #[default]
    Filled,
    /// True manufacturing boundary as a hairline.
    Outline,
    /// Stroke centre-line / Flash and Region contour hairline.
    ZeroWidth,
}

impl LayerDisplayMode {
    pub const ALL: [Self; 3] = [Self::Filled, Self::Outline, Self::ZeroWidth];
    pub fn code(self) -> u32 {
        match self {
            Self::Filled => 0,
            Self::Outline => 1,
            Self::ZeroWidth => 2,
        }
    }
}

/// View/runtime classification of a manufacturing object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayClass {
    Stroke,
    FlashCircle,
    FlashRectangle,
    FlashObround,
    FlashPolygon,
    ApertureMacro,
    /// Reserved: the parser rejects `%AB` today, so nothing produces this yet.
    /// This is a *Gerber* aperture block, never RCam's own reusable Block
    /// (S4-B2 places those under [`Self::BlockInstance`] instead).
    ApertureBlock,
    RegionFreeform,
    GeneratedText,
    /// A placed RCam [`crate::block::BlockInstance`] (S4-B2).
    BlockInstance,
    Other,
}

impl DisplayClass {
    pub const ALL: [Self; 11] = [
        Self::Stroke,
        Self::FlashCircle,
        Self::FlashRectangle,
        Self::FlashObround,
        Self::FlashPolygon,
        Self::ApertureMacro,
        Self::ApertureBlock,
        Self::RegionFreeform,
        Self::GeneratedText,
        Self::BlockInstance,
        Self::Other,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(10)
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Other)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Stroke => "stroke",
            Self::FlashCircle => "flash_circle",
            Self::FlashRectangle => "flash_rectangle",
            Self::FlashObround => "flash_obround",
            Self::FlashPolygon => "flash_polygon",
            Self::ApertureMacro => "aperture_macro",
            Self::ApertureBlock => "aperture_block",
            Self::RegionFreeform => "region_freeform",
            Self::GeneratedText => "generated_text",
            Self::BlockInstance => "block_instance",
            Self::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stroke => "线 / Stroke",
            Self::FlashCircle => "圆 / Circle",
            Self::FlashRectangle => "方形 / Rectangle",
            Self::FlashObround => "Obround",
            Self::FlashPolygon => "Polygon",
            Self::ApertureMacro => "AM",
            Self::ApertureBlock => "Block",
            Self::RegionFreeform => "散图 / Region",
            Self::GeneratedText => "文字 / Text",
            Self::BlockInstance => "块 / Block",
            Self::Other => "其他 / Other",
        }
    }

    /// (hue shift in degrees, lightness delta) used for the automatic category colour.
    fn variant(self) -> (f64, f64) {
        match self {
            Self::Stroke => (0., 0.),
            Self::FlashCircle => (28., 0.),
            Self::FlashRectangle => (-28., 0.),
            Self::FlashObround => (56., 0.05),
            Self::FlashPolygon => (-56., 0.05),
            Self::ApertureMacro => (92., -0.06),
            Self::ApertureBlock => (-90., 0.10),
            Self::RegionFreeform => (90., -0.10),
            Self::GeneratedText => (140., 0.10),
            Self::BlockInstance => (-140., 0.05),
            Self::Other => (0., -0.20),
        }
    }
}

/// Automatic per-category colour derived from a layer colour (deterministic).
pub fn class_variant_color(base: Color, class: DisplayClass) -> Color {
    let (shift, delta) = class.variant();
    let (h, s, l) = base.to_hsl();
    let saturation = if class == DisplayClass::Other {
        s * 0.25
    } else {
        s
    };
    Color::from_hsl(
        h + shift,
        saturation.clamp(0.25, 0.9),
        (l + delta).clamp(0.30, 0.85),
    )
}

/// Classify one object without looking at renderer output.
pub fn classify_object(
    object: &SemanticObject,
    apertures: &HashMap<&str, &ApertureShape>,
) -> DisplayClass {
    if matches!(object.origin, ObjectOrigin::GeneratedText { .. }) {
        return DisplayClass::GeneratedText;
    }
    match &object.geometry {
        SemanticGeometry::Line { .. }
        | SemanticGeometry::RectangularSweep { .. }
        | SemanticGeometry::Arc { .. } => DisplayClass::Stroke,
        SemanticGeometry::Region { .. } => DisplayClass::RegionFreeform,
        SemanticGeometry::Flash { aperture_id, .. } => match apertures.get(aperture_id.as_str()) {
            Some(ApertureShape::Circle { .. }) => DisplayClass::FlashCircle,
            Some(ApertureShape::Rectangle { .. }) => DisplayClass::FlashRectangle,
            Some(ApertureShape::Obround { .. }) => DisplayClass::FlashObround,
            Some(ApertureShape::Polygon { .. }) => DisplayClass::FlashPolygon,
            Some(ApertureShape::Macro { .. }) => DisplayClass::ApertureMacro,
            None => DisplayClass::Other,
        },
        SemanticGeometry::BlockInstance { .. } => DisplayClass::BlockInstance,
    }
}

/// Aperture lookup used by `classify_object`.
pub fn aperture_shape_map(
    apertures: &[crate::ApertureDefinition],
) -> HashMap<&str, &ApertureShape> {
    apertures
        .iter()
        .map(|aperture| (aperture.id.as_str(), &aperture.shape))
        .collect()
}

/// Per-category Visible / Selectable / Locked (independent) and optional colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassInteractionStyle {
    pub visible: bool,
    pub selectable: bool,
    pub locked: bool,
    pub color_override: Option<Color>,
}

impl Default for ClassInteractionStyle {
    fn default() -> Self {
        Self {
            visible: true,
            selectable: true,
            locked: false,
            color_override: None,
        }
    }
}

pub type DisplayClassStyles = BTreeMap<DisplayClass, ClassInteractionStyle>;

pub fn default_class_styles() -> DisplayClassStyles {
    DisplayClass::ALL
        .into_iter()
        .map(|class| (class, ClassInteractionStyle::default()))
        .collect()
}

/// Rendering style of one layer. Never serialised into Gerber.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerViewStyle {
    pub base_color: Color,
    pub color_mode: ColorMode,
    pub display_mode: LayerDisplayMode,
    pub classes: DisplayClassStyles,
}

impl LayerViewStyle {
    pub fn new(base_color: Color) -> Self {
        Self {
            base_color,
            color_mode: ColorMode::LayerColor,
            display_mode: LayerDisplayMode::Filled,
            classes: default_class_styles(),
        }
    }
}

/// Workspace state of one layer (session-only until `.rcam` exists in S4-B2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerWorkspaceState {
    pub kind: LayerKind,
    pub display_name: String,
    pub visible: bool,
    pub selectable: bool,
    pub locked: bool,
    pub style: LayerViewStyle,
}

impl LayerWorkspaceState {
    pub fn new(kind: LayerKind, display_name: impl Into<String>, base_color: Color) -> Self {
        Self {
            kind,
            display_name: display_name.into(),
            visible: true,
            selectable: true,
            locked: false,
            style: LayerViewStyle::new(base_color),
        }
    }

    pub fn class_style(&self, class: DisplayClass) -> ClassInteractionStyle {
        self.style.classes.get(&class).copied().unwrap_or_default()
    }

    /// `layer.visible && class.visible`
    pub fn effective_visible(&self, class: DisplayClass) -> bool {
        self.visible && self.class_style(class).visible
    }

    /// `effective_visible && layer.selectable && class.selectable`
    pub fn effective_selectable(&self, class: DisplayClass) -> bool {
        let style = self.class_style(class);
        self.effective_visible(class) && self.selectable && style.selectable
    }

    /// `layer.locked || class.locked`
    pub fn effective_locked(&self, class: DisplayClass) -> bool {
        self.locked || self.class_style(class).locked
    }

    /// Colour used for drawing objects of `class` on this layer.
    pub fn effective_color(&self, class: DisplayClass) -> Color {
        match self.style.color_mode {
            ColorMode::LayerColor => self.style.base_color,
            ColorMode::CategoryColor => self
                .class_style(class)
                .color_override
                .unwrap_or_else(|| class_variant_color(self.style.base_color, class)),
        }
    }
}

/// Import-time information kept for display only; never a link to the disk file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportProvenance {
    pub import_id: String,
    pub original_file_name: String,
    pub imported_sha256: String,
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub imported_at: String,
}

/// UTC timestamp string from Unix seconds (no external date dependency).
pub fn utc_timestamp(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let rem = unix_seconds % 86_400;
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Objects of one imported layer at import time, used to count "modified" objects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportBaseline {
    objects: HashMap<String, u64>,
}

fn hash_point(h: &mut impl Hasher, p: crate::MmPoint) {
    h.write_u64(p.x_mm.to_bits());
    h.write_u64(p.y_mm.to_bits());
}

fn hash_arc(h: &mut impl Hasher, a: &crate::ArcGeometry) {
    hash_point(h, a.start);
    hash_point(h, a.end);
    hash_point(h, a.center);
    h.write_u8(a.direction as u8);
    h.write_u8(u8::from(a.full_circle));
    if let Some(source) = a.source {
        h.write_u8(1);
        h.write_u64(source.resolution_mm.to_bits());
        h.write_u8(u8::from(source.single_quadrant));
    } else {
        h.write_u8(0);
    }
}

/// Stable in-process fingerprint of manufacturing geometry (bit-exact f64).
pub fn geometry_fingerprint(geometry: &SemanticGeometry) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match geometry {
        SemanticGeometry::Flash {
            center,
            aperture_id,
            transform,
        } => {
            h.write_u8(1);
            hash_point(&mut h, *center);
            h.write(aperture_id.as_bytes());
            h.write_u8(transform.mirror as u8);
            h.write_u64(transform.rotation_deg.to_bits());
            h.write_u64(transform.scale.to_bits());
        }
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => {
            h.write_u8(2);
            hash_point(&mut h, *start);
            hash_point(&mut h, *end);
            h.write_u64(width_mm.to_bits());
        }
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => {
            h.write_u8(3);
            hash_point(&mut h, *start);
            hash_point(&mut h, *end);
            h.write_u64(width_mm.to_bits());
            h.write_u64(height_mm.to_bits());
        }
        SemanticGeometry::Arc { path, width_mm } => {
            h.write_u8(4);
            hash_arc(&mut h, path);
            h.write_u64(width_mm.to_bits());
        }
        SemanticGeometry::Region { contours } => {
            h.write_u8(5);
            for contour in contours {
                h.write_u8(contour.role as u8);
                for edge in &contour.edges {
                    match edge {
                        crate::RegionEdge::Line { start, end } => {
                            h.write_u8(0);
                            hash_point(&mut h, *start);
                            hash_point(&mut h, *end);
                        }
                        crate::RegionEdge::Arc(arc) => {
                            h.write_u8(1);
                            hash_arc(&mut h, arc);
                        }
                    }
                }
            }
        }
        SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } => {
            h.write_u8(6);
            h.write(definition_id.0.as_bytes());
            hash_point(&mut h, transform.translation);
            h.write_u64(transform.rotation_deg.to_bits());
            h.write_u8(transform.mirror as u8);
        }
    }
    h.finish()
}

impl ImportBaseline {
    pub fn capture(layer: &SemanticLayer) -> Self {
        Self {
            objects: layer
                .objects
                .iter()
                .filter(|object| matches!(object.origin, ObjectOrigin::Imported { .. }))
                .map(|object| {
                    (
                        object.object_id.clone(),
                        geometry_fingerprint(&object.geometry),
                    )
                })
                .collect(),
        }
    }

    pub fn imported_count(&self) -> usize {
        self.objects.len()
    }
}

/// Formal "what does this layer contain" summary used by Delete confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LayerContentSummary {
    pub object_count: usize,
    pub imported_object_count: usize,
    pub generated_object_count: usize,
    /// Imported objects whose geometry changed, plus imported objects deleted.
    pub modified_object_count: usize,
    pub has_import_provenance: bool,
    pub manufacturing_dirty: bool,
}

/// How risky deleting a layer is. Independent of the source type of the layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteRisk {
    /// `object_count == 0`.
    Empty,
    /// Non-empty, no unsaved manufacturing change.
    NonEmptyClean,
    /// Non-empty with modified or generated content.
    NonEmptyDirty,
}

impl LayerContentSummary {
    /// Formal definition of an empty layer: no manufacturing object at all,
    /// even when import provenance exists.
    pub fn is_empty(&self) -> bool {
        self.object_count == 0
    }

    pub fn delete_risk(&self) -> DeleteRisk {
        if self.is_empty() {
            DeleteRisk::Empty
        } else if self.manufacturing_dirty {
            DeleteRisk::NonEmptyDirty
        } else {
            DeleteRisk::NonEmptyClean
        }
    }
}

pub fn summarize_layer(
    layer: &SemanticLayer,
    baseline: Option<&ImportBaseline>,
    has_import_provenance: bool,
) -> LayerContentSummary {
    let mut imported = 0;
    let mut generated = 0;
    let mut changed = 0;
    for object in &layer.objects {
        match object.origin {
            ObjectOrigin::Imported { .. } => {
                imported += 1;
                if let Some(original) = baseline.and_then(|b| b.objects.get(&object.object_id))
                    && *original != geometry_fingerprint(&object.geometry)
                {
                    changed += 1;
                }
            }
            ObjectOrigin::Generated { .. } | ObjectOrigin::GeneratedText { .. } => generated += 1,
        }
    }
    let deleted = baseline.map_or(0, |b| b.imported_count().saturating_sub(imported));
    let modified = changed + deleted;
    LayerContentSummary {
        object_count: layer.objects.len(),
        imported_object_count: imported,
        generated_object_count: generated,
        modified_object_count: modified,
        has_import_provenance,
        manufacturing_dirty: generated > 0 || modified > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_hex_round_trip_and_serde() {
        let color = Color::from_hex("#0aBf7c").unwrap();
        assert_eq!(color, Color::rgb(10, 191, 124));
        assert_eq!(color.to_hex(), "#0abf7c");
        use serde::de::IntoDeserializer;
        let parsed = |text: &'static str| {
            Color::deserialize(IntoDeserializer::<serde::de::value::Error>::into_deserializer(text))
        };
        assert_eq!(parsed("#0abf7c").unwrap(), color);
        for bad in ["0abf7c", "#0abf7", "#0abf7cd", "#ggggg0", "#é0abf7"] {
            assert!(Color::from_hex(bad).is_none(), "{bad}");
        }
        assert!(parsed("red").is_err());
    }

    #[test]
    fn auto_palette_is_deterministic_distinct_and_readable() {
        assert_eq!(auto_layer_color(0), auto_layer_color(0));
        let colors: Vec<_> = (0..16).map(auto_layer_color).collect();
        for (i, a) in colors.iter().enumerate() {
            assert!(
                a.contrast_ratio(CANVAS_BACKGROUND) >= 4.5,
                "colour {i} {a:?} is too dark on the canvas"
            );
            for (j, b) in colors.iter().enumerate().skip(i + 1) {
                // The first eight layers (the common case) stay clearly apart;
                // beyond that the palette wraps and only needs to stay distinguishable.
                let floor = if j < 8 { 40. } else { 25. };
                assert!(
                    a.delta_e(*b) >= floor,
                    "colours {i}/{j} are too close: {a:?} {b:?}"
                );
            }
            if let Some(next) = colors.get(i + 1) {
                assert!(
                    a.delta_e(*next) >= 30.,
                    "neighbours {i}/{} must differ strongly",
                    i + 1
                );
            }
        }
    }

    #[test]
    fn category_variants_distinguish_block_from_region() {
        for index in 0..16 {
            let base = auto_layer_color(index);
            let block = class_variant_color(base, DisplayClass::ApertureBlock);
            let region = class_variant_color(base, DisplayClass::RegionFreeform);
            assert!(
                block.delta_e(region) >= 40.,
                "layer {index}: {block:?} vs {region:?}"
            );
            assert_ne!(block, base);
            assert_ne!(region, base);
        }
    }

    #[test]
    fn effective_state_formulas() {
        let mut state = LayerWorkspaceState::new(LayerKind::Gerber, "L", Color::rgb(1, 2, 3));
        let am = DisplayClass::ApertureMacro;
        let circle = DisplayClass::FlashCircle;
        state.style.classes.get_mut(&am).unwrap().locked = true;
        assert!(!state.effective_locked(circle));
        assert!(state.effective_locked(am));
        assert!(state.effective_selectable(am), "locked AM stays selectable");
        state.style.classes.get_mut(&circle).unwrap().visible = false;
        assert!(!state.effective_visible(circle));
        assert!(!state.effective_selectable(circle));
        state.style.classes.get_mut(&circle).unwrap().visible = true;
        state.selectable = false;
        assert!(state.effective_visible(circle) && !state.effective_selectable(circle));
        state.selectable = true;
        state.visible = false;
        assert!(!state.effective_visible(am) && !state.effective_selectable(am));
        state.visible = true;
        state.locked = true;
        assert!(state.effective_locked(circle) && state.effective_selectable(circle));
    }

    #[test]
    fn colour_modes_select_layer_or_category_colour() {
        let base = Color::rgb(40, 180, 160);
        let mut state = LayerWorkspaceState::new(LayerKind::Gerber, "L", base);
        assert_eq!(state.effective_color(DisplayClass::RegionFreeform), base);
        state.style.color_mode = ColorMode::CategoryColor;
        let auto = state.effective_color(DisplayClass::RegionFreeform);
        assert_ne!(auto, base);
        state
            .style
            .classes
            .get_mut(&DisplayClass::RegionFreeform)
            .unwrap()
            .color_override = Some(Color::rgb(255, 0, 0));
        assert_eq!(
            state.effective_color(DisplayClass::RegionFreeform),
            Color::rgb(255, 0, 0)
        );
    }

    #[test]
    fn timestamp_formatting() {
        assert_eq!(utc_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_timestamp(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(utc_timestamp(1_789_961_806), "2026-09-21T03:36:46Z");
    }

    #[test]
    fn class_keys_and_indices_are_stable() {
        for (i, class) in DisplayClass::ALL.into_iter().enumerate() {
            assert_eq!(class.index(), i);
            assert_eq!(DisplayClass::from_index(i), class);
            assert!(!class.key().is_empty() && !class.label().is_empty());
        }
        assert_eq!(default_class_styles().len(), DisplayClass::ALL.len());
    }
}
