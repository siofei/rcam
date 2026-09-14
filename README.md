# Gerber editor S0 technology validation

This workspace contains the first runnable S0 slice of the Gerber editor. It
is a read-only technology validation, not EditableV1 and not a production
Gerber editor or exporter.

The slice proves one deliberately small path: a real `gerber_parser` parser
reads a finite circle-flash subset, `gerber-io` rejects everything outside that
subset, `editor-core` evaluates f64 millimetre coverage with local aperture
holes and per-layer Dark/Clear order, and `editor-service` exposes the result
through a versioned JSON DTO boundary. `editor-app` displays the same service
snapshot through an eframe-managed wgpu callback. The demo also includes the
fixed line/ring and cross-layer Clear checks used by the S0 risk review.

The current implementation intentionally provides no object editing, undo,
save/export, or general-file compatibility. Those remain V1 requirements and
are reported as unsupported by the capability DTO.

The S0 window uses English labels so the default eframe font remains legible
on a clean installation; no user font is bundled. Chinese UI and vector text
support are deferred to the planned S4 scope.

## Workspace

```text
crates/editor-core/       f64/mm geometry and layer coverage
crates/gerber-io/         strict finite parser adapter
crates/editor-service/    UI-free S0 service and JSON request envelope
crates/editor-app/        eframe + egui-wgpu S0 demonstration window
fixtures/synthetic/       deterministic S0 parser fixture
docs/DEPENDENCIES.md      locked dependency and alternative record
THIRD_PARTY_NOTICES.md    direct dependency source and licence notices
evidence/                 local verification logs and review evidence
```

The local reproducible toolchain is pinned by `rust-toolchain.toml`. For the
development machine used for this S0 run, Rustup, Cargo's registry/cache, and
the build target were explicitly directed below `.tools/`; these environment
variables are not implicit Cargo defaults. On a machine with the pinned
toolchain available:

```bash
export RUSTUP_HOME="$PWD/.tools/rustup"
export CARGO_HOME="$PWD/.tools/cargo"
export CARGO_TARGET_DIR="$PWD/.tools/target"
export PATH="$PWD/.tools/bin:$PWD/.tools/cargo/bin:$PATH"
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
cargo run --release --locked -p editor-app
```

The S0 automation contract is the real parser/service contract test at
`crates/editor-service/tests/automation_contract.rs`. A V1 headless editing
workflow is not claimed because editing and export are not implemented.

Read `AGENTS.md`, `docs/DESIGN_V1.md`, `docs/ACCEPTANCE_V1.md`, and
`docs/AUTOMATION_API.md` for the governing design and acceptance boundaries.
