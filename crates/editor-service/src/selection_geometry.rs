//! Read-only selection material query. Cache is revision/precision/ID bound;
//! a revision is monotonically increasing through Undo/Redo, never a content ID.
use super::*;
use editor_core::BoundsMm;
pub use editor_core::hit_test::selection_geometry::{
    CompositeMaterial, QueryError as SelectionGeometryError,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionCentersParams {
    pub groups: Vec<SelectionGroup>,
    pub semantics: SelectionMaterialSemantics,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMaterialSemantics {
    SelectedLayerComposite,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SelectionMaterialResult {
    Computed { value: CompositeMaterial },
    Unavailable { error: SelectionGeometryError },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionCentersResult {
    pub document_id: String,
    pub computed_revision: String,
    pub resolution_mm: f64,
    pub bounds_mm: Option<BoundsMm>,
    pub bounding_center_mm: Option<MmPoint>,
    pub material: SelectionMaterialResult,
    pub selected_count: usize,
    pub cache_hit: bool,
}
#[derive(Default)]
pub(super) struct SelectionCentersCache {
    entries: VecDeque<(String, SelectionCentersResult)>,
    bytes: usize,
}
impl SelectionCentersCache {
    fn get(
        &mut self,
        key: &str,
        mut checkpoint: impl FnMut() -> Result<(), ServiceError>,
    ) -> Result<Option<SelectionCentersResult>, ServiceError> {
        let mut position = None;
        // Total keys <=64MiB, entries<=64: at most262144 bounded256-byte
        // comparisons, with checks even on length mismatch/early mismatch.
        for (index, (candidate, _)) in self.entries.iter().enumerate() {
            checkpoint()?;
            if candidate.len() != key.len() {
                continue;
            }
            let mut equal = true;
            for (a, b) in candidate
                .as_bytes()
                .chunks(256)
                .zip(key.as_bytes().chunks(256))
            {
                checkpoint()?;
                if a != b {
                    equal = false;
                    break;
                }
            }
            if equal {
                position = Some(index);
                break;
            }
        }
        let Some(position) = position else {
            return Ok(None);
        };
        // Cancellation leaves the LRU untouched; publish/move only after checks.
        checkpoint()?;
        let entry = self.entries.remove(position).expect("found cache position");
        let mut result = entry.1.clone();
        result.cache_hit = true;
        self.entries.push_back(entry);
        Ok(Some(result))
    }
    fn insert(&mut self, key: String, value: SelectionCentersResult) {
        let size = key.len() + 1024;
        while self.entries.len() >= 64 || self.bytes.saturating_add(size) > 64 * 1024 * 1024 {
            if let Some((k, _)) = self.entries.pop_front() {
                self.bytes -= k.len() + 1024;
            } else {
                return;
            }
        }
        self.bytes += size;
        self.entries.push_back((key, value));
    }
}
impl ApplicationService {
    pub fn geometry_selection_centers(
        &mut self,
        id: &str,
        revision: &str,
        params: SelectionCentersParams,
    ) -> Result<SelectionCentersResult, ServiceError> {
        self.geometry_selection_centers_cancellable(id, revision, params, || false)
    }
    pub fn geometry_selection_centers_cancellable(
        &mut self,
        id: &str,
        revision: &str,
        params: SelectionCentersParams,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SelectionCentersResult, ServiceError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        if cancelled() {
            return Err(selection_query_error(SelectionGeometryError::Cancelled));
        }
        let record = self
            .documents
            .get(id)
            .ok_or_else(|| ServiceError::not_found("document", id))?;
        if revision != record.revision.to_string() {
            return Err(ServiceError {
                code: "REVISION_CONFLICT".into(),
                message: "选择几何查询修订已过期".into(),
                details: serde_json::json!({"expected_revision":revision,"current_revision":record.revision.to_string()}),
            });
        }
        let count = params
            .groups
            .iter()
            .try_fold(0usize, |n, g| n.checked_add(g.object_ids.len()))
            .ok_or_else(|| selection_query_error(SelectionGeometryError::ResourceLimit))?;
        if count > editor_core::hit_test::selection_geometry::MAX_SELECTED
            || params.groups.len() > editor_core::hit_test::selection_geometry::MAX_SELECTED
        {
            return Err(selection_query_error(SelectionGeometryError::ResourceLimit));
        }
        let mut canonical = BTreeMap::new();
        let mut geometries = Vec::with_capacity(count);
        let mut expanded = 0usize;
        let mut admitted_edges = 0usize;
        for group in &params.groups {
            if cancelled() {
                return Err(selection_query_error(SelectionGeometryError::Cancelled));
            }
            if std::time::Instant::now() >= deadline {
                return Err(selection_query_error(SelectionGeometryError::ResourceLimit));
            }
            let ids: BTreeSet<_> = group.object_ids.iter().map(String::as_str).collect();
            if ids.len() != group.object_ids.len()
                || ids.is_empty()
                || canonical
                    .insert(group.layer_id.as_str(), ids.clone())
                    .is_some()
            {
                return Err(ServiceError::invalid("空目标或重复图层/对象"));
            }
            let layer = record
                .document
                .layers
                .iter()
                .find(|l| l.id == group.layer_id)
                .ok_or_else(|| ServiceError::not_found("layer", &group.layer_id))?;
            let mut found = 0;
            for object in &layer.objects {
                if cancelled() {
                    return Err(selection_query_error(SelectionGeometryError::Cancelled));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(selection_query_error(SelectionGeometryError::ResourceLimit));
                }
                if ids.contains(object.object_id.as_str()) {
                    let (n, edges) = editor_core::hit_test::selection_geometry::admission_cost(
                        &record.document,
                        &object.geometry,
                        &mut cancelled,
                    )
                    .map_err(selection_query_error)?;
                    expanded = expanded.saturating_add(n);
                    admitted_edges = admitted_edges.saturating_add(edges);
                    if expanded > editor_core::hit_test::selection_geometry::MAX_PRIMITIVES
                        || admitted_edges
                            .saturating_mul(1024)
                            .saturating_add(expanded.saturating_mul(512))
                            > editor_core::hit_test::selection_geometry::MAX_TEMP_BYTES
                    {
                        return Err(selection_query_error(SelectionGeometryError::ResourceLimit));
                    }
                    geometries.push(&object.geometry);
                    found += 1;
                }
            }
            if found != ids.len() {
                return Err(ServiceError::invalid("目标对象不属于指定图层"));
            }
        }
        let resolution = record.manufacturing_precision.resolution_mm;
        // Identity uses unambiguous JSON strings, including document lifecycle ID.
        let key = serde_json::to_string(&(
            id,
            revision,
            resolution.to_bits(),
            params.semantics,
            canonical,
        ))
        .map_err(serialize_error)?;
        if let Some(result) = self.selection_centers_cache.get(&key, || {
            if cancelled() {
                return Err(selection_query_error(SelectionGeometryError::Cancelled));
            }
            if std::time::Instant::now() >= deadline {
                return Err(selection_query_error(SelectionGeometryError::ResourceLimit));
            }
            Ok(())
        })? {
            if cancelled() {
                return Err(selection_query_error(SelectionGeometryError::Cancelled));
            }
            if std::time::Instant::now() >= deadline {
                return Err(selection_query_error(SelectionGeometryError::ResourceLimit));
            }
            return Ok(result);
        }
        let record = &self.documents[id];
        let mut bounds: Option<BoundsMm> = None;
        for chunk in geometries.chunks(32) {
            if cancelled() {
                return Err(selection_query_error(SelectionGeometryError::Cancelled));
            }
            if std::time::Instant::now() >= deadline {
                return Err(selection_query_error(SelectionGeometryError::ResourceLimit));
            }
            let next = editor_core::geometries_bounds_with_blocks(
                chunk.iter().copied(),
                &record.document.apertures,
                &record.document.block_definitions,
            )
            .map_err(|e| ServiceError::invalid(e.to_string()))?;
            if let Some(next) = next {
                bounds = Some(bounds.map_or(next, |b| b.union(next)));
            }
        }
        if cancelled() {
            return Err(selection_query_error(SelectionGeometryError::Cancelled));
        }
        let material = match editor_core::hit_test::selection_geometry::calculate_with_deadline(
            &record.document,
            &params.groups,
            resolution,
            deadline,
            &mut cancelled,
        ) {
            Ok(q) => SelectionMaterialResult::Computed { value: q.material },
            Err(SelectionGeometryError::Cancelled) => {
                return Err(selection_query_error(SelectionGeometryError::Cancelled));
            }
            Err(e @ SelectionGeometryError::InvalidArgument(_)) => {
                return Err(selection_query_error(e));
            }
            Err(error) => SelectionMaterialResult::Unavailable { error },
        };
        let result = SelectionCentersResult {
            document_id: id.into(),
            computed_revision: revision.into(),
            resolution_mm: resolution,
            bounds_mm: bounds,
            bounding_center_mm: bounds.map(BoundsMm::center),
            material,
            selected_count: count,
            cache_hit: false,
        };
        if cancelled() {
            return Err(selection_query_error(SelectionGeometryError::Cancelled));
        }
        // Only successful deterministic calculations enter the cache; transient
        // deadline/resource errors must remain retryable.
        if matches!(result.material, SelectionMaterialResult::Computed { .. }) {
            self.selection_centers_cache.insert(key, result.clone());
        }
        Ok(result)
    }
}
fn selection_query_error(error: SelectionGeometryError) -> ServiceError {
    let code = match error {
        SelectionGeometryError::Cancelled => "CANCELLED",
        SelectionGeometryError::ResourceLimit => "RESOURCE_LIMIT",
        SelectionGeometryError::PrecisionUncertain(_) => "PRECISION_UNCERTAIN",
        SelectionGeometryError::UnsupportedGeometry(_) => "UNSUPPORTED_FEATURE",
        SelectionGeometryError::NumericOverflow => "NUMERIC_OVERFLOW",
        SelectionGeometryError::InvalidArgument(_) => "INVALID_ARGUMENT",
    };
    ServiceError {
        code: code.into(),
        message: "选择制造几何查询未完成".into(),
        details: serde_json::json!({"cause":error}),
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn cancelled_long_key_comparison_preserves_cache_lru() {
        let value = SelectionCentersResult {
            document_id: "cache-test".into(),
            computed_revision: "0".into(),
            resolution_mm: 1e-4,
            bounds_mm: None,
            bounding_center_mm: None,
            material: SelectionMaterialResult::Computed {
                value: CompositeMaterial::ZeroArea,
            },
            selected_count: 0,
            cache_hit: false,
        };
        let mut cache = SelectionCentersCache::default();
        let key = "a".repeat(4096);
        cache.insert(key.clone(), value.clone());
        cache.insert("short".into(), value);
        let before = cache.entries.clone();
        let bytes = cache.bytes;
        let mut polls = 0;
        assert_eq!(
            cache
                .get(&key, || {
                    polls += 1;
                    if polls == 4 {
                        Err(selection_query_error(SelectionGeometryError::Cancelled))
                    } else {
                        Ok(())
                    }
                })
                .unwrap_err()
                .code,
            "CANCELLED"
        );
        assert_eq!(cache.entries, before);
        assert_eq!(cache.bytes, bytes);
        assert!(cache.get(&key, || Ok(())).unwrap().unwrap().cache_hit);
    }
}
