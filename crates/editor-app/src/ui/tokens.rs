//! Centralized layout/colour constants (S4-B2 Final Closeout UI Component
//! Foundation), with theme-aware semantic text colours.
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

/// Modal width is determined by style and viewport, never dynamic content.
pub const MODAL_MARGIN: f32 = 48.;
pub fn modal_width(ctx: &egui::Context, preferred: f32, min: f32) -> f32 {
    let available = (ctx.content_rect().width() - MODAL_MARGIN).max(1.);
    preferred.min(available).max(min.min(available))
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

/// Warning/attention text on themed panels. Pure yellow is unreadable on light panels.
pub fn warning_text(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        Color32::from_rgb(255, 205, 96)
    } else {
        Color32::from_rgb(120, 65, 0)
    }
}

/// Grip sizes are physical pixels; divide by pixels_per_point when painting.
pub const GRIP_MARKER_PX: f32 = 8.;
pub const GRIP_HIT_PX: f32 = 10.;
pub const GRIP_NORMAL: Color32 = Color32::LIGHT_BLUE;
pub const GRIP_HOVER: Color32 = Color32::WHITE;
pub const GRIP_ACTIVE: Color32 = Color32::LIGHT_GREEN;

/// D2 transient bounds/window overlays, fixed physical pixel outline.
pub const CANDIDATE_WINDOW: Color32 = Color32::GOLD;
pub const CANDIDATE_BOUNDS: Color32 = Color32::LIGHT_BLUE;
pub const CANDIDATE_OUTLINE_PX: f32 = 1.5;

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(color: Color32) -> f64 {
        let linear = |value: u8| {
            let value = f64::from(value) / 255.;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
    }
    #[test]
    fn warning_text_has_high_contrast_on_light_and_dark_panel_surfaces() {
        for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
            let foreground = warning_text(&visuals);
            assert_eq!(foreground.a(), 255);
            for background in [
                visuals.panel_fill,
                visuals.window_fill,
                visuals.extreme_bg_color,
                visuals.widgets.noninteractive.bg_fill,
                visuals.widgets.hovered.bg_fill,
            ] {
                let (fg, bg) = (luminance(foreground), luminance(background));
                let contrast = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                assert!(
                    contrast >= 4.5,
                    "dark={} contrast={contrast}",
                    visuals.dark_mode
                );
            }
        }
        assert_ne!(
            warning_text(&egui::Visuals::light()),
            warning_text(&egui::Visuals::dark())
        );
    }
}
