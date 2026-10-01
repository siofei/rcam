//! Session-only shape identities. All currently supported edits are rigid or
//! structural. Any future size/aperture/node edit must allocate a fresh token
//! and restore before/after tokens with its history entry (never reuse by ID).
use super::*;
use editor_core::metrics::{GeometryMetrics, MAX_METRICS_WORK, MetricsError};
use std::collections::{HashSet, VecDeque};

pub const MAX_METRICS_OBJECTS: usize = 10_000;
const MAX_CACHE_ENTRIES: usize = 4096;
const MAX_IDENTITIES: usize = 20_000;
const MAX_IDENTITY_BYTES: usize = 2 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MetricValue {
    Exact { area_mm2: f64, perimeter_mm: f64 },
    Unsupported { reason: String },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsItem {
    pub object_id: String,
    #[serde(flatten)]
    pub value: MetricValue,
}
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub exact_count: usize,
    pub unsupported_count: usize,
    pub object_area_sum_mm2: f64,
    pub object_perimeter_sum_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsResult {
    pub document_id: String,
    pub revision: String,
    pub layer_id: String,
    pub items: Vec<MetricsItem>,
    pub summary: MetricsSummary,
}
#[derive(Debug, Default, Clone)]
pub(super) struct MetricsCache {
    active: HashMap<String, u64>,
    historical: HashMap<String, u64>,
    next: u64,
    values: HashMap<u64, Result<GeometryMetrics, MetricsError>>,
    fifo: VecDeque<u64>,
    calculations: usize,
    hits: usize,
}
impl MetricsCache {
    fn trim_identities(&mut self) {
        let bytes: usize = self
            .active
            .keys()
            .chain(self.historical.keys())
            .map(|id| id.len() + 64)
            .sum();
        if self.active.len() + self.historical.len() >= MAX_IDENTITIES
            || bytes >= MAX_IDENTITY_BYTES
        {
            // Eviction loses reuse only; manufacturing/history are untouched.
            self.active.clear();
            self.historical.clear();
            self.values.clear();
            self.fifo.clear();
        }
    }
    fn token(&mut self, id: &str) -> u64 {
        if let Some(t) = self.active.get(id) {
            return *t;
        }
        let token = self.historical.remove(id).unwrap_or_else(|| {
            self.next += 1;
            self.next
        });
        self.active.insert(id.into(), token);
        token
    }
    pub(super) fn duplicate(&mut self, sources: &[String], copies: &[String]) {
        self.trim_identities();
        for (source, copy) in sources.iter().zip(copies) {
            let t = self.token(source);
            self.active.insert(copy.clone(), t);
        }
        self.trim_identities();
    }
    pub(super) fn reconcile(&mut self, document: &SemanticDocument, ids: &[String]) {
        let tracked: HashSet<_> = ids
            .iter()
            .filter(|id| self.active.contains_key(*id) || self.historical.contains_key(*id))
            .map(String::as_str)
            .collect();
        if tracked.is_empty() {
            return;
        }
        let mut alive = HashSet::new();
        for object in document.layers.iter().flat_map(|l| &l.objects) {
            if tracked.contains(object.object_id.as_str()) {
                alive.insert(object.object_id.as_str());
                if alive.len() == tracked.len() {
                    break;
                }
            }
        }
        for id in ids {
            if alive.contains(id.as_str()) {
                if let Some(t) = self.historical.remove(id) {
                    self.active.insert(id.clone(), t);
                }
            } else if let Some(t) = self.active.remove(id) {
                self.historical.insert(id.clone(), t);
            }
        }
        self.trim_identities();
    }
    pub(super) fn invalidate_shapes(&mut self, ids: &[String]) {
        for id in ids {
            if let Some(token) = self
                .active
                .remove(id)
                .or_else(|| self.historical.remove(id))
            {
                self.values.remove(&token);
                self.fifo.retain(|candidate| *candidate != token);
            }
        }
        self.trim_identities();
    }
    fn insert(&mut self, token: u64, value: Result<GeometryMetrics, MetricsError>) {
        if self.values.len() >= MAX_CACHE_ENTRIES
            && let Some(old) = self.fifo.pop_front()
        {
            self.values.remove(&old);
        }
        self.fifo.push_back(token);
        self.values.insert(token, value);
    }
}
impl ApplicationService {
    pub fn objects_metrics(
        &mut self,
        id: &str,
        params: MetricsParams,
    ) -> Result<MetricsResult, ServiceError> {
        if params.object_ids.len() > MAX_METRICS_OBJECTS {
            return Err(ServiceError::resource(
                "metrics_objects",
                MAX_METRICS_OBJECTS,
                params.object_ids.len(),
            ));
        }
        let mut unique = HashSet::new();
        if params.object_ids.iter().any(|id| !unique.insert(id)) {
            return Err(ServiceError::invalid_field(
                "object_ids",
                "duplicate object ID",
            ));
        }
        let record = self
            .documents
            .get_mut(id)
            .ok_or_else(|| ServiceError::not_found("document", id))?;
        let layer = record
            .document
            .layers
            .iter()
            .find(|l| l.id == params.layer_id)
            .ok_or_else(|| ServiceError::not_found("layer", &params.layer_id))?;
        let index: HashMap<_, _> = layer
            .objects
            .iter()
            .map(|o| (o.object_id.as_str(), o))
            .collect();
        let objects = params
            .object_ids
            .iter()
            .map(|id| {
                index
                    .get(id.as_str())
                    .copied()
                    .ok_or_else(|| ServiceError::not_found("object", id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Stage all derived changes: resource failure returns no partial query/cache.
        let mut cache = record.metrics.clone();
        cache.trim_identities();
        let mut work = 0;
        let mut items = Vec::new();
        let mut summary = MetricsSummary::default();
        for object in objects {
            let token = cache.token(&object.object_id);
            let result = if let Some(value) = cache.values.get(&token) {
                cache.hits += 1;
                *value
            } else {
                cache.calculations += 1;
                let value =
                    editor_core::metrics::calculate(&record.document, &object.geometry, &mut work);
                if matches!(value, Err(MetricsError::ResourceLimit)) {
                    return Err(ServiceError::resource(
                        "metrics_work",
                        MAX_METRICS_WORK,
                        work,
                    ));
                }
                cache.insert(token, value);
                value
            };
            let value = match result {
                Ok(m) => {
                    summary.exact_count += 1;
                    summary.object_area_sum_mm2 += m.area_mm2;
                    summary.object_perimeter_sum_mm += m.perimeter_mm;
                    MetricValue::Exact {
                        area_mm2: m.area_mm2,
                        perimeter_mm: m.perimeter_mm,
                    }
                }
                Err(MetricsError::Unsupported(reason)) => {
                    summary.unsupported_count += 1;
                    MetricValue::Unsupported {
                        reason: reason.into(),
                    }
                }
                Err(MetricsError::ResourceLimit) => unreachable!(),
            };
            items.push(MetricsItem {
                object_id: object.object_id.clone(),
                value,
            });
        }
        if !summary.object_area_sum_mm2.is_finite() || !summary.object_perimeter_sum_mm.is_finite()
        {
            return Err(ServiceError::invalid_field(
                "object_ids",
                "metrics sum overflow",
            ));
        }
        cache.trim_identities();
        record.metrics = cache;
        Ok(MetricsResult {
            document_id: id.into(),
            revision: record.revision.to_string(),
            layer_id: params.layer_id,
            items,
            summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn move_rotate_mirror_duplicate_delete_undo_reuse_metrics_cache() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s2a3/gui_primitives.gbr");
        let mut s = ApplicationService::new();
        s.grant_file_access(&path, false).unwrap();
        let d = s.open(path.to_str().unwrap()).unwrap();
        let id = &d.document_id;
        let layer = &d.layer_ids[0];
        let object = s.documents[id].document.layers[0].objects[0]
            .object_id
            .clone();
        let params = MetricsParams {
            layer_id: layer.clone(),
            object_ids: vec![object.clone()],
        };
        let expected = s.objects_metrics(id, params.clone()).unwrap().items;
        for step in 0..3 {
            let rev = s.document_get(id).unwrap().revision;
            match step {
                0 => s
                    .objects_move(
                        id,
                        &rev,
                        MoveParams {
                            layer_id: layer.clone(),
                            object_ids: vec![object.clone()],
                            dx_mm: 5.,
                            dy_mm: -3.,
                        },
                    )
                    .unwrap(),
                1 => s
                    .objects_rotate(
                        id,
                        &rev,
                        RotateParams {
                            layer_id: layer.clone(),
                            object_ids: vec![object.clone()],
                            angle_deg: 37.,
                            pivot_mm: PivotMm { x_mm: 0., y_mm: 0. },
                        },
                    )
                    .unwrap(),
                _ => s
                    .objects_mirror(
                        id,
                        &rev,
                        MirrorParams {
                            layer_id: layer.clone(),
                            object_ids: vec![object.clone()],
                            axis: MirrorAxis::Horizontal { coordinate_mm: 0. },
                        },
                    )
                    .unwrap(),
            };
            assert_eq!(
                s.objects_metrics(id, params.clone()).unwrap().items,
                expected
            );
        }
        let copy = s
            .objects_duplicate(
                id,
                "3",
                MoveParams {
                    layer_id: layer.clone(),
                    object_ids: vec![object],
                    dx_mm: 0.,
                    dy_mm: 0.,
                },
            )
            .unwrap()
            .changed_object_ids;
        let copied = MetricsParams {
            layer_id: layer.clone(),
            object_ids: copy.clone(),
        };
        s.objects_metrics(id, copied.clone()).unwrap();
        s.objects_delete(
            id,
            "4",
            DeleteParams {
                layer_id: layer.clone(),
                object_ids: copy.clone(),
            },
        )
        .unwrap();
        assert!(!s.documents[id].metrics.active.contains_key(&copy[0]));
        s.history_undo(id, "5").unwrap();
        s.objects_metrics(id, copied).unwrap();
        assert_eq!(s.documents[id].metrics.calculations, 1);
        assert_eq!(s.documents[id].metrics.hits, 5);
        s.history_redo(id, "6").unwrap();
        assert!(!s.documents[id].metrics.active.contains_key(&copy[0]));
        s.close(id, "7", true).unwrap();
        assert!(!s.documents.contains_key(id));
    }
    #[test]
    fn bounded_cache_eviction_is_recomputable() {
        let mut c = MetricsCache::default();
        for i in 0..MAX_CACHE_ENTRIES + 1 {
            c.insert(
                i as u64,
                Ok(GeometryMetrics {
                    area_mm2: 1.,
                    perimeter_mm: 4.,
                }),
            );
        }
        assert_eq!(c.values.len(), MAX_CACHE_ENTRIES);
        assert!(!c.values.contains_key(&0));
        for i in 0..MAX_IDENTITIES {
            c.token(&i.to_string());
        }
        c.trim_identities();
        assert!(c.active.is_empty());
        assert!(c.values.is_empty());
    }
}
