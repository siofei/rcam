//! Registered RefDes-assisted bounds candidates, not associations or openings.
use super::*;
use editor_core::{
    BoundsMm, ObjectOrigin, SemanticGeometry, candidate_window::CandidateWindow,
    world_index::WorldIndex,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

pub const MAX_NEARBY_CANDIDATES: usize = 10_000;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManufacturingSearchWindow {
    ComponentLocalRect { width_mm: f64, height_mm: f64 },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NearbyManufacturingQuery {
    pub revision: String,
    pub component_id: String,
    pub layer_ids: Vec<String>,
    pub window: ManufacturingSearchWindow,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManufacturingCandidate {
    pub layer_id: String,
    /// For text this is the lexically first member ObjectId, not a new identity.
    pub object_id: String,
    /// Whole logical text group, or one atomic ordinary object/BlockInstance.
    pub member_object_ids: Vec<String>,
    pub geometry_kind: String,
    pub exposure: editor_core::Exposure,
    pub world_bounds: BoundsMm,
    /// Distance to manufacturing envelope, not material or ownership proof.
    pub distance_to_component_mm: f64,
    pub center_distance_mm: f64,
    pub fully_inside_window: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManufacturingCandidatePage {
    pub document_id: String,
    pub revision: String,
    pub total: usize,
    pub window: CandidateWindow,
    pub items: Vec<ManufacturingCandidate>,
    pub nearby_object_count: usize,
}
#[derive(Debug, Clone)]
struct Group {
    layer: usize,
    members: Vec<usize>,
    bounds: BoundsMm,
}
#[derive(Debug, Clone)]
struct ManufacturingIndex {
    generation: u64,
    index: WorldIndex,
    groups: Vec<Group>,
    by_object: Vec<Vec<usize>>,
    by_id: Vec<HashMap<String, usize>>,
}
#[derive(Debug, Default, Clone)]
struct Cache {
    manufacturing: Option<ManufacturingIndex>,
    board: Option<Arc<editor_core::pnp::BoardState>>,
    components: HashMap<String, usize>,
}
#[derive(Debug, Default)]
pub(super) struct CandidateCache(Mutex<Cache>);
impl Clone for CandidateCache {
    fn clone(&self) -> Self {
        Self(Mutex::new(
            self.0.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        ))
    }
}
impl ManufacturingIndex {
    fn build(record: &S1DocumentRecord) -> Result<Self, ServiceError> {
        let d = &record.document;
        let index = WorldIndex::build(&d.layers, &d.apertures, &d.block_definitions)
            .map_err(ServiceError::invalid)?;
        let mut text = HashMap::new();
        let mut groups: Vec<Group> = vec![];
        let mut by_object: Vec<_> = d
            .layers
            .iter()
            .map(|l| vec![usize::MAX; l.objects.len()])
            .collect();
        for &(bounds, l, o) in index.entries() {
            let object = &d.layers[l].objects[o];
            let group = if let ObjectOrigin::GeneratedText { operation_id } = &object.origin {
                *text.entry((l, operation_id.clone())).or_insert_with(|| {
                    groups.push(Group {
                        layer: l,
                        members: vec![],
                        bounds,
                    });
                    groups.len() - 1
                })
            } else {
                groups.push(Group {
                    layer: l,
                    members: vec![],
                    bounds,
                });
                groups.len() - 1
            };
            groups[group].members.push(o);
            groups[group].bounds = groups[group].bounds.union(bounds);
            by_object[l][o] = group;
        }
        for g in &mut groups {
            g.members.sort_by(|a, b| {
                d.layers[g.layer].objects[*a]
                    .object_id
                    .cmp(&d.layers[g.layer].objects[*b].object_id)
            });
        }
        let by_id = d
            .layers
            .iter()
            .map(|l| {
                l.objects
                    .iter()
                    .enumerate()
                    .map(|(i, o)| (o.object_id.clone(), i))
                    .collect()
            })
            .collect();
        Ok(Self {
            generation: record.history.content_generation(),
            index,
            groups,
            by_object,
            by_id,
        })
    }
}
fn geometry_kind(o: &editor_core::SemanticObject) -> &'static str {
    if matches!(o.origin, ObjectOrigin::GeneratedText { .. }) {
        return "GeneratedText";
    }
    match o.geometry {
        SemanticGeometry::Flash { .. } => "Flash",
        SemanticGeometry::Line { .. } => "Line",
        SemanticGeometry::Arc { .. } => "Arc",
        SemanticGeometry::Region { .. } => "Region",
        SemanticGeometry::RectangularSweep { .. } => "RectangularSweep",
        SemanticGeometry::BlockInstance { .. } => "BlockInstance",
    }
}
impl ApplicationService {
    /// Rust GUI worker helper: resolve already returned bounded candidate members.
    /// Not a JSON operation and never changes GUI selection in the service.
    pub fn candidate_member_objects(
        &self,
        document_id: &str,
        revision: &str,
        candidates: &[ManufacturingCandidate],
    ) -> Result<Vec<Vec<ObjectInfo>>, ServiceError> {
        let r = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(r.revision, revision)?;
        if candidates
            .iter()
            .map(|c| c.member_object_ids.len())
            .sum::<usize>()
            > MAX_NEARBY_CANDIDATES
        {
            return Err(ServiceError::invalid("candidate member budget"));
        }
        let cache = r.candidates.0.lock().unwrap_or_else(|p| p.into_inner());
        let indexed = cache
            .manufacturing
            .as_ref()
            .filter(|c| c.generation == r.history.content_generation())
            .ok_or_else(|| ServiceError::invalid("candidate cache stale; query first"))?;
        candidates
            .iter()
            .map(|c| {
                let l = r
                    .document
                    .layers
                    .iter()
                    .position(|l| l.id == c.layer_id)
                    .ok_or_else(|| ServiceError::not_found("layer", &c.layer_id))?;
                c.member_object_ids
                    .iter()
                    .map(|id| {
                        let i = *indexed.by_id[l]
                            .get(id)
                            .ok_or_else(|| ServiceError::not_found("object", id))?;
                        Ok(ObjectInfo {
                            layer_id: c.layer_id.clone(),
                            object: r.document.layers[l].objects[i].clone(),
                        })
                    })
                    .collect()
            })
            .collect()
    }
    pub fn components_nearby_manufacturing(
        &self,
        document_id: &str,
        q: &NearbyManufacturingQuery,
    ) -> Result<ManufacturingCandidatePage, ServiceError> {
        let started = Instant::now();
        let op = rcam_diagnostics::Operation::begin_document(
            "components.nearby_manufacturing",
            document_id,
            self.documents.get(document_id).map(|r| r.revision),
        );
        let result = (|| {
            let record = self
                .documents
                .get(document_id)
                .ok_or_else(|| ServiceError::not_found("document", document_id))?;
            check_revision(record.revision, &q.revision)?;
            if q.limit == 0
                || q.limit > 500
                || q.layer_ids.is_empty()
                || q.layer_ids.len() > record.document.layers.len()
            {
                return Err(ServiceError::invalid(
                    "explicit nonempty unique layers and limit 1..=500 required",
                ));
            }
            let mut layers = Vec::new();
            for id in &q.layer_ids {
                let l = record
                    .document
                    .layers
                    .iter()
                    .position(|l| &l.id == id)
                    .ok_or_else(|| ServiceError::not_found("layer", id))?;
                if layers.contains(&l) {
                    return Err(ServiceError::invalid("duplicate layer"));
                }
                if record
                    .workspace
                    .get(id)
                    .is_none_or(|l| l.kind != editor_core::workspace::LayerKind::Gerber)
                {
                    return Err(ServiceError::invalid(
                        "only Gerber manufacturing layers are supported",
                    ));
                }
                layers.push(l);
            }
            let ManufacturingSearchWindow::ComponentLocalRect {
                width_mm,
                height_mm,
            } = q.window;
            CandidateWindow::new(MmPoint::new(0., 0.), 0., width_mm, height_mm)
                .map_err(ServiceError::invalid)?;
            let board = record
                .board
                .as_ref()
                .ok_or_else(|| ServiceError::not_found("component", &q.component_id))?;
            let mut cache = record
                .candidates
                .0
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            if cache.board.as_ref().is_none_or(|b| !Arc::ptr_eq(b, board)) {
                cache.components = board
                    .components
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.id.0.clone(), i))
                    .collect();
                cache.board = Some(board.clone());
            }
            let i = *cache
                .components
                .get(&q.component_id)
                .ok_or_else(|| ServiceError::not_found("component", &q.component_id))?;
            let info = component_info(&board.components[i], board);
            let (Some(center), Some(angle)) = (info.world_position, info.world_rotation_deg) else {
                return Err(ServiceError {
                    code: "REGISTRATION_REQUIRED".into(),
                    message: "请先完成 Board→World 配准".into(),
                    details: serde_json::json!({}),
                });
            };
            let window = CandidateWindow::new(center, angle, width_mm, height_mm)
                .map_err(ServiceError::invalid)?;
            let build = Instant::now();
            if cache
                .manufacturing
                .as_ref()
                .is_none_or(|c| c.generation != record.history.content_generation())
            {
                cache.manufacturing = Some(ManufacturingIndex::build(record)?);
            }
            let build_us = build.elapsed().as_micros() as u64;
            let indexed = cache.manufacturing.as_ref().unwrap();
            let index_started = Instant::now();
            let (nearby, envelope_tests) = indexed
                .index
                .nearby_with_stats(window.bounds(), &layers, MAX_NEARBY_CANDIDATES)
                .map_err(|n| {
                    ServiceError::resource("nearby_manufacturing", n, MAX_NEARBY_CANDIDATES)
                })?;
            let index_us = index_started.elapsed().as_micros() as u64;
            let classify_started = Instant::now();
            let mut seen = HashSet::new();
            let mut found = Vec::new();
            let mut members = 0;
            for &(b, l, o) in &nearby {
                if !window.classify(b).0 {
                    continue;
                }
                let g = indexed.by_object[l][o];
                if !seen.insert(g) {
                    continue;
                }
                let group = &indexed.groups[g];
                members += group.members.len();
                if members > MAX_NEARBY_CANDIDATES {
                    return Err(ServiceError::resource(
                        "candidate_members",
                        members,
                        MAX_NEARBY_CANDIDATES,
                    ));
                }
                let objects = &record.document.layers[l].objects;
                let first = &objects[group.members[0]];
                found.push((
                    l,
                    ManufacturingCandidate {
                        layer_id: record.document.layers[l].id.clone(),
                        object_id: first.object_id.clone(),
                        member_object_ids: group
                            .members
                            .iter()
                            .map(|i| objects[*i].object_id.clone())
                            .collect(),
                        geometry_kind: geometry_kind(first).into(),
                        exposure: first.exposure,
                        world_bounds: group.bounds,
                        distance_to_component_mm: window.distance_to_bounds(group.bounds),
                        center_distance_mm: center.distance_mm(group.bounds.center()),
                        fully_inside_window: window.classify(group.bounds).1,
                    },
                ));
            }
            let classify_us = classify_started.elapsed().as_micros() as u64;
            let sort = Instant::now();
            found.sort_by(|a, b| {
                b.1.fully_inside_window
                    .cmp(&a.1.fully_inside_window)
                    .then_with(|| a.1.center_distance_mm.total_cmp(&b.1.center_distance_mm))
                    .then(a.0.cmp(&b.0))
                    .then(a.1.object_id.cmp(&b.1.object_id))
            });
            rcam_diagnostics::measurements(
                rcam_diagnostics::Level::Info,
                "components.nearby.counts",
                &[
                    ("candidate_count", found.len() as u64),
                    ("registered", 1),
                    (
                        "registration_reflect_x",
                        u64::from(board.registration.as_ref().unwrap().transform.reflect_x),
                    ),
                    ("layer_count", layers.len() as u64),
                    ("nearby_object_count", nearby.len() as u64),
                    ("window_width_category", width_mm.log10().max(0.) as u64),
                    ("window_height_category", height_mm.log10().max(0.) as u64),
                    ("index_build_us", build_us),
                    ("world_index_us", index_us),
                    ("envelope_tests", envelope_tests as u64),
                    ("classify_us", classify_us),
                    ("sort_us", sort.elapsed().as_micros() as u64),
                    ("query_us", started.elapsed().as_micros() as u64),
                    ("budget_rejected", 0),
                ],
            );
            Ok(ManufacturingCandidatePage {
                document_id: document_id.into(),
                revision: q.revision.clone(),
                total: found.len(),
                window,
                nearby_object_count: nearby.len(),
                items: found
                    .into_iter()
                    .skip(q.offset)
                    .take(q.limit)
                    .map(|(_, c)| c)
                    .collect(),
            })
        })();
        if result
            .as_ref()
            .err()
            .is_some_and(|e: &ServiceError| e.code == "RESOURCE_LIMIT")
        {
            rcam_diagnostics::measurements(
                rcam_diagnostics::Level::Info,
                "components.nearby.counts",
                &[("budget_rejected", 1)],
            );
        }
        op.end(
            self.documents.get(document_id).map(|r| r.revision),
            result.as_ref().err().map(|e| e.code.as_str()),
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserved_drill_layer_is_rejected_before_component_lookup_without_mutation() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s4d2");
        let mut s = ApplicationService::with_file_access(FileAccessPolicy::new(
            root.clone(),
            [root.clone()],
            [root],
        ));
        let d = s.open("board.gbr").unwrap();
        let layer = d.layer_ids[0].clone();
        s.documents
            .get_mut(&d.document_id)
            .unwrap()
            .workspace
            .get_mut(&layer)
            .unwrap()
            .kind = editor_core::workspace::LayerKind::Drill;
        let before = s.document_get(&d.document_id).unwrap();
        let error = s
            .components_nearby_manufacturing(
                &d.document_id,
                &NearbyManufacturingQuery {
                    revision: before.revision.clone(),
                    component_id: "reserved".into(),
                    layer_ids: vec![layer],
                    window: ManufacturingSearchWindow::ComponentLocalRect {
                        width_mm: 10.,
                        height_mm: 10.,
                    },
                    offset: 0,
                    limit: 500,
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "INVALID_ARGUMENT");
        assert!(error.message.contains("only Gerber"));
        assert_eq!(s.document_get(&d.document_id).unwrap(), before);
    }
}
