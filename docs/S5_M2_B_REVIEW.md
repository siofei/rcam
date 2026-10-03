# S5-M2-B Batch Drag — review record

IN_PROGRESS, not accepted. Mac-first bounded scope, requirements/AT mapping and
frozen thresholds in S5_M2_B_PLAN; ADR0052. Parent A2 clean 03576c5 remains frozen.

Implementation: common f64 preview delta; static display bins queried at p and
p-delta with exposure-order merge; borrowed snap exclusions. Existing service
objects.move stays the only commit boundary. No core/service/schema/writer change.
Internal-only recorder uses production native input, worker and Metal callback;
measures UI allocations, CPU, per-frame uploads, completion and exact coordinates.

Evidence intake: exports/S5M2_B_intake_20261002 in the main repository. Existing
release baseline p95 P100K preview-index rebuild 2.758334 ms (small viewport, not
an end-to-end score). Failing index-reuse regression reproduced before repair.
Initial expression compile error preserved, then four-size real service batch
and exact Undo/Redo tests passed. Native production/reference pixel parity passed
on M1 Metal after sandbox adapter-unavailable failure; no software fallback.

Pilot01 failed: the new internal recorder initially missed recovery-directory
isolation and a pre-existing recovery modal blocked drag. No user recovery was
opened/deleted; directory files were still dated Sep 25/Oct 1. The existing
recovery guard prevents autosave while its prompt is open. Add the same isolated
state/recovery directory routing as A2; no Recovery product behavior changed.
The rebuild during that failed pilot also changed its executable path, and its
runner correctly rejected binary identity. Later runs use fixed binary copies.
These pilot failures are retained and cannot count toward acceptance.

Pilot02 (100 selected) passed 361 preview frames; p95 29.0265 ms, maximum
39.243125 ms; commit/Undo/Redo 100.008833/103.992083/103.070125 ms; peak RSS
544899072 bytes. This is a pilot, not the required three-round final matrix.
Pilot03 and complete candidate gates are pending at this record's creation.

All formal automated gates, complete new native matrix, candidate independent
review, clean stage commit, clean/fresh package validation and final audit remain
NOT COMPLETED. Windows, K1 deferred native work, full V1/CORE10/P100K and PMIX
remain outside this acceptance. No push or Library upload.

Candidate01 automated freeze: 14 gates passed, then metal-batch failed with
DISPLAY_PRECISION for the newly added 10000/-20000 mm delta at scene 1000 ppm.
This is the existing 0.1 physical-pixel guard working, not a shader mismatch.
Candidate02 retains all oversized combinations as explicit refusal assertions
and adds 128/-128 mm for exact pixel comparisons: fixture bounds are asserted
fully disjoint after translation, and the original precision limit is unchanged.
No product code or native performance protocol/threshold changed for this repair.
The failed gate and source identity remain under S5M2_B_gates_candidate01.

Pilot03: 500/1000/5000 move and Escape/focus-loss passed the then-current pilot
verifier. PointerGone reached cancellation but failed external metrics because
process exit was sampled as RSS=0 without a process-state field. The runner now
records ps state and accepts zero RSS only for an explicit zombie; living samples
must be positive. No sample is dropped. New-project was not run after that failure.
All pilots remain diagnostic evidence, not formal matrix completion.

Candidate02 completed 19/19 gates and 16/16 native runs, and its Source/Evidence
fresh extraction passed the then-current verifier. Semantic tampering subsequently
found a verifier defect: a moved snapshot with Undo=2 was accepted if the event's
Undo remained 1 and hashes were rebound. This candidate is NOT review-ready.
Candidate03 requires exact snapshot/event state equality, unique ordered snapshot
labels and unique associated completion events, including empty new-project
snapshots. The verifier now has 34 passing tests; all five package tamper probes
reject under the repaired verifier, and the original 16 runs remain valid.
Original native data has no snapshot/event state discrepancy in all 16 runs.
The verifier-only repair still gets a new manifest, build/gates and native matrix;
Candidate02's logs/package and failed negative check are retained unchanged.

Candidate03 completed 19 gates, but native round-2/1000 was rejected for a final
RSS=0 / ps state ?E sample after the report's final screenshot; the app exited 0.
macOS ps(1), inspected locally, defines E as "trying to exit". Candidate04 records
whether the completed report already exists at each sample. Zero RSS requires
both that completion boundary and a documented Z/E terminal state; living states
or missing completion still reject. All raw samples remain; RSS/CPU maxima and
performance budgets are unchanged. Three explicit regressions cover E after
completion, E before completion, and R after completion. Candidate03 remains FAIL.

2026-10-03 independent candidate04 source review: REQUEST CHANGES (three P1).
Review report is local at /Users/lxf/Documents/Codex/2026-10-03/task/
S5M2_B_candidate04_independent_review/独立源审报告.txt. It confirms the actual
19 gates/16 native data and production geometry, but demonstrates missing-frame,
all-zero-preview and false-completion/worker-error evidence being accepted after
rehashing. Candidate04 is not source PASS and cannot be committed as approved.

Remediation is specified in S5_M2_B_EVIDENCE_REVIEW_FIX. The recorder now captures
actual RawInput/view/gesture, a shared worker clock/result state, continuous frame
extent, explicit visible completion frame links, and screenshot round-trip IDs
and full payload hashes. New-project waits for actual empty-project GPU completion.
The verifier binds the full interval and unchanged trajectory, exact manufacturing
conversion and existing paint tolerance, successful workers and all completion
states. No production model/GPU algorithm/A2 lifecycle/budget change was made.

The first compile caught a moved screenshot byte buffer; corrected by borrowing.
The first synthetic-unit run caught a fixture/old event-clock origin mismatch;
transaction duration is now consistently derived from the actual input-frame
clock used by the recorder. These development failures are preserved in intake.
New-recorder pilot (5000 move + all four interruptions) passed, including visible
new-project cancellation. It is diagnostic, not a replacement for a new frozen
16-run matrix. Rehashed real-package probes will include all independent review
counterexamples and additional missing/rephased/renumbered-frame, camera, worker
state/count/clock and cancellation variants. Re-review and commit remain pending.

2026-10-03 candidate05 independent re-review: REQUEST CHANGES. P1-02 (actual
trajectory) and P1-03 (successful worker to visible completion) are independently
closed. P1-01 remains: deleting a single first/middle/last preview sample and
renumbering all references was accepted despite draw +2 / uniform +224 between
remaining samples. The earlier broad renumbered-deletion claim is withdrawn.
Candidate05 and the independent report/accepted counterexamples are preserved.

Candidate06 only changes verifier/tests/docs. Every recorded frame is now bound
from counter zero through final run totals to the independent production draw
and uniform-upload counters: one painted sample = one callback = 112 uniform
bytes; unpainted startup samples consume neither. Full input ID/phase/time and
trajectory/completion checks remain. First/middle/last single-frame and short
chunk removal with full renumbering are explicit package counterexamples. Worker
elapsed is derived from shared batch ticks and the legacy observation must be
positive, finite and consistent with its earlier sampling boundary (<=1ms gap).
The 76 focused Python tests and revalidation of all 16 unchanged candidate05 raw
runs pass. Final remanifested attacks, portable identity tests and extraction
results will be recorded outside Source in candidate06 HANDOFF and raw logs.

The review envelope intentionally separates CandidateSource from unchanged
candidate05 ObservationSource/ObservationEvidence. verify_batch_drag_review.py
checks both complete source manifests and a strict six-file verifier/docs change
allowlist; it runs the new checker against the original observation identity.
It reports PRECOMMIT_REVIEW_PASS only. The ordinary clean evidence verifier and
all Rust/recorder/protocol/Cargo/gate commands are unchanged. This is not a claim
that candidate05 binaries were built from the candidate06 manifest. Cargo and
native are NOT rerun in this verifier-only precommit repair; the retained 19
gates/16 native runs continue to identify candidate05. Independent re-review,
clean stage commit (no push), clean independent-target gates/native and final
same-commit Source/Evidence audit remain required. No phase PASS is declared.
