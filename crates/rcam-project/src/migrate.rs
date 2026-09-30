//! Unknown-version boundary. The codec reads versions 1 and 2 directly and
//! migrates the v1 empty Board placeholder to None before validation (ADR 0042).
use crate::{error::ProjectError, model::RCamProject};
/// Unknown versions fail closed; no speculative migration or partial loading.
pub fn migrate(format_version: u32, _bytes: &[u8]) -> Result<RCamProject, ProjectError> {
    Err(ProjectError::UnknownFormatVersion(format_version))
}
