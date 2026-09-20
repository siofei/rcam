# S0 dependency record

## Toolchain

S0 is pinned to Rust `1.89.0` in `rust-toolchain.toml`, with `rustfmt` and
`clippy`. The toolchain was installed under `.tools/rustup` and Cargo's
registry/cache and build target were kept under `.tools/` on the development
machine. No global Rust installation is required by this workspace.

## Locked choices

`gerber_parser` `0.5.0` and `gerber-types` `0.7.0` were selected after checking
their published package metadata and source API. S0 first performs an
independent finite-language pre-scan, then calls the real parser, checks the
complete parser error collection, and finally interprets the small typed command
set. The adapter currently uses the parser's `commands()` iterator only after
the error collection has been checked; it does not treat that iterator's
error-dropping behaviour as validation.

`eframe`, `egui`, and `egui-wgpu` are locked together at `0.33.3`. The custom
canvas uses eframe's `wgpu_render_state` and the official egui-wgpu callback
resource hooks, so the window owns the device and queue. The fragment shader
receives the bounded service snapshot, applies source-order exposure per layer,
and then combines the independent layer results. The GPU path is a display
validation only; f64/mm geometry remains in `editor-core`.

`serde`/`serde_json` provide the service-owned DTO boundary and strict JSON
request envelope. `bytemuck` provides checked POD conversion for the bounded
uniform block. Direct package sources, licences, uses, and alternatives are in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).

## Platform policy

The CI workflow covers Windows MSVC and macOS arm64 only. Linux and WSL2 are
outside the product target and are intentionally absent from CI. The local
headless tests do not claim native window, Metal, or Windows DX12 acceptance;
those require the corresponding platform runs.

## S4-A1 direct font parser

`ttf-parser = 0.25.1` is pinned directly in editor-text and Cargo.lock.
Source: https://crates.io/crates/ttf-parser/0.25.1 and
https://github.com/harfbuzz/ttf-parser. License: MIT OR Apache-2.0.
Purpose: static TTF/OTF glyph outline extraction, including TTC face selection.
A maintained bounded parser avoids implementing binary font parsing ourselves.
No shaping engine is introduced for the frozen single-line ASCII/CJK subset;
complex shaping and variable fonts remain unsupported. Font files are local
user inputs and are not distributed. See THIRD_PARTY_NOTICES.md.
