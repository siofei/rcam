# S5-M2-B candidate04 independent review remediation

2026-10-03; REQUEST CHANGES, P1-01/02/03. No commit before source re-review.
Stage S5-M2-B; R09/R11/R16/R17/R18/R19/R22; AT-032/039/040/041/043/065/067/075/090/091/092 bounded evidence mapping. Allowed edits: internal native_batch_drag recorder, its existing native_s5m1 worker observation hook, a cfg(internal-evidence) read-only Gesture threshold getter, Python verification/tests/packaging and documents. Manufacturing, A2 lifecycle, GPU algorithm and frozen protocol/budgets remain unchanged.

P1-01: retain every input frame with contiguous IDs from 1 to recorder finish's independently recorded final completed observation ID. Confirmed and release events identify their input frames; phase 6 must equal exactly the intervening IDs. Derive intervals from every adjacent input clock including confirmed-to-first-preview and last-preview-to-release. Bound both event edges by the existing 200ms stall limit. Percentiles and max include these boundary intervals; no deleted or rephased samples may be omitted.

P1-02: record actual injected RawInput pointer/buttons/cancel flags, input and post-update camera/rect/ppp, actual Gesture last/confirmed/dragging, and exact trajectory origin clock. Calculate elapsed from the input clock against the origin, then replay the unchanged frozen trajectory and f32 screen rounding. Screen-coordinate comparison uses at most 1e-4 logical point (well within the existing 0.1 physical-pixel guard); manufacture delta is independently reconstructed with the camera's f32 subtraction and f64 mm conversion, retaining 1e-9mm; actual paint remains 1e-4mm. Four physical-pixel threshold is the existing Drag constant, not a new tuning parameter. Missing/zero/synchronously corrupted previews and changed camera must reject.

P1-03: completion events explicitly reference the previous production callback frame and its completed monotonic clock; require exact current snapshot state and successful paint/GPU before timing ends. New-project cancellation waits for that visible empty-project frame. Worker observations share the recorder monotonic origin, have unique service sequence, success/error, actual result model state and counts; completion links the matching successful worker, with input <= worker start <= worker finish <= observed visible frame <= GPU complete <= event. Non-worker cancellation links a post-release zero-preview frame and no mutation worker. Screenshot requests/responses carry round-trip user data, dimensions and content SHA; complete PPM data and request/frame links are checked.

Validation: reproduce the review's frame truncation/phase mutation, all-zero or missing delta, unpainted/GPU-failed completion, worker-error/state/sequence contradictions and new-project model-only completion as remanifested real-package negatives. Keep candidate04 bytes and the independent report unchanged. New recorder fields require a new release binary and all 16 native runs, never manually enrich old evidence. Run applicable fmt/check/clippy/tests and new verifier regressions; freeze a new Source/Evidence candidate for parent re-review. Only after PASS: commit, independent clean target/native and final same-commit package audit. No Windows/K1/next stage work.

## Candidate06: remaining P1-01 and P2 (2026-10-03)

Candidate05 independent re-review closes P1-02/03 but demonstrates single first/
middle/last preview deletion with complete renumbering is still accepted. The
previous claim that all renumbered deletions reject was too broad: only a long
tail had been covered. Candidate05 and its original review stay unchanged.

This repair is verifier/tests/docs only (no Rust, new recorder or new native).
For this frozen single-canvas recorder, every recorded production callback must
have exactly one sample: each painted frame consumes draw +1 and uniform-upload
+112 bytes (seven four-lane 32-bit vectors in gpu::Uniforms); an unpainted startup
frame consumes neither. Validate each increment from zero, cumulative counts,
and final totals against the separately captured run counters. Full confirmed /
preview / release coverage therefore cross-checks actual prepare/paint counters,
not just renumberable input IDs. Multiple callbacks without corresponding samples
reject; they are not silently normalized. Add first/middle/last single deletion
and a short chunk, all fully renumbered and remanifested.

Worker-only elapsed time is derived from batch.finished_ns - batch.started_ns.
The earlier legacy elapsed sample is redundant, must be finite and positive,
and must precede the later clock sample (1ns numeric rounding slack). Allow at
most 1ms between these two observation reads; larger disagreement rejects the
measurement. This is an observation consistency bound, not a change to any
operation/performance budget. Current actual maximum gap is 0.013958ms. A zero
or contradictory legacy duration must reject; derived values are reported in a
separate audit without rewriting the original native summaries.

Precommit identity: new validator Source and unchanged candidate05 Observation
Source/Evidence are separate artifacts. A dedicated read-only review verifier
checks both complete manifests, same base commit, an exact scripts/docs-only
change allowlist, and that imported verifier modules come from the new Source,
then validates old evidence against its original source/binary identity with the
new checks. Default production evidence verifier is not weakened. The review
result explicitly says PRECOMMIT_REVIEW_PASS, never clean/same-commit PASS.
No Cargo/native rerun is needed for unchanged Rust in this precommit repair;
actual retained 19 gates/16 runs keep the candidate05 identity. Re-review precedes
commit and the later required clean independent-target/native/same-commit audit.
