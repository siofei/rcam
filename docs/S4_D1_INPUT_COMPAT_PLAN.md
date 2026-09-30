# S4-D1 input compatibility follow-up

Status: implemented; final verification recorded separately in the versioned input-compatibility review. Baseline bc97e028e485abf54dc01bc26551d40425c7d69a; isolated codex/s4d1-pnp-refdes worktree. Scope R03/R11/R15/R17/R19/R20/R21/R22; local regression AT-007/023/074/082–086/088–095/097. Original 96 AT cases remain unchanged. Allowed modules: core table validation/owned layout DTO, service XLSX/TXT adapter, app mapping/picker, direct dependency edge, synthetic tests and stage docs/manifests. Gerber semantics/writer and main mixed checkout are preserved.

D1-I01: data-only bounded XLSX, explicit sheet/header/columns, mixed raw numbers/text/shared and inline strings, precision and physical row diagnostics.
D1-I02: explicit fixed-width UTF-8 spans, preserved blank Side and declared unit checks.
D1-I03: existing CSV/TSV/schema defaults and service transactions/provenance/Undo/Redo/Save/Open/Recovery regressions.
D1-I04: malformed ZIP/XML/formula/unsupported cell/budget failures are atomic and privacy safe.
D1-I05: GUI selects source layout, sheet/header/spans, raw table (20 sampled rows), per-column role/Ignore selectors, no-header input, optional header proposal and separate manual column/unit/convention confirmations; no silent manufacturing interpretation.
D1-I06: five private original hashes unchanged, independent row/field comparison via real service preview; ambiguous conventions reported as pending.

Evidence schema_version=2, independent of frozen 96 cases. Candidate parser checks first, then required workspace/release and service boundary gates. Public tests use synthetic worksheets; real file paths/content remain private. Windows, physical registration and D2 are deferred.
