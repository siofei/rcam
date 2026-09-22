//! Icon abstraction (S4-B2 Final Closeout UI Component Foundation): call
//! sites reference `RcamIcon::X.glyph()` instead of a bare glyph literal, so
//! swapping the underlying representation (SVG/vector icons, say) later
//! never touches a call site. Every mapping below is the exact glyph the
//! call site it replaces already used — this is a rename, not a redesign.
//! Scoped to the glyphs the layer row actually renders today; a variant for
//! a concept with no current glyph (Import/Export/Snap/Measure/Text/Block
//! icons, a distinct Hidden/Unlocked glyph) is left for whoever adds that
//! glyph, rather than added speculatively and immediately dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RcamIcon {
    /// Also the glyph for the hidden state — the row highlights which one
    /// applies via `.selected(on)`, not a distinct glyph.
    Visible,
    /// Also the glyph for the unlocked state, for the same reason.
    Locked,
    Filled,
    Outline,
    ZeroWidth,
    More,
    ActiveLayer,
    InactiveLayer,
    DragHandle,
    Solo,
}

impl RcamIcon {
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Visible => "👁",
            Self::Locked => "🔒",
            Self::Filled => "▣",
            Self::Outline => "□",
            Self::ZeroWidth => "─",
            Self::More => "⋯",
            Self::ActiveLayer => "●",
            Self::InactiveLayer => "○",
            Self::DragHandle => "≡",
            Self::Solo => "S",
        }
    }
}
