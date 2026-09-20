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

## S1-A.1 reference material (not product dependencies)

No Cargo dependency, toolchain version, copied library code, or bundled font was added.
The Ucamco 2026.05 specification was read locally to implement arc semantics;
its SHA-256 is `ac9e9899f573d69e472b2112f1ae879c70f6cd13e17db41a1c885caf69961662`.
Source: [official specification](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf).

The project-authored `ucamco_full_circle_geometry.gbr` reconstructs the numeric circle
from `Polarities_and_Apertures.gbr` in the [official test archive](https://www.ucamco.com/files/downloads/file_en/423/gerber-layer-format-test-files_en.zip).
The original archive/PDF/complete upstream files are not redistributed in the source package.
Archive and upstream-file hashes are recorded in the public reference inventory. This is an
extracted geometric regression, not a claim that the complete upstream file passes: that file
also contains an unsupported thermal macro. The official arc-containing block file uses AB,
which remains outside the current supported subset.

The separately installed local gerbv is used as a reference process only; it is not linked,
copied, or distributed with the application. Its actual version and binary hash accompany
the reference results. Differences and unsupported reference behavior remain in the report.

## S2-A.3 native dialogs

App-only direct dependencies, already present transitively in Cargo.lock:

| Dependency | Locked version | Source | License | Use / alternative / platform |
|---|---|---|---|---|
| objc2 | 0.6.4 | https://github.com/madsmtm/objc2 | MIT | Main-thread marker and retained AppKit objects; macOS only. Avoid hand-written Objective-C FFI. |
| objc2-app-kit | 0.3.2 | https://github.com/madsmtm/objc2 | Zlib OR Apache-2.0 OR MIT | NSOpenPanel/NSSavePanel; macOS only, minimal features. Avoid an additional file-dialog framework. |
| objc2-foundation | 0.3.2 | https://github.com/madsmtm/objc2 | MIT | NSString/NSURL conversion; macOS only. |

API signatures and feature gates checked against the locked crates' generated official
bindings in the local Cargo source cache; license declarations inspected in each locked Cargo manifest.
No third-party source copied. The UI loads a local macOS system CJK font in memory;
no user/system font is copied into Git, source archives or app bundles.

## S4-A1 text outlines

`editor-text` directly uses existing locked `ttf-parser` 0.25.1,
MIT OR Apache-2.0, https://github.com/harfbuzz/ttf-parser. Purpose: static
TrueType/OpenType outlines on macOS and Windows. Alternative rustybuzz shaping
is deferred for the bounded single-line ASCII/CJK scope. No font is bundled.
Local acceptance uses installed Arial Unicode.ttf face 0 (SHA-256
876af2cd4854644e7f3e7feb2f688997fdb3343c6df6693611209c9dfb47ccec),
with local-use provenance and no redistribution. Font licensing is not inferred
from the filename or OS availability; callers supply their authorized source.

## S4-A2 material offset

clipper2-rust 1.1.0, https://crates.io/crates/clipper2-rust/1.1.0 .
Used under BSL-1.0 for closed polygon material offset; no font redistribution.

```text
Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the "Software") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

## S4-A2 system font catalog

macOS-only direct bindings: objc2-core-text 0.3.2 (Zlib OR Apache-2.0 OR MIT)
and objc2-core-foundation 0.3.2 (MIT), https://github.com/madsmtm/objc2 .
Core Text enumerates available installed font metadata; Core Foundation owns
and type-checks descriptor attributes. The app retains no redistributed font
bytes. This uses the existing objc2 ecosystem, with minimal catalog features,
in preference to handwritten FFI or approximate directory scans.

## Original RCam ASCII stroke font (S4-A2.2)

`crates/editor-text/src/stroke_font.txt` contains original centerline glyph data
created for RCam under the workspace MIT OR Apache-2.0 license. No CircuitCAM font,
Hershey dataset, operating-system font bytes, or third-party glyph table was copied.
CircuitCAM screenshots supplied by the user inform interaction behavior only.
