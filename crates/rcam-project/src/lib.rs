//! `.rcam` Native Project Model / Schema v1 (S4-B2).
//!
//! Pure model + codec: no `egui`/`eframe`/`wgpu`/`winit`/file-dialog
//! dependency (enforced by `tests/dependency_boundary.rs`, mirroring
//! `editor-service`'s own boundary test). This crate does not replace
//! `ApplicationService`: it only encodes/decodes the project state an
//! `ApplicationService` document already holds, for `.rcam` file I/O that
//! ships in S4-B3. `system.capabilities` must keep `project.open`/
//! `project.save` unsupported until that phase.
//!
//! `.rcam` is a ZIP container (`manifest.json` + `project.json` +
//! `layers/*.json` + `blocks/*.json`); see `docs/adr/0031-rcam-native-project-format-v1.md`.

pub mod codec;
pub mod error;
pub mod manifest;
pub mod migrate;
pub mod model;
pub mod timings;
pub mod zip_codec;

pub use codec::{decode, encode_v1};
pub use error::ProjectError;
pub use model::*;
