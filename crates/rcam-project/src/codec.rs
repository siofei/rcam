//! `.rcam` encode/decode pipeline (§7/§49/§50/§55 of the S4-B2 brief):
//! ZIP structure -> path validation -> budgets -> manifest -> per-entry
//! SHA-256 -> schema parse -> semantic validate. Every step is fail-closed;
//! there is no partial/best-effort load path.

use crate::error::ProjectError;
use crate::manifest::{self, Manifest};
use crate::migrate;
use crate::model::{BoardProjectState, LayerProjectState, RCamProject};
use crate::zip_codec::{self, ReadPolicy, ZipEntry};
use editor_core::block::BlockDefinition;
use std::collections::HashMap;

/// Resource budgets a `.rcam` reader enforces before trusting file content
/// (§49). Generous enough for the §57 performance fixture (400 openings x
/// 100 instances), far short of anything that could exhaust memory.
pub struct Budget {
    pub max_entries: usize,
    pub max_uncompressed_bytes: usize,
    pub max_entry_bytes: usize,
    pub max_path_len: usize,
    pub max_layers: usize,
    pub max_objects_per_layer: usize,
    pub max_block_definitions: usize,
    pub max_objects_per_block: usize,
    pub max_string_len: usize,
    /// Explicit JSON nesting-depth bound (S4-B2 Final Closeout B1), checked
    /// before any of `manifest.json`/`project.json`/a layer/a block
    /// definition is deserialized into its typed model. Comfortably below
    /// `serde_json`'s own ~128-frame implicit recursion limit, so a
    /// malicious/corrupt archive hits this typed, bounded `ResourceLimit`
    /// error rather than whatever `serde_json`'s undocumented internal
    /// limit happens to do — that implicit limit is never the security
    /// contract here, only an unrelied-upon backstop underneath it.
    pub max_json_depth: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_entries: 20_000,
            max_uncompressed_bytes: 512 * 1024 * 1024,
            max_entry_bytes: 128 * 1024 * 1024,
            max_path_len: 512,
            max_layers: 4_000,
            max_objects_per_layer: 1_000_000,
            max_block_definitions: 20_000,
            max_objects_per_block: 200_000,
            max_string_len: 1_000_000,
            max_json_depth: 64,
        }
    }
}

fn layer_path(layer_id: &str) -> Result<String, ProjectError> {
    safe_component(layer_id)?;
    Ok(format!("layers/{layer_id}.json"))
}

fn block_path(definition_id: &str) -> Result<String, ProjectError> {
    safe_component(definition_id)?;
    Ok(format!("blocks/{definition_id}.json"))
}

/// An id used as a filename component must itself be a safe single path
/// segment: `valid_id` (core) only forbids empty/control characters, not
/// `/`, so the encoder checks again here before it ever builds a path.
fn safe_component(id: &str) -> Result<(), ProjectError> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
    {
        return Err(ProjectError::PathTraversal(id.into()));
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ProjectRootFile {
    format_version: u32,
    project_id: String,
    manufacturing: crate::model::ManufacturingProjectSettings,
    workspace: crate::model::WorkspaceProjectState,
    layer_order: Vec<String>,
    block_definition_ids: Vec<String>,
    apertures: Vec<editor_core::ApertureDefinition>,
    board: Option<BoardProjectState>,
}

/// `serde_json` silently turns a non-finite `f64` into JSON `null` instead of
/// erroring (JSON has no NaN/Infinity token). Rather than pattern-matching
/// the resulting text (indistinguishable from a legitimate `Option::None`),
/// §48 is enforced *before* serialization: `RCamProject::validate` (called
/// at the top of `encode_v1`) walks every manufacturing number through
/// `SemanticDocument::validate`, plus the project-only float settings it
/// checks directly — so nothing non-finite ever reaches this function.
fn to_json(value: &impl serde::Serialize, what: &'static str) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec(value).map_err(|e| ProjectError::SchemaInvalid(format!("{what}: {e}")))
}

/// Deterministic encode (§38): stable field order (struct declaration order,
/// never a `HashMap`), stable entry order (sorted by path), a fixed ZIP
/// timestamp, and `serde_json::to_string`'s canonical compact form. Encoding
/// the same logical `RCamProject` twice yields byte-identical output.
pub fn encode_v1(project: &RCamProject) -> Result<Vec<u8>, ProjectError> {
    project.validate()?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    let root = ProjectRootFile {
        format_version: project.format_version,
        project_id: project.project_id.0.clone(),
        manufacturing: project.manufacturing,
        workspace: project.workspace.clone(),
        layer_order: project.layer_order.clone(),
        block_definition_ids: project
            .block_definitions
            .iter()
            .map(|d| d.id.0.clone())
            .collect(),
        apertures: project.apertures.clone(),
        board: project.board,
    };
    files.push(("project.json".into(), to_json(&root, "project.json")?));

    for layer in &project.layers {
        let path = layer_path(&layer.layer.id)?;
        files.push((path, to_json(layer, "layer")?));
    }
    for definition in &project.block_definitions {
        let path = block_path(&definition.id.0)?;
        files.push((path, to_json(definition, "block_definition")?));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let manifest = manifest::build(&project.project_id.0, &files);
    let manifest_bytes =
        serde_json::to_vec(&manifest).map_err(|e| ProjectError::SchemaInvalid(e.to_string()))?;

    let mut entries: Vec<ZipEntry> = Vec::with_capacity(files.len() + 1);
    entries.push(ZipEntry {
        path: "manifest.json",
        data: &manifest_bytes,
    });
    for (path, data) in &files {
        entries.push(ZipEntry { path, data });
    }
    Ok(zip_codec::write_zip(&entries))
}

fn read_policy(budget: &Budget) -> ReadPolicy {
    ReadPolicy {
        max_entries: budget.max_entries,
        max_uncompressed_bytes: budget.max_uncompressed_bytes,
        max_entry_bytes: budget.max_entry_bytes,
        max_path_len: budget.max_path_len,
    }
}

/// Walk a decoded JSON tree checking every string (object keys included,
/// since a map keyed by a project-controlled id is exactly the kind of
/// user-controlled string §5 asks to bound) against `max_string_len`, and
/// every nesting level against `max_json_depth`, before any of it becomes a
/// typed value. An unknown/ignored field is walked exactly like a known
/// one — nothing sidesteps this budget by not appearing in the schema.
fn check_json_budget(value: &serde_json::Value, budget: &Budget) -> Result<(), ProjectError> {
    fn check_string(s: &str, budget: &Budget) -> Result<(), ProjectError> {
        if s.len() > budget.max_string_len {
            return Err(ProjectError::ResourceLimit {
                resource: "string_len",
                limit: budget.max_string_len,
                actual: s.len(),
            });
        }
        Ok(())
    }
    fn walk(value: &serde_json::Value, budget: &Budget, depth: usize) -> Result<(), ProjectError> {
        if depth > budget.max_json_depth {
            return Err(ProjectError::ResourceLimit {
                resource: "json_depth",
                limit: budget.max_json_depth,
                actual: depth,
            });
        }
        match value {
            serde_json::Value::String(s) => check_string(s, budget)?,
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, budget, depth + 1)?;
                }
            }
            serde_json::Value::Object(fields) => {
                for (key, item) in fields {
                    check_string(key, budget)?;
                    walk(item, budget, depth + 1)?;
                }
            }
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            }
        }
        Ok(())
    }
    walk(value, budget, 0)
}

fn parse_json<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    budget: &Budget,
    what: &'static str,
) -> Result<T, ProjectError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| ProjectError::SchemaInvalid(format!("{what}: {e}")))?;
    check_json_budget(&value, budget)?;
    serde_json::from_value(value).map_err(|e| ProjectError::SchemaInvalid(format!("{what}: {e}")))
}

/// Decode with the default budget. See `decode_with_budget` for the full
/// pipeline.
pub fn decode(bytes: &[u8]) -> Result<RCamProject, ProjectError> {
    decode_with_budget(bytes, &Budget::default())
}

pub fn decode_with_budget(bytes: &[u8], budget: &Budget) -> Result<RCamProject, ProjectError> {
    let entries = zip_codec::read_zip(bytes, &read_policy(budget))?;
    let mut files: HashMap<String, Vec<u8>> = HashMap::with_capacity(entries.len());
    for entry in entries {
        files.insert(entry.path, entry.data);
    }
    let manifest_bytes = files
        .get("manifest.json")
        .ok_or(ProjectError::ManifestMissing)?;
    let manifest: Manifest = parse_json(manifest_bytes, budget, "manifest.json")?;
    // format_version is checked twice on purpose: once here (fail-closed
    // before trusting a single byte of project content, §50) and again
    // inside `RCamProject::validate` for any caller that builds a project by
    // hand instead of through this decoder.
    if manifest.format_version != crate::model::FORMAT_VERSION {
        return migrate::migrate(manifest.format_version, bytes);
    }
    manifest::verify(&manifest, &files)?;

    let project_bytes = files
        .get("project.json")
        .ok_or_else(|| ProjectError::SchemaInvalid("missing project.json".into()))?;
    let root: ProjectRootFile = parse_json(project_bytes, budget, "project.json")?;
    if root.format_version != crate::model::FORMAT_VERSION {
        return Err(ProjectError::UnknownFormatVersion(root.format_version));
    }
    if root.layer_order.len() > budget.max_layers {
        return Err(ProjectError::ResourceLimit {
            resource: "layers",
            limit: budget.max_layers,
            actual: root.layer_order.len(),
        });
    }
    if root.block_definition_ids.len() > budget.max_block_definitions {
        return Err(ProjectError::ResourceLimit {
            resource: "block_definitions",
            limit: budget.max_block_definitions,
            actual: root.block_definition_ids.len(),
        });
    }

    let mut layers = Vec::with_capacity(root.layer_order.len());
    for layer_id in &root.layer_order {
        let path = layer_path(layer_id)?;
        let bytes = files
            .get(&path)
            .ok_or_else(|| ProjectError::SchemaInvalid(format!("missing layer entry {path}")))?;
        let layer: LayerProjectState = parse_json(bytes, budget, "layer")?;
        if layer.layer.id != *layer_id {
            return Err(ProjectError::SchemaInvalid(format!(
                "layer entry {path} id does not match layer_order"
            )));
        }
        if layer.layer.objects.len() > budget.max_objects_per_layer {
            return Err(ProjectError::ResourceLimit {
                resource: "objects_per_layer",
                limit: budget.max_objects_per_layer,
                actual: layer.layer.objects.len(),
            });
        }
        layers.push(layer);
    }

    let mut block_definitions = Vec::with_capacity(root.block_definition_ids.len());
    for definition_id in &root.block_definition_ids {
        let path = block_path(definition_id)?;
        let bytes = files
            .get(&path)
            .ok_or_else(|| ProjectError::SchemaInvalid(format!("missing block entry {path}")))?;
        let definition: BlockDefinition = parse_json(bytes, budget, "block_definition")?;
        if definition.id.0 != *definition_id {
            return Err(ProjectError::SchemaInvalid(format!(
                "block entry {path} id does not match project.json"
            )));
        }
        if definition.objects.len() > budget.max_objects_per_block {
            return Err(ProjectError::ResourceLimit {
                resource: "objects_per_block",
                limit: budget.max_objects_per_block,
                actual: definition.objects.len(),
            });
        }
        block_definitions.push(definition);
    }

    // No Drill layer can be present, per §43, checked once more inside
    // `validate` below; Solo/Selection/Undo/AppPreferences are simply not
    // fields of `LayerProjectState`/`WorkspaceProjectState`, so there is
    // nothing to strip here (§13/§14/§40/§44 are enforced by the schema's
    // shape, not by a runtime filter).
    let mut workspace = root.workspace;
    if let Some(camera) = workspace.camera
        && !camera.is_valid()
    {
        workspace.camera = None;
    }

    let project = RCamProject {
        format_version: root.format_version,
        project_id: crate::model::ProjectId(root.project_id),
        manufacturing: root.manufacturing,
        workspace,
        layer_order: root.layer_order,
        layers,
        apertures: root.apertures,
        block_definitions,
        board: root.board,
    };
    project.validate()?;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn budget(max_string_len: usize, max_json_depth: usize) -> Budget {
        Budget {
            max_string_len,
            max_json_depth,
            ..Budget::default()
        }
    }

    #[test]
    fn string_exactly_at_limit_is_accepted_one_over_is_rejected() {
        let b = budget(8, 64);
        assert!(check_json_budget(&json!({"name": "12345678"}), &b).is_ok());
        let err = check_json_budget(&json!({"name": "123456789"}), &b).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::ResourceLimit {
                resource: "string_len",
                limit: 8,
                actual: 9,
            }
        ));
    }

    #[test]
    fn an_oversized_object_key_is_rejected_same_as_an_oversized_value() {
        let b = budget(4, 64);
        let err = check_json_budget(&json!({"12345": 1}), &b).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::ResourceLimit {
                resource: "string_len",
                limit: 4,
                actual: 5,
            }
        ));
    }

    /// Build `depth` levels of nested single-element arrays around `leaf`
    /// (so the outermost array is depth 1, matching `walk`'s convention that
    /// the top-level value passed to `check_json_budget` is depth 0).
    fn nested(depth: usize, leaf: serde_json::Value) -> serde_json::Value {
        (0..depth).fold(leaf, |value, _| json!([value]))
    }

    #[test]
    fn json_nested_exactly_at_limit_is_accepted_one_over_is_rejected() {
        let b = budget(1_000_000, 8);
        assert!(check_json_budget(&nested(8, json!(1)), &b).is_ok());
        let err = check_json_budget(&nested(9, json!(1)), &b).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::ResourceLimit {
                resource: "json_depth",
                limit: 8,
                actual: 9,
            }
        ));
    }

    #[test]
    fn a_huge_ignored_optional_object_is_still_bounded_by_both_budgets() {
        // "future_field" (12 chars) itself must fit; only its *content* is
        // deliberately probed against each budget below.
        let strings = budget(20, 64);
        // An unknown field's *shallow* string value is still walked (and
        // still passes when it fits) — nothing about being unrecognized by
        // the schema exempts it from the budget.
        assert!(check_json_budget(&json!({"future_field": "ok"}), &strings).is_ok());
        // Its content is not sight-unseen either: an oversized string inside
        // an unknown field is caught exactly like a known one...
        let err = check_json_budget(
            &json!({"future_field": "this string is definitely over twenty characters"}),
            &strings,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ProjectError::ResourceLimit {
                resource: "string_len",
                ..
            }
        ));
        // ...and so is excess nesting inside it.
        let depth = budget(1_000_000, 4);
        let err =
            check_json_budget(&json!({"future_field": nested(6, json!(1))}), &depth).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::ResourceLimit {
                resource: "json_depth",
                ..
            }
        ));
    }

    #[test]
    fn scalars_and_null_never_count_against_either_budget() {
        let b = budget(0, 0);
        assert!(check_json_budget(&json!(null), &b).is_ok());
        assert!(check_json_budget(&json!(true), &b).is_ok());
        assert!(check_json_budget(&json!(12345), &b).is_ok());
        assert!(check_json_budget(&json!(""), &b).is_ok());
    }
}
