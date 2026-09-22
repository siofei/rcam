//! `manifest.json` shape and verification (§6/§7 of the S4-B2 brief).

use crate::error::ProjectError;
use crate::model::{FORMAT, FORMAT_VERSION};
use editor_core::hash::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub sha256: String,
    pub uncompressed_size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub project_id: String,
    /// Sorted by `path` (deterministic encode, §38).
    pub entries: Vec<ManifestEntry>,
}

/// Build the manifest for a set of (path, content) pairs, already sorted by
/// path by the caller (`codec::encode_v1`).
pub fn build(project_id: &str, files: &[(String, Vec<u8>)]) -> Manifest {
    Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        project_id: project_id.into(),
        entries: files
            .iter()
            .map(|(path, data)| ManifestEntry {
                path: path.clone(),
                sha256: sha256_hex(data),
                uncompressed_size: data.len() as u64,
            })
            .collect(),
    }
}

/// Verify every manifest entry against the actual archive contents. Fails
/// closed on the first mismatch, missing entry, unknown `format`, or unknown
/// `format_version` (§7/§50) — never a partial/best-effort load.
pub fn verify(manifest: &Manifest, files: &HashMap<String, Vec<u8>>) -> Result<(), ProjectError> {
    if manifest.format != FORMAT {
        return Err(ProjectError::SchemaInvalid(format!(
            "manifest format {:?} is not \"rcam\"",
            manifest.format
        )));
    }
    if manifest.format_version != FORMAT_VERSION {
        return Err(ProjectError::UnknownFormatVersion(manifest.format_version));
    }
    for entry in &manifest.entries {
        let data = files.get(&entry.path).ok_or_else(|| {
            ProjectError::SchemaInvalid(format!("manifest references missing entry {}", entry.path))
        })?;
        if data.len() as u64 != entry.uncompressed_size || sha256_hex(data) != entry.sha256 {
            return Err(ProjectError::HashMismatch {
                path: entry.path.clone(),
            });
        }
    }
    // Every actual archive entry (other than the manifest itself) must be
    // named by the manifest too, so nothing smuggled in goes unverified.
    let manifest_paths: std::collections::HashSet<&str> = entry_paths(manifest).collect();
    for path in files.keys() {
        if path != "manifest.json" && !manifest_paths.contains(path.as_str()) {
            return Err(ProjectError::SchemaInvalid(format!(
                "archive entry {path} is not described by manifest.json"
            )));
        }
    }
    Ok(())
}

fn entry_paths(manifest: &Manifest) -> impl Iterator<Item = &str> {
    manifest.entries.iter().map(|entry| entry.path.as_str())
}
