//! Display-only memoization for resolving `BlockInstance` geometry (S4-B2
//! Final Closeout §2.2). Many instances of one shared `BlockDefinition`
//! commonly differ in world position but not in rotation/mirror (a steel-mesh
//! pattern repeats the same opening at a handful of orientations); caching
//! the rotation+mirror resolution per `(definition_id, revision)` and only
//! translating it per instance avoids re-deriving `resolve_instance`'s
//! rotation/reflection math for every one of many instances that share an
//! orientation. Never persisted, never manufacturing data.
use editor_core::block::{BlockDefinition, BlockTransform, ResolvedBlockObject, resolve_instance};

/// Losing reuse only: every entry is content-addressed by
/// `(definition_id, revision, rotation, mirror)`, so a full clear can never
/// serve a stale result — it only makes the next lookup for an evicted key
/// pay the resolve cost again.
const MAX_CACHE_ENTRIES: usize = 4096;

type Key = (String, u64, u64, bool);

#[derive(Default)]
pub struct BlockDisplayCache {
    entries: std::collections::HashMap<Key, Vec<ResolvedBlockObject>>,
}

impl BlockDisplayCache {
    #[cfg(test)]
    pub fn stats(&self) -> (usize, usize) {
        (
            self.entries.len(),
            self.entries.values().map(Vec::len).sum(),
        )
    }

    /// Resolve `definition` under `transform` in world space.
    pub fn resolve(
        &mut self,
        definition: &BlockDefinition,
        transform: &BlockTransform,
    ) -> Result<Vec<ResolvedBlockObject>, String> {
        let key = (
            definition.id.0.clone(),
            definition.revision,
            transform.rotation_deg.to_bits(),
            transform.mirror,
        );
        let local = if let Some(cached) = self.entries.get(&key) {
            cached.clone()
        } else {
            let resolved = resolve_instance(
                definition,
                &BlockTransform {
                    translation: editor_core::MmPoint::new(0., 0.),
                    rotation_deg: transform.rotation_deg,
                    mirror: transform.mirror,
                },
            )
            .map_err(|e| format!("VALIDATION_FAILED: block instance orientation {e:?}"))?;
            if self.entries.len() >= MAX_CACHE_ENTRIES {
                self.entries.clear();
            }
            self.entries.insert(key, resolved.clone());
            resolved
        };
        if transform.translation.x_mm == 0. && transform.translation.y_mm == 0. {
            return Ok(local);
        }
        let mut world = local;
        editor_core::block::translate_resolved(
            &mut world,
            transform.translation.x_mm,
            transform.translation.y_mm,
        )
        .map_err(|e| format!("VALIDATION_FAILED: block instance placement {e:?}"))?;
        Ok(world)
    }
}
