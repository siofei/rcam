# S4-D1 acceptance addendum

schema_version=2; stage=S4-D1; Mac-first bounded; independent of frozen 96 AT cases.

| Case | Required evidence |
|---|---|
| D1-01 | CSV/TSV/BOM/quotes/reordered columns/UTF-8, explicit mm/inch and CCW/CW/offset/axes/Side |
| D1-02 | Missing/duplicate columns, bad UTF-8/numbers/Side/RefDes, duplicate (Side,RefDes), exact budgets, zero project/history/ID mutation |
| D1-03 | Rigid transform identity/translation/arbitrary angle/reflection/inverse/direction, two-point fit/degenerate/distance mismatch/no scale |
| D1-04 | Stable IDs; list/search use bounded revision-fenced pages and exact/prefix/substring/Side/Footprint filters; get uses stable-ID lookup |
| D1-05 | Import/registration single history transaction, mixed geometry/Board Undo/Redo, dirty/no-op/conflict/redo preservation |
| D1-06 | v1 None/empty migration, v2 deterministic Save/Open/Recovery, invalid Board/JSON/ZIP fail-closed |
| D1-07 | Gerber byte identity and no manufacturing objects from metadata/view operations |
| D1-08 | Worker mapping/diagnostic preview/cancel, late reply safety, virtualized rows and pure session filters |
| D1-09 | Zoom/ppp overlay direction/position and camera focus; uncalibrated warning; object/grid registration picking |
| D1-10 | Release 100k query/list/import measured resource budgets |
| D1-11 | Native Apple Silicon/Metal final binary: task workflow, actual vs injected input distinguished, private state restored |
| D1-12 | Diagnostics counts/failure/categories with no source content or client paths |
| D1-13 | Required full/regression gates + clean commit + complete four-file delivery, fresh full-source extraction and per-file comparison |

D1-01–D1-13: PASS (Mac-first bounded), 2026-10-01, after candidate tests/native/archive audits. Actual observations, evidence kinds and final clean identities belong to S4_D1_REVIEW and versioned exports review. Windows deferred / not executed; physical human input is not claimed.
