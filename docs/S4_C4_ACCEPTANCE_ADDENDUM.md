# S4-C4 acceptance addendum

schema_version=2; local C4 identifiers only. The 96 cases and required_platforms in acceptance_cases.json and ACCEPTANCE_V1.md are unchanged.

| Local case | Required evidence |
|---|---|
| C4-01 | All six align modes: different sizes, transformed apertures, world Y-up and fixed anchor |
| C4-02 | Both distribution axes: equal/unequal sizes, negative gap, ties, fixed endpoints and order independence |
| C4-03 | Same-layer permissions, category policies, stale/missing/duplicate/insufficient targets, atomic failure |
| C4-04 | Eight commands one transaction, exact Undo/Redo, no-op zero revision/history/dirty change |
| C4-05 | Mixed Flash/Line/Arc/Region/Text/CompatibilitySolid/Block; stable IDs and Dark/Clear sequence |
| C4-06 | 100 BlockInstances: definition and orientation invariant, translation only |
| C4-07 | Save/Open/recovery, Gerber export/reopen, source hash unchanged and metrics preserved |
| C4-08 | Shared Menu/Context/Command routes, last-selected marker, selection-order and enablement tests |
| C4-09 | Diagnostics numeric mode/axis, hashed anchor, moved counts, revisions, no-op and privacy |
| C4-10 | Release 1K timings, 10K acceptance/10001 limit, native 1K context command |
| C4-11 | Native Mac align/distribute/negative gap/Block/Undo/Redo/Y-up/anchor/Save/Open/Export/Diagnostics/Metal |
| C4-12 | Final source identity, required full gates, regressions and deterministic full source/evidence ZIP checks |

Status starts NOT EXECUTED; actual results are recorded in S4_C4_REVIEW.md and immutable run evidence. Automated geometry tests cannot replace C4-11 interaction evidence. Windows deferred/not executed.
