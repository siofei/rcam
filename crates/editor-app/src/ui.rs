//! UI Component Foundation (S4-B2 Final Closeout): shared design tokens,
//! icon abstraction, button semantics, modal footer widgets and the
//! extracted LayerRow component. A behavior-preserving refactor — every
//! module here reproduces an existing call site's exact
//! layout/spacing/labels/styling, never redesigns it.
//!
pub mod buttons;
pub mod command_widgets;
pub mod icons;
pub mod layer_row;
pub mod modal_widgets;
pub mod tokens;
