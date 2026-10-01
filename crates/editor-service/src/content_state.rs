//! Session-only, ordered canonical content signatures. Never a disk digest.
use super::*;
use editor_core::edit::ContentChange;
use std::sync::Mutex;

const CHUNK_OBJECTS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    header: String,
    layers: Vec<(String, Vec<String>)>,
}
impl Fingerprint {
    fn header(document: &SemanticDocument) -> String {
        content_hash(&(
            &document.id,
            &document.unit,
            &document.format,
            &document.source,
            &document.apertures,
            &document.block_definitions,
        ))
    }
    fn build(document: &SemanticDocument) -> Self {
        Self {
            header: Self::header(document),
            layers: document
                .layers
                .iter()
                .map(|layer| {
                    (
                        layer.id.clone(),
                        layer
                            .objects
                            .chunks(CHUNK_OBJECTS)
                            .map(content_hash)
                            .collect(),
                    )
                })
                .collect(),
        }
    }
    fn update(&mut self, document: &SemanticDocument, change: &ContentChange) -> bool {
        if self.layers.len() != document.layers.len()
            || self.layers.iter().zip(&document.layers).enumerate().any(
                |(index, ((id, hashes), layer))| {
                    *id != layer.id
                        || ((index != change.layer || change.tail_from.is_none())
                            && hashes.len() != layer.objects.len().div_ceil(CHUNK_OBJECTS))
                },
            )
        {
            return false;
        }
        let Some(layer) = document.layers.get(change.layer) else {
            return false;
        };
        if change
            .object_indices
            .iter()
            .any(|&i| i >= layer.objects.len())
        {
            return false;
        }
        let mut chunks: Vec<_> = if let Some(first) = change.tail_from {
            let count = layer.objects.len().div_ceil(CHUNK_OBJECTS);
            self.layers[change.layer].1.resize(count, String::new());
            (first / CHUNK_OBJECTS..count).collect()
        } else {
            change
                .object_indices
                .iter()
                .map(|i| i / CHUNK_OBJECTS)
                .collect()
        };
        chunks.sort_unstable();
        chunks.dedup();
        for chunk in chunks {
            let start = chunk * CHUNK_OBJECTS;
            let end = (start + CHUNK_OBJECTS).min(layer.objects.len());
            self.layers[change.layer].1[chunk] = content_hash(&layer.objects[start..end]);
        }
        self.header = Self::header(document);
        true
    }
}

#[derive(Debug, Clone)]
struct State {
    saved: Fingerprint,
    current: Fingerprint,
    revision: u64,
    generation: u64,
    dirty: bool,
}

#[derive(Debug)]
pub(super) struct ContentState(Mutex<State>);
impl Clone for ContentState {
    fn clone(&self) -> Self {
        Self(Mutex::new(
            self.0.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        ))
    }
}
impl ContentState {
    pub(super) fn new(document: &SemanticDocument, revision: u64, generation: u64) -> Self {
        let current = Fingerprint::build(document);
        Self(Mutex::new(State {
            saved: current.clone(),
            current,
            revision,
            generation,
            dirty: false,
        }))
    }
    pub(super) fn is_dirty(
        &self,
        document: &SemanticDocument,
        revision: u64,
        history: &EditHistory,
    ) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if state.revision == revision {
            return state.dirty;
        }
        let generation = history.content_generation();
        if generation == state.generation {
            // Board edits do not change SemanticDocument. Header remains explicit.
            state.current.header = Fingerprint::header(document);
        } else if state.generation.checked_add(1) != Some(generation)
            || history
                .last_content_change()
                .is_none_or(|change| !state.current.update(document, change))
        {
            state.current = Fingerprint::build(document);
        }
        state.dirty = state.current != state.saved;
        state.revision = revision;
        state.generation = generation;
        state.dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn assert_oracle(
        cache: &ContentState,
        document: &SemanticDocument,
        baseline: &[u8],
        revision: u64,
        history: &EditHistory,
    ) {
        assert_eq!(
            cache.is_dirty(document, revision, history),
            serde_json::to_vec(document).unwrap() != baseline
        );
        assert_eq!(
            cache.0.lock().unwrap().current,
            Fingerprint::build(document)
        );
    }
    #[test]
    fn chunk_boundaries_signed_zero_and_header_fields_match_oracle() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s1a1/g75_exact.gbr");
        let mut doc = gerber_io::parse_s1(&fs::read(root).unwrap(), "chunks")
            .unwrap()
            .document;
        doc.layers[0].objects = (0..1537)
            .map(|i| editor_core::SemanticObject {
                object_id: format!("chunk-{i}"),
                geometry: editor_core::SemanticGeometry::Line {
                    start: MmPoint::new(if i == 512 { -0.0 } else { i as f64 }, 0.),
                    end: MmPoint::new(i as f64 + 1., 0.),
                    width_mm: 0.2,
                },
                exposure: editor_core::Exposure::Dark,
                origin: editor_core::ObjectOrigin::Generated {
                    operation_id: "chunks".into(),
                },
            })
            .collect();
        let baseline = serde_json::to_vec(&doc).unwrap();
        let cache = ContentState::new(&doc, 0, 0);
        let mut history = EditHistory::default();
        let layer = doc.layers[0].id.clone();
        history
            .move_objects(
                &mut doc,
                &layer,
                &["chunk-511".into(), "chunk-512".into(), "chunk-1536".into()],
                0.,
                1.,
            )
            .unwrap();
        assert_oracle(&cache, &doc, &baseline, 1, &history);
        let state = cache.0.lock().unwrap();
        assert_eq!(
            state.current.layers[0]
                .1
                .iter()
                .zip(&state.saved.layers[0].1)
                .filter(|(a, b)| a != b)
                .count(),
            3
        );
        drop(state);
        history.undo(&mut doc).unwrap();
        assert_oracle(&cache, &doc, &baseline, 2, &history);
        // Numeric PartialEq sees signed zero as equal; canonical signatures must not.
        let equal = doc.clone();
        let editor_core::SemanticGeometry::Line { start, .. } =
            &mut doc.layers[0].objects[512].geometry
        else {
            unreachable!()
        };
        start.x_mm = 0.0;
        assert_eq!(doc, equal);
        assert_ne!(Fingerprint::build(&doc), Fingerprint::build(&equal));
        doc = equal;
        history
            .duplicate_objects(&mut doc, &layer, &["chunk-512".into()], 0., 0.)
            .unwrap();
        assert_oracle(&cache, &doc, &baseline, 3, &history);
        history.undo(&mut doc).unwrap();
        assert_oracle(&cache, &doc, &baseline, 4, &history);
        history
            .add_layers(
                &mut doc,
                vec![editor_core::edit::LayerAdd {
                    layer: editor_core::SemanticLayer {
                        id: "extra".into(),
                        objects: vec![],
                    },
                    apertures: vec![],
                }],
            )
            .unwrap();
        assert_oracle(&cache, &doc, &baseline, 5, &history);
        history.undo(&mut doc).unwrap();
        assert_oracle(&cache, &doc, &baseline, 6, &history);
        let mut metadata = doc.clone();
        metadata.source.encoding = Some("different".into());
        assert_ne!(Fingerprint::build(&metadata), Fingerprint::build(&doc));
        let mut reordered = doc.clone();
        reordered.layers[0].objects.swap(510, 511);
        assert_ne!(Fingerprint::build(&reordered), Fingerprint::build(&doc));
    }

    #[test]
    fn incremental_dirty_matches_full_json_across_history_branches_and_structure() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s1a1/g75_exact.gbr");
        let scene = gerber_io::parse_s1(&fs::read(root).unwrap(), "dirty-cache").unwrap();
        let mut doc = scene.document;
        let mut history = EditHistory::default();
        history.seed_generated_ids(&doc).unwrap();
        let layer = doc.layers[0].id.clone();
        let object = doc.layers[0].objects[0].object_id.clone();
        let baseline = serde_json::to_vec(&doc).unwrap();
        let cache = ContentState::new(&doc, 0, 0);
        history
            .move_objects(&mut doc, &layer, std::slice::from_ref(&object), 1., 0.)
            .unwrap();
        assert_oracle(&cache, &doc, &baseline, 1, &history);
        history.undo(&mut doc).unwrap();
        assert_oracle(&cache, &doc, &baseline, 2, &history);
        history.redo(&mut doc).unwrap();
        assert_oracle(&cache, &doc, &baseline, 3, &history);
        // Two unobserved commits invalidate a stale partial hint.
        history
            .move_objects(&mut doc, &layer, std::slice::from_ref(&object), -1., 0.)
            .unwrap();
        history
            .move_objects(&mut doc, &layer, std::slice::from_ref(&object), 1., 0.)
            .unwrap();
        assert_oracle(&cache, &doc, &baseline, 5, &history);
        history.undo(&mut doc).unwrap();
        assert_oracle(&cache, &doc, &baseline, 6, &history);
        history
            .delete_objects(&mut doc, &layer, std::slice::from_ref(&object))
            .unwrap();
        assert_oracle(&cache, &doc, &baseline, 7, &history);
        history.undo(&mut doc).unwrap();
        assert_oracle(&cache, &doc, &baseline, 8, &history);
        let saved = ContentState::new(&doc, 8, history.content_generation());
        assert!(!saved.clone().is_dirty(&doc, 8, &history));
        assert!(
            history
                .move_objects(&mut doc, &layer, &["missing".into()], 1., 0.)
                .is_err()
        );
        assert!(!saved.is_dirty(&doc, 8, &history));
    }
}
