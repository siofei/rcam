# Third-party notices for S0

The versions below are the direct Cargo dependencies locked in
`Cargo.lock`. Their source, licence expression, purpose, and the considered
alternative are recorded here so the S0 boundary is reviewable. Cargo's
transitive dependency graph is also fixed by the lockfile; this file does not
restate every transitive package notice.

| Dependency | Locked version | Source | Licence | S0 use | Considered alternative |
| --- | ---: | --- | --- | --- | --- |
| `gerber_parser` | 0.5.0 | [crates.io](https://crates.io/crates/gerber_parser/0.5.0), [source](https://github.com/MakerPnP/gerber-parser) | MIT OR Apache-2.0 | Parse the input after the local strict pre-scan; the adapter then checks parser errors and interprets the accepted AST. | A handwritten parser would increase format and security risk; `libgerber`/gerbv is a future geometry option, not used by S0. |
| `gerber-types` | 0.7.0 | [crates.io](https://crates.io/crates/gerber-types/0.7.0), [source](https://github.com/MakerPnP/gerber-types) | MIT OR Apache-2.0 | Match the parser's typed command AST without exposing it through the service API. | Defining an independent command AST would duplicate parser semantics. |
| `eframe` | 0.33.3 | [crates.io](https://crates.io/crates/eframe/0.33.3), [source](https://github.com/emilk/egui) | MIT OR Apache-2.0 | Own the native window and the wgpu device/queue lifecycle. | A bespoke winit/wgpu host would duplicate eframe lifecycle handling. |
| `egui-wgpu` | 0.33.3 | [crates.io](https://crates.io/crates/egui-wgpu/0.33.3), [source](https://github.com/emilk/egui) | MIT OR Apache-2.0 | Register the official eframe callback resources and issue the small custom canvas draw. | `painter.circle_filled` cannot prove the required custom GPU path or local Clear behaviour. |
| `bytemuck` | 1.25.2 | [crates.io](https://crates.io/crates/bytemuck/1.25.2), [source](https://github.com/Lokathor/bytemuck) | MIT OR Apache-2.0 | Checked POD conversion for the bounded GPU uniform record. | Manual byte packing is more error-prone and obscures alignment checks. |
| `serde` / `serde_json` | 1.0.229 / 1.0.151 | [serde](https://crates.io/crates/serde), [serde_json](https://crates.io/crates/serde_json) | MIT OR Apache-2.0 | Encode the service's own versioned DTOs and strict request envelope. | Exposing parser AST or ad-hoc JSON would violate the service boundary. |

The application uses only the APIs documented by the locked versions. The
direct dependency declarations are in the workspace `Cargo.toml`; no user
Gerber, font, or private sample is downloaded or sent to a third party.
