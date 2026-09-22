//! Centralized layout/colour constants (S4-B2 Final Closeout UI Component
//! Foundation). Every value here is copied from the call site it replaces,
//! not redesigned — this module only gives the scattered literals one name
//! and one place to change, it does not alter any current layout or colour.
use eframe::egui;
use egui::Color32;

/// Spacing scale used across compact rows, palette grids and inline gaps.
pub const SPACING_XS: f32 = 2.;
pub const SPACING_SM: f32 = 3.;
pub const SPACING_MD: f32 = 4.;
pub const SPACING_LG: f32 = 6.;
pub const SPACING_XL: f32 = 8.;

/// LayerRow control sizing.
pub const ROW_SWATCH_SIZE: f32 = 14.;
pub const PALETTE_SWATCH_SIZE: f32 = 20.;
pub const ROW_TOGGLE_MIN_WIDTH: f32 = 22.;
pub const ROW_CORNER_RADIUS: f32 = 3.;

/// Modal sizing: `preferred` clamped to the viewport minus a fixed margin,
/// never smaller than `min` — the exact `W.min(ctx.content_rect().width() -
/// 48.).max(min)` shape every dialog already used ad hoc.
pub const MODAL_MARGIN: f32 = 48.;
pub fn modal_width(ctx: &egui::Context, preferred: f32, min: f32) -> f32 {
    preferred
        .min(ctx.content_rect().width() - MODAL_MARGIN)
        .max(min)
}

/// Semantic colours. `selection_highlight` is the active-row tint; egui's
/// `Color32::from_rgba_unmultiplied` is not `const fn`, so these are plain
/// functions rather than `const` values.
pub fn selection_highlight() -> Color32 {
    Color32::from_rgba_unmultiplied(80, 130, 200, 46)
}
pub fn destructive() -> Color32 {
    Color32::from_rgb(255, 120, 110)
}
