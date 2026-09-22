//! UI Component Foundation (S4-B2 Final Closeout): shared design tokens,
//! icon abstraction, button semantics, modal footer widgets and the
//! extracted LayerRow component. A behavior-preserving refactor — every
//! module here reproduces an existing call site's exact
//! layout/spacing/labels/styling, never redesigns it.
//!
//! No `command_widgets` module: a `CommandId`-driven label helper was
//! built and tested against `editor_core::command::standard_commands`, but
//! every real toolbar/menu button it could replace (`main.rs`'s File/Edit
//! menus) already shows text that has drifted from the registry — a
//! different duplicate/delete label ("原位复制"/"删除对象" vs the
//! registry's "复制对象"/"删除"), a different Delete shortcut glyph ("⌫"
//! vs the registry's own `Key::Delete` hint), a different Redo shortcut
//! order/format ("Shift+⌘Z" vs "⌘⇧Z"). `CommandDispatcher` is still a
//! placeholder (that module's own doc comment says so), so nothing has
//! reconciled the registry against the UI yet. Wiring the helper into any
//! of those call sites now would silently change displayed text with no
//! way to know, from this pass alone, which side is the one actually
//! correct — the opposite of a behavior-preserving refactor. That
//! reconciliation is real follow-up work, not something to paper over here.
pub mod buttons;
pub mod icons;
pub mod layer_row;
pub mod modal_widgets;
pub mod tokens;
