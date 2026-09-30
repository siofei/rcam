# S4-D1 PCB / PnP / RefDes Foundation plan

Status: PASS (Mac-first bounded), 2026-10-01. Baseline clean C5 commit 484734984dc57122e975d2ce30211e32e95f72d7. Scope follows S4_D1_TASK.md and ADR 0042; Mac-first bounded only.

Requirements R03/R06/R08/R09/R11/R15/R16/R17/R18/R19/R20/R21/R22. Existing local AT mappings: AT-007/023/025/039–042/058–059/063–066/069/074/082–086/088–095/097. Their identities/expected/required_platforms and the 96-case schema-v2 baseline remain frozen; new behavior is tested in D1 addendum.

Allowed modules: editor-core board/pnp and project-history extension, editor-service components/project/dispatch, rcam-project model/codec/manifest/migration, editor-app component UI/worker/overlay and evidence feature, diagnostics allowlisted summaries, synthetic fixtures, targeted tests, gate/package scripts, stage documentation and source manifest. Do not alter Gerber semantics/writer, private inputs, fonts, C5 behavior or dependency lockfile.

Sequence: freeze ADR/schema/transactions → bounded parser/registration → service queries and atomic history → codec/Save/Open/Recovery → import mapping preview + registration + virtual list/focus overlay → targeted tests and 100k benchmark → full gates/release → exact-final native/Metal/diagnostics → clean commit and complete deterministic source/public evidence four-file delivery → review and stop.

Candidate gates, native workflow and full-source archive audits completed successfully; final identity is bound by the versioned exports review. Windows/full V1/CORE10/P100K are deferred, no D2 work.
