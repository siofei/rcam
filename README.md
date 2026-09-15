> 当前推进到 **S2-A.2：精确对象几何命中**。公开 `objects.hit_test` 与制造边界查询。
> GUI 仍是 S0 演示；S2-A.3 GUI 尚未完成。契约见 [ADR 0014](docs/adr/0014-s2a2-hit-test.md)，实测见 [S2-A.2 报告](docs/S2_A2_REVIEW.md)。

# RCam Gerber editor — Mac-first S1-B2c

The headless application service now supports authorized Gerber open/query,
atomic multi-object Move/Duplicate/Delete/Rotate/Mirror, Undo/Redo, validation, safe export to a new path and
reopen. Geometry stays in the shared f64 manufacturing model; history stores
changed geometry or inserted/deleted objects plus bounded ID order guards. The window remains the S0 demonstration and now retains
its application service for its lifetime.

Current stage checks target macOS arm64. Windows is deferred, not passed.
This is not EditableV1, full CORE10 acceptance or a production release.
CORE-03/07/08 compatibility gaps and independent arc reference differences
remain recorded. See `docs/S1_B2B_REVIEW.md` and `docs/adr/0010-s1-b2b-transform-representation.md`.
Historical S1-A.1 and S1-B1 evidence remains under `evidence-public/`; new runs use their own run IDs.

The S0 window uses English labels so the default eframe font remains legible
on a clean installation; no user font is bundled. Chinese UI and vector text
support are deferred to the planned S4 scope.

Rotate preserves true manufacturing geometry; mirrored arcs reverse direction.
Rectangular sweeps support exact multiples of 90 degrees only (odd turns swap aperture dimensions);
mirror axes are world horizontal/vertical. Unsupported mixed selections reject atomically.
Workspace display name/visibility/lock now live in the service, independently of manufacturing dirty and history.
`layer.update` checks both manufacturing and workspace revisions. New edits respect locks; Undo/Redo remain available.
See ADR 0012 and `docs/S1_B2C_REVIEW.md` for scope and evidence. The next stage is S2-A Mac GUI, not implemented here.

## Workspace

```text
crates/editor-core/       f64/mm geometry and layer coverage
crates/gerber-io/         strict finite parser adapter
crates/editor-service/    UI-free S0/S2-A.2 service and JSON request envelope
crates/editor-app/        eframe + egui-wgpu S0 demonstration window
fixtures/synthetic/       immutable S0-C inputs and independent S1-A truth
docs/DEPENDENCIES.md      locked dependency and alternative record
THIRD_PARTY_NOTICES.md    direct dependency source and licence notices
evidence/                 local verification logs and review evidence
```

The toolchain is pinned by `rust-toolchain.toml`. Use an existing installation
of Rust 1.89.0 with rustfmt and clippy; do not switch RUSTUP_HOME to an empty
folder. The original machine has its existing toolchain and caches in
`.tools/`; those paths are local evidence, not prerequisites for other hosts.

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
cargo run --release --locked -p editor-app
```

The real parser/service contracts live in `automation_contract.rs` and
`headless_workflow.rs` under `crates/editor-service/tests/`. They exercise
open/query/validate/export/reopen without a window. This is an unchanged
document baseline; the S1-B1 Move workflow and S1-B2a Duplicate/Delete and S1-B2b Rotate/Mirror workflows are implemented; the full V1 headless automation workflow remains incomplete.

```text
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo test --locked -p editor-service --test s1a_semantic_truth
cargo test --locked -p editor-service --test s1a_independent
cargo test --locked -p editor-core --test s1a_independent_geometry
cargo tree --locked -p editor-service --edges normal
```

Read `AGENTS.md`, `docs/DESIGN_V1.md`, `docs/ACCEPTANCE_V1.md`, and
`docs/AUTOMATION_API.md` for the governing design and acceptance boundaries.

## S0-B regression and evidence

```
cargo test --locked -p gerber-io --test review_parser_regressions
cargo test --locked -p editor-core --test review_geometry_regressions
cargo test --locked -p editor-service --test review_service_regressions
cargo test --locked -p editor-service --test historical_audit -- --nocapture
cargo test --locked -p editor-service --test dependency_boundary -- --nocapture
cargo test --locked -p editor-app native_gpu_coverage_regressions -- --ignored --nocapture
python3 -m unittest discover -s scripts -p test_audit_core10.py
python3 scripts/source_manifest.py --check
```

The GPU check requires native Metal or DX12 hardware; it fails if no adapter
is available. It is explicitly ignored in ordinary workspace tests, not counted
as passed. Windows can use `python` for the dependency-free audit/hash scripts.
The original review attachment and its three regression files were absent;
new regressions are project-authored and do not claim to reproduce that attachment.
See `docs/S0_B_REVIEW.md` for actual runs, failures, platform gaps and file hashes.
Current source hashes are in `MANIFEST.sha256`; the old design-package manifest
is preserved under `docs/archive/`. Binaries have separate per-run hashes.

The S0 canvas release-frame drag fix and its regression/runtime evidence are
recorded in `docs/DRAG_FIX_REVIEW.md`.
