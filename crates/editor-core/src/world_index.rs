//! Shared f64 manufacturing envelope index, independent of view policy.
use crate::*;
pub type EnvelopeEntry = (BoundsMm, usize, usize);
#[derive(Debug, Clone, Default)]
pub struct WorldIndex {
    entries: Vec<EnvelopeEntry>,
    nodes: Vec<Node>,
    order: Vec<usize>,
}
#[derive(Debug, Clone)]
struct Node {
    bounds: BoundsMm,
    range: std::ops::Range<usize>,
    children: Option<(usize, usize)>,
}
fn intersects(a: BoundsMm, b: BoundsMm) -> bool {
    a.min_x_mm <= b.max_x_mm
        && a.max_x_mm >= b.min_x_mm
        && a.min_y_mm <= b.max_y_mm
        && a.max_y_mm >= b.min_y_mm
}
impl WorldIndex {
    pub fn build(
        layers: &[SemanticLayer],
        apertures: &[ApertureDefinition],
        blocks: &[block::BlockDefinition],
    ) -> Result<Self, String> {
        let mut entries = Vec::new();
        let bounds = individual_geometries_bounds_with_blocks(
            layers.iter().flat_map(|l| &l.objects).map(|o| &o.geometry),
            apertures,
            blocks,
        )
        .map_err(|e| format!("VALIDATION_FAILED: world envelope: {e}"))?;
        let mut bounds = bounds.into_iter();
        for (layer, data) in layers.iter().enumerate() {
            for object in 0..data.objects.len() {
                if let Some(bounds) = bounds.next().flatten() {
                    entries.push((bounds, layer, object));
                }
            }
        }
        entries.sort_by(|a, b| a.0.min_x_mm.total_cmp(&b.0.min_x_mm));
        let mut result = Self {
            entries,
            nodes: vec![],
            order: vec![],
        };
        result.rebuild_tree();
        Ok(result)
    }
    fn rebuild_tree(&mut self) {
        self.nodes.clear();
        self.order = (0..self.entries.len()).collect();
        if !self.order.is_empty() {
            self.build_node(0..self.order.len());
        }
    }
    fn refit_tree(&mut self) {
        for i in (0..self.nodes.len()).rev() {
            let b = if let Some((a, b)) = self.nodes[i].children {
                self.nodes[a].bounds.union(self.nodes[b].bounds)
            } else {
                self.order[self.nodes[i].range.clone()]
                    .iter()
                    .map(|j| self.entries[*j].0)
                    .reduce(BoundsMm::union)
                    .unwrap()
            };
            self.nodes[i].bounds = b;
        }
    }
    fn build_node(&mut self, range: std::ops::Range<usize>) -> usize {
        let bounds = self.order[range.clone()]
            .iter()
            .map(|i| self.entries[*i].0)
            .reduce(BoundsMm::union)
            .unwrap();
        let id = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            range: range.clone(),
            children: None,
        });
        if range.len() > 16 {
            let x = bounds.max_x_mm - bounds.min_x_mm >= bounds.max_y_mm - bounds.min_y_mm;
            let middle = range.len() / 2;
            self.order[range.clone()].select_nth_unstable_by(middle, |a, b| {
                let p = self.entries[*a].0.center();
                let q = self.entries[*b].0.center();
                (if x {
                    p.x_mm.total_cmp(&q.x_mm)
                } else {
                    p.y_mm.total_cmp(&q.y_mm)
                })
                .then(a.cmp(b))
            });
            let m = range.start + middle;
            let left = self.build_node(range.start..m);
            let right = self.build_node(m..range.end);
            self.nodes[id].children = Some((left, right));
        }
        id
    }

    pub fn update(
        &self,
        layers: &[SemanticLayer],
        apertures: &[ApertureDefinition],
        blocks: &[block::BlockDefinition],
        changed: &[(usize, usize)],
    ) -> Result<Self, String> {
        let bounds = individual_geometries_bounds_with_blocks(
            changed.iter().map(|&(l, o)| &layers[l].objects[o].geometry),
            apertures,
            blocks,
        )
        .map_err(|e| format!("VALIDATION_FAILED: world envelope: {e}"))?;
        let mut updates: std::collections::HashMap<_, _> =
            changed.iter().copied().zip(bounds).collect();
        let mut result = self.clone();
        result
            .entries
            .retain_mut(|(bounds, l, o)| match updates.remove(&(*l, *o)) {
                Some(Some(next)) => {
                    *bounds = next;
                    true
                }
                Some(None) => false,
                None => true,
            });
        result.entries.extend(
            updates
                .into_iter()
                .filter_map(|((l, o), b)| b.map(|b| (b, l, o))),
        );
        if result.entries.len() == self.entries.len() {
            result.refit_tree();
        } else {
            result.rebuild_tree();
        }
        Ok(result)
    }
    /// Object-level neighborhood for Object Snap. This returns only stable
    /// snapshot indices; feature generation remains lazy in the caller.
    /// Explicit layer scope, no view permissions, reject before partial output.
    pub fn nearby(
        &self,
        view: BoundsMm,
        layers: &[usize],
        budget: usize,
    ) -> Result<Vec<EnvelopeEntry>, usize> {
        self.nearby_with_stats(view, layers, budget)
            .map(|(items, _)| items)
    }
    /// Counts leaf envelope tests so sparse-neighborhood work can be audited.
    pub fn nearby_with_stats(
        &self,
        view: BoundsMm,
        layers: &[usize],
        budget: usize,
    ) -> Result<(Vec<EnvelopeEntry>, usize), usize> {
        let mut result = Vec::new();
        let mut tested = 0;
        let mut stack = vec![];
        if !self.nodes.is_empty() {
            stack.push(0);
        }
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i];
            if !intersects(n.bounds, view) {
                continue;
            }
            if let Some((a, b)) = n.children {
                stack.push(b);
                stack.push(a);
            } else {
                for &i in &self.order[n.range.clone()] {
                    let (b, l, o) = self.entries[i];
                    tested += 1;
                    if (layers.is_empty() || layers.contains(&l)) && intersects(b, view) {
                        if result.len() == budget {
                            return Err(budget.saturating_add(1));
                        }
                        result.push((b, l, o));
                    }
                }
            }
        }
        Ok((result, tested))
    }
    pub fn entries(&self) -> &[(BoundsMm, usize, usize)] {
        &self.entries
    }
    pub fn query_indices(&self, view: BoundsMm) -> Vec<(usize, usize)> {
        let mut indices: Vec<_> = self
            .nearby(view, &[], usize::MAX)
            .unwrap()
            .into_iter()
            .map(|(_, l, o)| (l, o))
            .collect();
        indices.sort_unstable();
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_axis_tree_sparse_queries_and_updates_match_independent_scan() {
        let mut layer = SemanticLayer {
            id: "l".into(),
            objects: (0..100000)
                .map(|i| SemanticObject {
                    object_id: format!("o{i}"),
                    geometry: SemanticGeometry::Line {
                        start: MmPoint::new(0., i as f64 * 2.),
                        end: MmPoint::new(1., i as f64 * 2.),
                        width_mm: 0.1,
                    },
                    exposure: Exposure::Dark,
                    origin: ObjectOrigin::Generated {
                        operation_id: "test".into(),
                    },
                })
                .collect(),
        };
        let index = WorldIndex::build(std::slice::from_ref(&layer), &[], &[]).unwrap();
        let view = BoundsMm {
            min_x_mm: -1.,
            max_x_mm: 2.,
            min_y_mm: 99999.,
            max_y_mm: 100001.,
        };
        let (nearby, tested) = index.nearby_with_stats(view, &[0], 10000).unwrap();
        assert!(tested < 100);
        assert_eq!(nearby.len(), 1);
        assert_eq!(nearby[0].2, 50000);
        let oracle: Vec<_> = index
            .entries()
            .iter()
            .filter(|(b, _, _)| intersects(*b, view))
            .map(|(_, l, o)| (*l, *o))
            .collect();
        assert_eq!(index.query_indices(view), oracle);
        layer.objects[50000].geometry = SemanticGeometry::Line {
            start: MmPoint::new(10., 0.),
            end: MmPoint::new(11., 0.),
            width_mm: 0.1,
        };
        let updated = index.update(&[layer], &[], &[], &[(0, 50000)]).unwrap();
        assert!(updated.query_indices(view).is_empty());
    }
}
