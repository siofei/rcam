# S4-A2.1 — Mac-first UI and text contour refactor

Scope: R09–R14, R16, R18, R21–R22; local coverage AT-015, AT-031–034,
AT-037, AT-040, AT-043–054, AT-063–065, AT-086–095.
Allowed modules: editor-app, editor-text, editor-core Region validation entry point,
service integration tests, stage documentation and evidence/package scripts.

Implement one integrated text interaction closure: exclusive parameter modal drafts,
translation-only floating placement, canonical contour Line/Arc manufacturing geometry.
Existing Gerber semantics require local retraced cut-ins for holes; independent
contours remain additive. No change to Dark/Clear, writer truth or tolerance limits.
Arc refitting uses certified polygon-to-arc radial bounds plus the original Bezier
convex-hull flattening bound. Offset and integer conversion consume explicit budget.
Native IME/Metal evidence is separate from automated tests. Windows deferred / not
executed. Global Units remains BLOCKED UNTIL S4-A2.1 PASS.
