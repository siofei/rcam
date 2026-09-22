//! Migration dispatch boundary (§51 of the S4-B2 brief).
//!
//! S4-B2 freezes only `format_version = 1`; there is nothing to migrate
//! *from* yet. This module exists so a future `format_version = 2` reader
//! has one call site to extend (`decode_v2` bytes -> `migrate_v1_to_v2` ->
//! `RCamProject`) instead of an ever-growing pile of `Option<>` fields
//! bolted onto a single serde struct.

use crate::error::ProjectError;
use crate::model::RCamProject;

/// Dispatch on an unrecognized `format_version`. Every version other than 1
/// is unknown today, so this always fails closed (§50) rather than guessing
/// at a decode strategy.
pub fn migrate(format_version: u32, _bytes: &[u8]) -> Result<RCamProject, ProjectError> {
    Err(ProjectError::UnknownFormatVersion(format_version))
}
