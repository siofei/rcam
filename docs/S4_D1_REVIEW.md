# S4-D1 review

S4-D1 PCB / PnP / RefDes Foundation = **PASS (Mac-first bounded)**, 2026-10-01. This conclusion follows completed implementation-candidate tests, native observations and complete four-file archive audits. Final clean commit, binaries and package identity are authoritative in versioned `exports/S4D1_*/REVIEW.md`; that final source is rerun and repackaged before delivery. Baseline C5: 484734984dc57122e975d2ce30211e32e95f72d7. Isolated branch: codex/s4d1-pnp-refdes; the main mixed checkout is preserved.

## Scope and behavior

R03/R06/R08/R09/R11/R15/R16/R17/R18/R19/R20/R21/R22; local regression AT-007/023/025/039–042/058–059/063–066/069/074/082–086/088–095/097. D1-01–D1-13 are the new addendum; original 96 AT identities/expected/platforms, ACCEPTANCE_V1.md and Cargo.lock remain byte-identical to C5.

Explicit bounded CSV/TSV mapping/units/Side/rotation preview feeds an independent immutable ComponentPlacement table. Board registration is rigid f64 mm only, with no scale, shear or automatic Bottom reflection. List/search/get use stable IDs and revision-fenced bounded pages; GUI search and virtual rows run through the worker. Uncalibrated focus warns and leaves the camera unchanged. Registered focus and marker/arrow/RefDes overlay use the same Board→World transform without creating manufacturing geometry.

Import and registration each occupy one chronological history transaction. Board shares service content revision but manufacturing dirty remains geometry-only; project dirty includes Board. `.rcam` with no Board stays v1; Board projects use v2 with strict validation. Save/Open/Recovery retain exact Board content and registration; Gerber exports remain byte-identical across metadata changes. Default public release omits all active evidence-control entry points.

## Executed checks and evidence

Clean candidate d06f6535baf6491702d93b64e0a6c68b47e7418d: all 16 commands exited 0. Private raw logs: `evidence/s4d1-d06f653-candidate2/`; native observations and binary identity retained in isolated synthetic scratch storage; public final evidence uses relative archive paths. Gates: fmt, workspace/all-target check and clippy, internal-feature clippy, core/service/project PnP and diagnostic tests (9 passed), app component tests (2 passed), full workspace (726 passed, 0 failed, 41 intentionally ignored across 79 targets), automation_contract/headless_workflow, normal service dependency tree, default release build, standalone release 100k measurement, two existing native Metal invariance/parity tests, internal-evidence release build, 467-file manifest check and 2 package self-tests. Window/GPU dependencies are absent from the service normal tree.

Release 100k candidate: preview 113.711 ms, import 203.193 ms, queries/list 0.924–1.121 ms, virtual index 2.444 ms, actual standalone process peak RSS 112,148,480 bytes. Dirty hashing is separately reported (124.929 ms), never hidden in query measurements. Thresholds 5 s import/preview, 100 ms query/list and 512 MiB RSS are unchanged. This measures component data, not manufacturing P100K acceptance.

Native Apple Silicon/Metal: 21 independent-verified records from real EditorApp Actions, worker, ApplicationService and renderer. Executed invalid preview/rejection with zero mutation, explicit valid mapping import, uncalibrated warning, two-point registration using actual C1 object snap features, Bottom/C15 and Top queries, focus coordinates, unit/overlay changes, registration Undo/Redo, project Save/Open, reflected 37° registration Recovery, before/after Gerber byte identity and Diagnostic Package. Actual CUA additionally opened the PnP native picker, previewed invalid rows, cancelled with clean revision/history, and applied Bottom/C15 search. egui controls expose no detailed native accessibility tree; the complete workflow therefore uses the allowed controlled synthetic native path. These are separate evidence kinds; no physical human-input claim.

Candidate archive audit: complete Source ZIP fresh-extracted, manifest and package self-tests executed there, tested-source payload 467/467 hashes matched. Source/Evidence ZIP CRC, sorted entry order, fixed timestamps/permissions and UTF-8 names checked. Repeated evidence generation is byte-identical. EVIDENCE.sha256 and external SHA256SUMS verified. Public evidence includes only allowlisted synthetic artifacts and path-redacted logs; raw host paths/binaries remain private. Final rerun/audit supersedes candidate package identity.

## Limits and stop point

Search controls are drafts until Search / Apply is clicked. A Board revision rebuilds the table with all components; reapply desired draft filters after import/registration/Undo/Redo. Import diagnostics retain at most 100 categories/rows while reporting the full count. There is no continuous source-file link or automatic component-to-opening association.

Windows deferred / not executed. Physical mouse/trackpad/modifier and human IME workflows are not claimed by the controlled native records. Full V1, CORE10 10/10 and manufacturing P100K are not accepted here. Component association/library replacement, Drill and assembly export remain outside D1. Stop before S4-D2.
