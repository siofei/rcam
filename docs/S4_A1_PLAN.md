# S4-A1 vector text core — Mac-first

Baseline: S3 commit 21f5a1c42ff55bbfbac24e4598749397334008fb. Scope: S4-A1 only.
Requirements: R05, R11, R12, R14, R16, R20, R21, R22.
Local cases: AT-047–052, AT-054, AT-086, AT-088–092, AT-097;
AT-001/002 and source/evidence package integrity are supporting gates.
This does not complete GUI/IME cases, independent viewer acceptance, Windows,
or the full V1/CORE10 gate. Windows remains deferred / not executed.

Allowed modules: new editor-text, editor-core atomic generated insertion,
editor-service text DTO/dispatch/authorized font reading, service/core text tests,
workspace dependency lock, docs and deterministic packaging scripts. No formal
text GUI, automatic bridging, font manager, new interchange formats, or S4-A2.

Implementation contract: ADR 0024. Real font data remains local and is never
copied into source or evidence archives. Use RCAM_TEXT_FONT and
RCAM_TEXT_FONT_SHA256 to select another explicitly authorized static CJK font.
Mac evidence defaults to the installed Arial Unicode face 0, with a pinned hash.
The test must fail if that font is absent or differs; absence is not a pass.

Exit requires one clean tested commit, all final gates, native macOS headless
font/output/geometry evidence, a full source ZIP, public evidence ZIP, fresh
Python extraction and manifest/package tests, tracked payload equality and
external SHA-256 records for both archives. Public evidence strips private
workspace paths while preserving command outputs and exit codes.

Packaging note: source ZIP metadata contains `source_zip_sha256: null` and an
explicit external-sidecar explanation. Embedding the digest of an entire ZIP
inside itself would be circular. The actual digest is stored in public evidence
and final package verification, alongside the evidence ZIP digest.


Grouped automated coverage (test functions cover multiple parameter cases):

| Requested checks | Implementation evidence |
|---|---|
| ASCII, CJK, glyph holes, height, three horizontal and four vertical anchors | editor-text ascii_cjk_holes_height_and_alignment; winding regression |
| Tracking, 0/90/arbitrary rotation, finite validation | tracking_and_rigid_rotation; invalid_input_is_rejected |
| Quadratic/cubic flatten error, cusps/backtracking | 10,001 independent dense evaluations per synthetic curve |
| Font/hash/index/empty/missing glyph rejection; locked/stale/NaN atomicity | s4a1_text_workflow rejection matrix; editor-text parser tests |
| Generated origin, shared operation, single transaction, stable Undo/Redo | text_transaction_undo_redo_export_reopen_and_metrics |
| Writer/reopen, unchanged input, exact polygon metrics | same test, four real font/rotation/alignment workflows |
| Hole preserves background through export/reopen | hole_keeps_background_and_empty_hole_after_reopen |
| JSON dispatch, capability, strict envelope, no GUI/GPU dependencies | new JSON text test and existing contract/dependency tests |

Metrics report each decomposed polygon, not whole-glyph perimeter or final layer
area. Original font text/group metadata is not recoverable from ordinary Gerber.
