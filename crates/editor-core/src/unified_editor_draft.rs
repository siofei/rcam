//! Opaque, bounded manufacturing checkpoints. No main-document trial commits.
use super::*;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_DRAFT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftStep {
    pub groups: Vec<SelectionGroup>,
    pub operation: SelectionEdit,
}

/// Host-only absolute rectangle dimensions, using the existing grip size semantics.
/// No inferred edge, area, axis or hole-deformation policy.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftApertureSizeStep {
    pub groups: Vec<SelectionGroup>,
    pub width_mm: f64,
    pub height_mm: f64,
}
#[derive(Debug, Clone)]
enum Input {
    Transform(DraftStep),
    ApertureSize(DraftApertureSizeStep),
}
impl Input {
    fn groups(&self) -> &Vec<SelectionGroup> {
        match self {
            Self::Transform(s) => &s.groups,
            Self::ApertureSize(s) => &s.groups,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
struct DraftAperture {
    base_id: String,
    definition: ApertureDefinition,
}
#[derive(Debug, Clone, PartialEq)]
struct Checkpoint {
    geometry: Vec<SemanticGeometry>,
    apertures: Vec<DraftAperture>,
}
impl std::ops::Deref for Checkpoint {
    type Target = Vec<SemanticGeometry>;
    fn deref(&self) -> &Self::Target {
        &self.geometry
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DraftResources {
    pub resident_bytes: usize,
    pub reserved_peak_bytes: usize,
    pub byte_limit: usize,
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub entry_limit: usize,
}

#[derive(Debug)]
struct Target {
    layer: usize,
    layer_id: String,
    index: usize,
    object_id: String,
}

/// Borrowed prospective manufacturing geometry in authoritative source-slot order.
/// The host may prepare artifacts, but cannot change or replace candidate geometry.
#[derive(Clone, Copy)]
pub struct DraftCandidate<'a> {
    targets: &'a [Target],
    geometry: &'a [SemanticGeometry],
    apertures: &'a [DraftAperture],
}
impl<'a> DraftCandidate<'a> {
    /// Only definitions owned by this exact prospective checkpoint.
    pub fn apertures(self) -> impl ExactSizeIterator<Item = &'a ApertureDefinition> {
        self.apertures.iter().map(|a| &a.definition)
    }
    pub fn objects(
        self,
    ) -> impl ExactSizeIterator<Item = (&'a str, &'a str, &'a SemanticGeometry)> {
        self.targets
            .iter()
            .zip(self.geometry)
            .map(|(target, geometry)| {
                (
                    target.layer_id.as_str(),
                    target.object_id.as_str(),
                    geometry,
                )
            })
    }
}

/// Only typed transforms can change this object. Geometry views are borrowed/read-only.
#[derive(Debug)]
pub struct ManufacturingDraft {
    document_id: String,
    namespace: u64,
    content_generation: u64,
    generation: u64,
    targets: Vec<Target>,
    groups: Vec<SelectionGroup>,
    entry: Arc<Checkpoint>,
    work: Arc<Checkpoint>,
    preview: Option<Arc<Checkpoint>>,
    undo: Vec<Arc<Checkpoint>>,
    redo: Vec<Arc<Checkpoint>>,
    latest: Option<Input>,
    byte_limit: usize,
    entry_limit: usize,
    scratch_bytes: usize,
}

fn checked_add(a: usize, b: usize) -> Result<usize, EditError> {
    a.checked_add(b).ok_or(EditError::ResourceLimit)
}
fn checked_mul(a: usize, b: usize) -> Result<usize, EditError> {
    a.checked_mul(b).ok_or(EditError::ResourceLimit)
}
fn groups_bytes(groups: &Vec<SelectionGroup>) -> Result<usize, EditError> {
    let mut n = checked_mul(groups.capacity(), size_of::<SelectionGroup>() + 64)?;
    for g in groups {
        n = checked_add(n, g.layer_id.capacity() + 64)?;
        n = checked_add(
            n,
            checked_mul(g.object_ids.capacity(), size_of::<String>() + 64)?,
        )?;
        for id in &g.object_ids {
            n = checked_add(n, id.capacity())?;
        }
    }
    Ok(n)
}
fn geometry_capacity(g: &SemanticGeometry) -> Result<usize, EditError> {
    match g {
        SemanticGeometry::Flash { aperture_id, .. } => Ok(aperture_id.capacity() + 64),
        SemanticGeometry::BlockInstance { definition_id, .. } => {
            Ok(definition_id.0.capacity() + 64)
        }
        SemanticGeometry::Region { contours } => {
            let mut n = checked_mul(contours.capacity(), size_of::<RegionContour>() + 64)?;
            for c in contours {
                n = checked_add(
                    n,
                    checked_mul(c.edges.capacity(), size_of::<RegionEdge>())? + 64,
                )?;
            }
            Ok(n)
        }
        _ => Ok(0),
    }
}
fn geometry_bytes(snapshot: &Vec<SemanticGeometry>) -> Result<usize, EditError> {
    let mut n = checked_mul(snapshot.capacity(), size_of::<SemanticGeometry>())?;
    for g in snapshot {
        n = checked_add(n, geometry_capacity(g)?)?;
    }
    checked_add(n, size_of::<Vec<SemanticGeometry>>() + 256)
}

fn aperture_bytes(a: &DraftAperture) -> Result<usize, EditError> {
    // Overlay definitions are rectangle-only; reject expansion instead of undercharging macros.
    if !matches!(a.definition.shape, ApertureShape::Rectangle { .. }) {
        return Err(EditError::UnsupportedTransform);
    }
    checked_add(
        size_of::<DraftAperture>() + 128,
        checked_add(a.base_id.capacity(), a.definition.id.capacity())?,
    )
}
fn snapshot_bytes(s: &Checkpoint) -> Result<usize, EditError> {
    let mut n = checked_add(
        geometry_bytes(&s.geometry)?,
        checked_mul(s.apertures.capacity(), size_of::<DraftAperture>() + 64)?,
    )?;
    for a in &s.apertures {
        n = checked_add(n, aperture_bytes(a)?)?;
    }
    Ok(n)
}

impl EditHistory {
    pub fn begin_manufacturing_draft(
        &self,
        document: &SemanticDocument,
        groups: Vec<SelectionGroup>,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<ManufacturingDraft, EditError> {
        checkpoint()?;
        let targets = self.selection_targets(document, &groups, MAX_MOVE_OBJECTS)?;
        // Also bounds definition resolution and validation maps; not a session peak claim.
        let demand = self.move_selection_demand_checked(document, &groups, checkpoint)?;
        demand.admit()?;
        let count = targets.iter().map(|(_, ii)| ii.len()).sum::<usize>();
        let mut metadata = checked_add(groups_bytes(&groups)?, document.id.capacity() + 256)?;
        let mut geometry = checked_mul(count, size_of::<SemanticGeometry>())? + 256;
        for (l, indices) in &targets {
            for &i in indices {
                let layer = &document.layers[*l];
                let object = &layer.objects[i];
                metadata = checked_add(
                    metadata,
                    size_of::<Target>() + layer.id.len() + object.object_id.len() + 128,
                )?;
                geometry = checked_add(geometry, geometry_capacity(&object.geometry)?)?;
            }
        }
        // Entry + potential work candidate + conservative transaction/resolve scratch.
        let peak = checked_add(
            checked_add(metadata, checked_mul(geometry, 2)?)?,
            demand.work_bytes,
        )?;
        if peak > self.max_bytes {
            return Err(EditError::ResourceLimit);
        }
        let mut slots = Vec::new();
        let mut entry = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| EditError::ResourceLimit)?;
        entry
            .try_reserve_exact(count)
            .map_err(|_| EditError::ResourceLimit)?;
        for (l, indices) in targets {
            for i in indices {
                checkpoint()?;
                let layer = &document.layers[l];
                let object = &layer.objects[i];
                slots.push(Target {
                    layer: l,
                    layer_id: layer.id.clone(),
                    index: i,
                    object_id: object.object_id.clone(),
                });
                entry.push(object.geometry.clone());
            }
        }
        let entry = Arc::new(Checkpoint {
            geometry: entry,
            apertures: vec![],
        });
        let namespace = NEXT_DRAFT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| EditError::ResourceLimit)?;
        let result = ManufacturingDraft {
            document_id: document.id.clone(),
            namespace,
            content_generation: self.content_generation,
            generation: 0,
            targets: slots,
            groups,
            work: Arc::clone(&entry),
            entry,
            preview: None,
            undo: vec![],
            redo: vec![],
            latest: None,
            byte_limit: self.max_bytes,
            entry_limit: self.max_entries,
            scratch_bytes: demand.work_bytes,
        };
        result.admit(snapshot_bytes(&result.work)?)?;
        checkpoint()?;
        Ok(result)
    }

    /// Every validation and cancellation barrier precedes the one cross-layer commit.
    pub fn apply_manufacturing_draft(
        &mut self,
        document: &mut SemanticDocument,
        draft: &ManufacturingDraft,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
        begin_commit: &mut impl FnMut(bool) -> Result<(), EditError>,
    ) -> Result<Vec<String>, EditError> {
        self.apply_manufacturing_draft_prepared(
            document,
            draft,
            checkpoint,
            &mut |_| Ok(()),
            begin_commit,
        )
    }
    pub fn apply_manufacturing_draft_prepared(
        &mut self,
        document: &mut SemanticDocument,
        draft: &ManufacturingDraft,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), EditError>,
        begin_commit: &mut impl FnMut(bool) -> Result<(), EditError>,
    ) -> Result<Vec<String>, EditError> {
        draft.check_fence(self, document)?;
        let candidate = draft.calculate(document, checkpoint)?;
        let final_work = candidate.as_ref().unwrap_or(&draft.work);
        let mut next_generated = self.next_generated_aperture_id;
        let final_work = if final_work.apertures.is_empty() {
            std::borrow::Cow::Borrowed(&**final_work)
        } else {
            let key = checked_add(document.id.len(), 128)?;
            let mapping_bytes = checked_mul(
                final_work.apertures.len(),
                checked_add(checked_mul(key, 6)?, 512)?,
            )?;
            let rewritten_geometry = checked_mul(draft.targets.len(), checked_mul(key, 4)?)?;
            draft.admit(checked_add(
                checked_mul(snapshot_bytes(final_work)?, 3)?,
                checked_add(mapping_bytes, rewritten_geometry)?,
            )?)?;
            let mut final_work = (**final_work).clone();
            let mut dcode = document
                .apertures
                .iter()
                .map(|a| a.source_dcode)
                .max()
                .unwrap_or(9);
            let mut ids = HashSet::new();
            let mut mapping = HashMap::new();
            ids.try_reserve(final_work.apertures.len())
                .map_err(|_| EditError::ResourceLimit)?;
            mapping
                .try_reserve(final_work.apertures.len())
                .map_err(|_| EditError::ResourceLimit)?;
            for a in &mut final_work.apertures {
                checkpoint()?;
                let id = format!("{}-generated-aperture-{next_generated}", document.id);
                next_generated = next_generated
                    .checked_add(1)
                    .ok_or(EditError::ResourceLimit)?;
                dcode = dcode.checked_add(1).ok_or(EditError::ResourceLimit)?;
                if document.apertures.iter().any(|a| a.id == id) || !ids.insert(id.clone()) {
                    return Err(EditError::InvalidArgument);
                }
                mapping.insert(a.definition.id.clone(), id.clone());
                a.definition.id = id;
                a.definition.source_dcode = dcode;
            }
            for g in &mut final_work.geometry {
                checkpoint()?;
                if let SemanticGeometry::Flash { aperture_id, .. } = g
                    && let Some(id) = mapping.get(aperture_id)
                {
                    *aperture_id = id.clone();
                }
            }
            std::borrow::Cow::Owned(final_work)
        };
        draft.admit(candidate.as_ref().map_or(Ok(0), |g| snapshot_bytes(g))?)?;
        let mut parts: BTreeMap<usize, Transaction> = BTreeMap::new();
        for (n, target) in draft.targets.iter().enumerate() {
            checkpoint()?;
            let layer = document
                .layers
                .get(target.layer)
                .filter(|l| l.id == target.layer_id)
                .ok_or(EditError::InvalidArgument)?;
            let object = layer
                .objects
                .get(target.index)
                .filter(|o| o.object_id == target.object_id)
                .ok_or(EditError::InvalidArgument)?;
            if object.geometry != draft.entry[n] {
                return Err(EditError::InvalidArgument);
            }
            if draft.entry[n] == final_work[n] {
                continue;
            }
            let tx = parts.entry(target.layer).or_insert_with(|| Transaction {
                layer_id: target.layer_id.clone(),
                layer: target.layer,
                operation: Operation::Modify(vec![]),
                before_order: vec![],
                after_order: vec![],
                bytes: 0,
            });
            let Operation::Modify(changes) = &mut tx.operation else {
                unreachable!()
            };
            changes.push(Change {
                object_id: target.object_id.clone(),
                index: target.index,
                before: draft.entry[n].clone(),
                after: final_work[n].clone(),
            });
        }
        checkpoint()?;
        if parts.is_empty() {
            prepare(draft.candidate(&final_work))?;
            checkpoint()?;
            // NoChange is a successful terminal with no main content/history publication.
            begin_commit(false)?;
            return Ok(vec![]);
        }
        // Existing demand includes all delta/commit copies; its full conservative charge
        // is used as this transaction's history charge, independently of session resident.
        let demand = self.move_selection_demand_checked(document, &draft.groups, checkpoint)?;
        demand.admit()?;
        let bytes = if final_work.apertures.is_empty() {
            demand.history_bytes
        } else {
            checked_add(
                demand.history_bytes,
                checked_mul(snapshot_bytes(&final_work)?, 2)?,
            )?
        };
        self.budget(bytes)?;
        let tx = Transaction {
            layer_id: String::new(),
            layer: 0,
            operation: if final_work.apertures.is_empty() {
                Operation::Selection(parts.into_values().collect())
            } else {
                Operation::DraftApertures {
                    parts: parts.into_values().collect(),
                    inserted: final_work
                        .apertures
                        .iter()
                        .enumerate()
                        .map(|(n, a)| (document.apertures.len() + n, a.definition.clone()))
                        .collect(),
                }
            },
            before_order: vec![],
            after_order: vec![],
            bytes,
        };
        check_transaction(document, &tx, true)?;
        if !final_work.apertures.is_empty() {
            document
                .apertures
                .try_reserve_exact(final_work.apertures.len())
                .map_err(|_| EditError::ResourceLimit)?;
            self.undo
                .try_reserve_exact(1)
                .map_err(|_| EditError::ResourceLimit)?;
        }
        prepare(draft.candidate(&final_work))?;
        checkpoint()?;
        begin_commit(true)?;
        let ids = self.commit(document, tx);
        self.next_generated_aperture_id = next_generated;
        Ok(ids)
    }
}

impl ManufacturingDraft {
    fn candidate<'a>(&'a self, geometry: &'a Checkpoint) -> DraftCandidate<'a> {
        DraftCandidate {
            targets: &self.targets,
            geometry: &geometry.geometry,
            apertures: &geometry.apertures,
        }
    }
    pub fn work_candidate(&self) -> DraftCandidate<'_> {
        self.candidate(&self.work)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn groups(&self) -> &[SelectionGroup] {
        &self.groups
    }
    pub fn entry_geometry(&self) -> &[SemanticGeometry] {
        &self.entry
    }
    pub fn work_geometry(&self) -> &[SemanticGeometry] {
        &self.work
    }
    pub fn preview_geometry(&self) -> Option<&[SemanticGeometry]> {
        self.preview.as_deref().map(|s| s.geometry.as_slice())
    }
    pub fn latest_groups(&self) -> Option<&[SelectionGroup]> {
        self.latest.as_ref().map(|s| s.groups().as_slice())
    }
    pub fn has_work_changes(&self) -> bool {
        self.entry != self.work
    }
    fn check_fence(
        &self,
        history: &EditHistory,
        document: &SemanticDocument,
    ) -> Result<(), EditError> {
        if self.document_id != document.id || self.content_generation != history.content_generation
        {
            Err(EditError::InvalidArgument)
        } else {
            Ok(())
        }
    }
    fn next_generation(&self) -> Result<u64, EditError> {
        self.generation
            .checked_add(1)
            .ok_or(EditError::ResourceLimit)
    }
    fn resident_bytes(&self) -> Result<usize, EditError> {
        let mut n = size_of::<Self>() + self.document_id.capacity() + groups_bytes(&self.groups)?;
        n = checked_add(
            n,
            checked_mul(self.targets.capacity(), size_of::<Target>())?,
        )?;
        for t in &self.targets {
            n = checked_add(n, t.layer_id.capacity() + t.object_id.capacity() + 128)?;
        }
        n = checked_add(
            n,
            checked_mul(
                self.undo.capacity() + self.redo.capacity(),
                size_of::<Arc<Checkpoint>>() + 64,
            )?,
        )?;
        let mut seen = HashSet::new();
        for s in std::iter::once(&self.entry)
            .chain(std::iter::once(&self.work))
            .chain(self.preview.iter())
            .chain(&self.undo)
            .chain(&self.redo)
        {
            if seen.insert(Arc::as_ptr(s)) {
                n = checked_add(n, snapshot_bytes(s)?)?;
            }
        }
        if let Some(step) = &self.latest {
            n = checked_add(n, groups_bytes(step.groups())? + size_of::<Input>())?;
        }
        Ok(n)
    }
    fn admit(&self, extra: usize) -> Result<(), EditError> {
        let peak = checked_add(
            checked_add(self.resident_bytes()?, self.scratch_bytes)?,
            extra,
        )?;
        if peak > self.byte_limit {
            Err(EditError::ResourceLimit)
        } else {
            Ok(())
        }
    }
    pub fn resources(&self) -> Result<DraftResources, EditError> {
        let resident = self.resident_bytes()?;
        Ok(DraftResources {
            resident_bytes: resident,
            reserved_peak_bytes: checked_add(
                checked_add(resident, self.scratch_bytes)?,
                snapshot_bytes(&self.work)?,
            )?,
            byte_limit: self.byte_limit,
            undo_entries: self.undo.len(),
            redo_entries: self.redo.len(),
            entry_limit: self.entry_limit,
        })
    }
    pub fn set_step(
        &mut self,
        history: &EditHistory,
        document: &SemanticDocument,
        generation: u64,
        step: DraftStep,
    ) -> Result<u64, EditError> {
        self.check_fence(history, document)?;
        if generation != self.generation {
            return Err(EditError::InvalidArgument);
        }
        match &step.operation {
            SelectionEdit::Move { dx_mm, dy_mm }
                if MmPoint::new(*dx_mm, *dy_mm).is_valid_geometry() => {}
            SelectionEdit::Rotate {
                angle_deg,
                pivot_mm,
            } => {
                crate::transform::WorldTransform::rotation(*angle_deg, *pivot_mm)?;
            }
            SelectionEdit::Mirror { axis } => {
                crate::transform::WorldTransform::reflection(*axis)?;
            }
            _ => return Err(EditError::InvalidArgument),
        }
        self.set_input(history, document, generation, Input::Transform(step))
    }
    pub fn set_aperture_size_step(
        &mut self,
        history: &EditHistory,
        document: &SemanticDocument,
        generation: u64,
        step: DraftApertureSizeStep,
    ) -> Result<u64, EditError> {
        if !step.width_mm.is_finite()
            || !step.height_mm.is_finite()
            || step.width_mm <= 0.
            || step.height_mm <= 0.
        {
            return Err(EditError::InvalidArgument);
        }
        self.set_input(history, document, generation, Input::ApertureSize(step))
    }
    fn set_input(
        &mut self,
        history: &EditHistory,
        document: &SemanticDocument,
        generation: u64,
        step: Input,
    ) -> Result<u64, EditError> {
        self.check_fence(history, document)?;
        if generation != self.generation {
            return Err(EditError::InvalidArgument);
        }
        let selected = history.selection_targets(document, step.groups(), MAX_MOVE_OBJECTS)?;
        let entry: HashSet<_> = self.targets.iter().map(|t| (t.layer, t.index)).collect();
        if selected
            .iter()
            .any(|(l, ii)| ii.iter().any(|i| !entry.contains(&(*l, *i))))
        {
            return Err(EditError::InvalidArgument);
        }
        let next = self.next_generation()?;
        self.admit(
            groups_bytes(step.groups())? + size_of::<Input>() + snapshot_bytes(&self.work)?,
        )?;
        self.latest = Some(step);
        self.preview = None;
        self.generation = next;
        Ok(next)
    }
    fn calculate(
        &self,
        document: &SemanticDocument,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<Option<Arc<Checkpoint>>, EditError> {
        checkpoint()?;
        let Some(step) = &self.latest else {
            return Ok(None);
        };
        self.admit(snapshot_bytes(&self.work)?)?;
        let selected: HashSet<_> = step
            .groups()
            .iter()
            .flat_map(|g| {
                g.object_ids
                    .iter()
                    .map(move |id| (g.layer_id.as_str(), id.as_str()))
            })
            .collect();
        // Each target can add at most one rectangle definition and one bounded key.
        let max_key = document
            .id
            .len()
            .max(
                document
                    .apertures
                    .iter()
                    .map(|a| a.id.len())
                    .max()
                    .unwrap_or(0),
            )
            .checked_add(128)
            .ok_or(EditError::ResourceLimit)?;
        let extra = checked_mul(
            if matches!(step, Input::ApertureSize(_)) {
                self.targets.len()
            } else {
                0
            },
            checked_add(size_of::<DraftAperture>() + 512, checked_mul(max_key, 4)?)?,
        )?;
        self.admit(checked_add(snapshot_bytes(&self.work)?, extra)?)?;
        let mut candidate = (*self.work).clone();
        if matches!(step, Input::ApertureSize(_)) {
            candidate
                .apertures
                .try_reserve_exact(self.targets.len())
                .map_err(|_| EditError::ResourceLimit)?;
        }
        for (n, target) in self.targets.iter().enumerate() {
            checkpoint()?;
            if !selected.contains(&(target.layer_id.as_str(), target.object_id.as_str())) {
                continue;
            }
            let geometry = &mut candidate.geometry[n];
            match step {
                Input::Transform(s) => *geometry = s.operation.preview_geometry(geometry)?,
                Input::ApertureSize(s) => {
                    let SemanticGeometry::Flash { aperture_id, .. } = geometry else {
                        return Err(EditError::UnsupportedTransform);
                    };
                    let staged = candidate
                        .apertures
                        .iter()
                        .find(|a| a.definition.id == *aperture_id);
                    let base_id = staged.map_or(aperture_id.as_str(), |a| a.base_id.as_str());
                    let base = document
                        .apertures
                        .iter()
                        .find(|a| a.id == base_id)
                        .ok_or(EditError::InvalidArgument)?;
                    if !matches!(base.shape, ApertureShape::Rectangle { .. }) {
                        return Err(EditError::UnsupportedTransform);
                    }
                    let shape = resized_shape(&base.shape, s.width_mm, Some(s.height_mm))?;
                    super::validate_aperture_shape(&shape).map_err(EditError::InvalidGeometry)?;
                    if shape == base.shape {
                        *aperture_id = base.id.clone();
                        continue;
                    }
                    if let Some(a) = candidate
                        .apertures
                        .iter()
                        .find(|a| a.base_id == base.id && a.definition.shape == shape)
                    {
                        *aperture_id = a.definition.id.clone();
                        continue;
                    }
                    let id = format!(
                        "{}-draft-{}-{}-{}",
                        document.id, self.namespace, self.generation, n
                    );
                    if document.apertures.iter().any(|a| a.id == id)
                        || candidate.apertures.iter().any(|a| a.definition.id == id)
                    {
                        return Err(EditError::InvalidArgument);
                    }
                    let source_dcode = 0; // Assigned only to live overlays after pruning.
                    *aperture_id = id.clone();
                    candidate.apertures.push(DraftAperture {
                        base_id: base.id.clone(),
                        definition: ApertureDefinition {
                            id,
                            source_dcode,
                            shape,
                        },
                    });
                }
            }
        }
        let used: HashSet<_> = candidate
            .geometry
            .iter()
            .filter_map(|g| {
                if let SemanticGeometry::Flash { aperture_id, .. } = g {
                    Some(aperture_id.as_str())
                } else {
                    None
                }
            })
            .collect();
        candidate
            .apertures
            .retain(|a| used.contains(a.definition.id.as_str()));
        let mut dcode = document
            .apertures
            .iter()
            .map(|a| a.source_dcode)
            .max()
            .unwrap_or(9);
        for a in &mut candidate.apertures {
            checkpoint()?;
            dcode = dcode.checked_add(1).ok_or(EditError::ResourceLimit)?;
            a.definition.source_dcode = dcode;
        }
        let apertures = document
            .apertures
            .iter()
            .map(|a| a.id.clone())
            .chain(candidate.apertures.iter().map(|a| a.definition.id.clone()))
            .collect();
        let definitions = block_definition_ids(document);
        for geometry in &candidate.geometry {
            checkpoint()?;
            validate_geometry(geometry, &apertures, &definitions)
                .map_err(EditError::InvalidGeometry)?;
            validate_block_resolution(document, geometry)?;
        }
        self.admit(snapshot_bytes(&candidate)?)?;
        checkpoint()?;
        Ok(Some(Arc::new(candidate)))
    }
    pub fn preview_step(
        &mut self,
        history: &EditHistory,
        document: &SemanticDocument,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
        begin_publish: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<(), EditError> {
        self.preview_step_prepared(
            history,
            document,
            checkpoint,
            &mut |_| Ok(()),
            begin_publish,
        )
    }
    pub fn preview_step_prepared(
        &mut self,
        history: &EditHistory,
        document: &SemanticDocument,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), EditError>,
        begin_publish: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<(), EditError> {
        self.check_fence(history, document)?;
        let preview = self.calculate(document, checkpoint)?;
        prepare(self.candidate(preview.as_ref().unwrap_or(&self.work)))?;
        checkpoint()?;
        begin_publish()?;
        self.preview = preview;
        Ok(())
    }
    pub fn execute_step(
        &mut self,
        history: &EditHistory,
        document: &SemanticDocument,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
        begin_publish: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<u64, EditError> {
        self.execute_step_prepared(
            history,
            document,
            checkpoint,
            &mut |_| Ok(()),
            begin_publish,
        )
    }
    pub fn execute_step_prepared(
        &mut self,
        history: &EditHistory,
        document: &SemanticDocument,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), EditError>,
        begin_publish: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<u64, EditError> {
        self.check_fence(history, document)?;
        if self.latest.is_none() {
            return Err(EditError::InvalidArgument);
        }
        if self.undo.len() >= self.entry_limit {
            return Err(EditError::ResourceLimit);
        }
        let next = self.next_generation()?;
        let candidate = self
            .calculate(document, checkpoint)?
            .ok_or(EditError::InvalidArgument)?;
        self.admit(snapshot_bytes(&candidate)? + size_of::<Arc<Checkpoint>>() + 64)?;
        self.undo
            .try_reserve_exact(1)
            .map_err(|_| EditError::ResourceLimit)?;
        prepare(self.candidate(&candidate))?;
        checkpoint()?;
        begin_publish()?;
        self.undo.push(Arc::clone(&self.work));
        self.work = candidate;
        self.redo.clear();
        self.preview = None;
        self.latest = None;
        self.generation = next;
        Ok(next)
    }
    pub fn undo(&mut self) -> Result<u64, EditError> {
        self.undo_prepared(&mut |_| Ok(()), &mut || Ok(()))
    }
    pub fn undo_prepared(
        &mut self,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), EditError>,
        begin_publish: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<u64, EditError> {
        if self.undo.is_empty() {
            return Err(EditError::EmptyHistory);
        }
        let next = self.next_generation()?;
        self.admit(size_of::<Arc<Checkpoint>>() + 64)?;
        self.redo
            .try_reserve_exact(1)
            .map_err(|_| EditError::ResourceLimit)?;
        prepare(self.candidate(self.undo.last().unwrap()))?;
        begin_publish()?;
        self.redo.push(Arc::clone(&self.work));
        self.work = self.undo.pop().unwrap();
        self.preview = None;
        self.latest = None;
        self.generation = next;
        Ok(next)
    }
    pub fn redo(&mut self) -> Result<u64, EditError> {
        self.redo_prepared(&mut |_| Ok(()), &mut || Ok(()))
    }
    pub fn redo_prepared(
        &mut self,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), EditError>,
        begin_publish: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<u64, EditError> {
        if self.redo.is_empty() {
            return Err(EditError::EmptyHistory);
        }
        let next = self.next_generation()?;
        self.admit(size_of::<Arc<Checkpoint>>() + 64)?;
        self.undo
            .try_reserve_exact(1)
            .map_err(|_| EditError::ResourceLimit)?;
        prepare(self.candidate(self.redo.last().unwrap()))?;
        begin_publish()?;
        self.undo.push(Arc::clone(&self.work));
        self.work = self.redo.pop().unwrap();
        self.preview = None;
        self.latest = None;
        self.generation = next;
        Ok(next)
    }
    pub fn reset(&mut self) -> Result<u64, EditError> {
        self.reset_prepared(&mut |_| Ok(()), &mut || Ok(()))
    }
    pub fn reset_prepared(
        &mut self,
        prepare: &mut impl FnMut(DraftCandidate<'_>) -> Result<(), EditError>,
        begin_publish: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<u64, EditError> {
        let next = self.next_generation()?;
        prepare(self.candidate(&self.entry))?;
        begin_publish()?;
        self.work = Arc::clone(&self.entry);
        self.undo.clear();
        self.redo.clear();
        self.preview = None;
        self.latest = None;
        self.generation = next;
        Ok(next)
    }
}
