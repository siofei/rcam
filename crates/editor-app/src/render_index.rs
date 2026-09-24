//! Bounded, ordered, display-only world-space bins. No manufacturing state.
use crate::display::Object;

pub const MAX_GRID_CELLS: usize = 262_144;
pub const MAX_CELL_REFERENCES: usize = 1_000_000;
const MAX_RENDER_OBJECTS: usize = 1_000_000;
const MAX_OBJECTS_PER_CELL: usize = 16_384;
#[derive(Clone, Debug, Default)]
pub struct RenderIndex {
    pub world: [f32; 4],
    pub grid: [f32; 4], // local-world origin and inverse cell size
    pub cols: u32,
    pub rows: u32,
    pub data: Vec<u32>, // offsets (absolute), then ordered object indices
    pub max_candidates: usize,
}
#[derive(Debug, Default)]
pub struct ViewportRenderSet {
    pub world_bounds: [f64; 4],
    pub cell_range: Option<[usize; 4]>,
    pub ordered_candidate_ids: Vec<u32>,
    pub max_candidates_in_view: usize,
    pub cell_references_visited: usize,
}
fn limit(resource: &str, limit: usize, actual: usize) -> String {
    format!("RESOURCE_LIMIT: resource={resource} limit={limit} actual={actual}")
}
impl RenderIndex {
    pub fn build(objects: &[Object], selected: &[u32], delta: [f32; 2]) -> Result<Self, String> {
        let _timing = rcam_diagnostics::Timing::start("render_index.build");
        Self::bounded(objects, selected, delta, MAX_CELL_REFERENCES)
    }
    fn bounded(
        objects: &[Object],
        selected: &[u32],
        delta: [f32; 2],
        budget: usize,
    ) -> Result<Self, String> {
        if objects.len() > MAX_RENDER_OBJECTS {
            return Err(limit("render_objects", MAX_RENDER_OBJECTS, objects.len()));
        }
        let bounds: Vec<_> = objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.meta[3] != 0)
            .map(|(i, o)| {
                let d = if selected.get(i).copied().unwrap_or(0) != 0 {
                    delta
                } else {
                    [0.; 2]
                };
                (
                    i as u32,
                    std::array::from_fn::<_, 4, _>(|k| o.bounds[k] + d[k % 2]),
                )
            })
            .collect();
        if bounds
            .iter()
            .any(|(_, b)| b.iter().any(|x| !x.is_finite()) || b[0] > b[2] || b[1] > b[3])
        {
            return Err("VALIDATION_FAILED: invalid render index bounds".into());
        }
        let mut world = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for (_, b) in &bounds {
            for k in 0..2 {
                world[k] = world[k].min(b[k]);
                world[k + 2] = world[k + 2].max(b[k + 2]);
            }
        }
        if bounds.is_empty() {
            world = [0., 0., 1., 1.];
        }
        let w = (f64::from(world[2]) - f64::from(world[0])).max(1e-20);
        let h = (f64::from(world[3]) - f64::from(world[1])).max(1e-20);
        // Denser bins keep zoomed, sparse viewports from charging every
        // sample for unrelated nearby objects. Reference budget still bounds
        // the index and the loop below coarsens when necessary.
        let cells = bounds
            .len()
            .saturating_mul(16)
            .max(if bounds.len() >= 16 { 4096 } else { 1 })
            .clamp(1, MAX_GRID_CELLS);
        let mut cols = ((cells as f64 * w / h).sqrt().ceil() as usize).clamp(1, cells);
        let mut rows = (cells / cols).max(1);
        loop {
            let grid = [
                world[0],
                world[1],
                (cols as f64 / w) as f32,
                (rows as f64 / h) as f32,
            ];
            if grid.iter().any(|v| !v.is_finite()) {
                return Err("VALIDATION_FAILED: render index scale".into());
            }
            let mut index = Self {
                // Translation/subtraction can round in opposite directions in WGSL.
                // Expand only the global early-out envelope; cell grid and material tests stay unchanged.
                world: std::array::from_fn(|k| {
                    let axis = k % 2;
                    let pad =
                        world[axis].abs().max(world[axis + 2].abs()).max(1.) * f32::EPSILON * 8.;
                    world[k] + if k < 2 { -pad } else { pad }
                }),
                grid,
                cols: cols as u32,
                rows: rows as u32,
                ..Default::default()
            };
            let mut counts = vec![0usize; cols * rows];
            let mut total = 0usize;
            for (_, b) in &bounds {
                let [x0, y0, x1, y1] = index.range(*b);
                total = total.saturating_add((x1 - x0 + 1) * (y1 - y0 + 1));
                if total > budget {
                    break;
                }
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        counts[y * cols + x] += 1;
                    }
                }
            }
            if total > budget {
                if cols == 1 && rows == 1 {
                    return Err(limit("cell_references", budget, total));
                }
                cols = cols.div_ceil(2);
                rows = rows.div_ceil(2);
                continue;
            }
            index.max_candidates = counts.iter().copied().max().unwrap_or(0);
            if index.max_candidates > MAX_OBJECTS_PER_CELL {
                return Err(limit(
                    "objects_per_cell",
                    MAX_OBJECTS_PER_CELL,
                    index.max_candidates,
                ));
            }
            index.data = Vec::with_capacity(counts.len() + 1 + total);
            let mut offset = (counts.len() + 1) as u32;
            index.data.push(offset);
            for count in counts {
                offset += count as u32;
                index.data.push(offset);
            }
            let mut cursor = index.data[..cols * rows].to_vec();
            index.data.resize(offset as usize, 0);
            // Insert in scene order, never sort by aperture/polarity/layer.
            for (id, b) in &bounds {
                let [x0, y0, x1, y1] = index.range(*b);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let c = y * cols + x;
                        index.data[cursor[c] as usize] = *id;
                        cursor[c] += 1;
                    }
                }
            }
            return Ok(index);
        }
    }
    /// Conservative query in scene-local world mm; never scans the scene.
    pub fn viewport(&self, bounds: [f64; 4]) -> ViewportRenderSet {
        let mut result = ViewportRenderSet {
            world_bounds: bounds,
            ..Default::default()
        };
        if (0..2).any(|k| {
            bounds[k + 2] < f64::from(self.world[k]) || bounds[k] > f64::from(self.world[k + 2])
        }) {
            return result;
        }
        let cell = |v: f64, k: usize| {
            (((v - f64::from(self.grid[k])) * f64::from(self.grid[k + 2]))
                .floor()
                .max(0.) as usize)
                .min(if k == 0 {
                    self.cols as usize - 1
                } else {
                    self.rows as usize - 1
                })
        };
        let range = [
            cell(bounds[0], 0).saturating_sub(1),
            cell(bounds[1], 1).saturating_sub(1),
            (cell(bounds[2], 0) + 1).min(self.cols as usize - 1),
            (cell(bounds[3], 1) + 1).min(self.rows as usize - 1),
        ];
        result.cell_range = Some(range);
        for y in range[1]..=range[3] {
            for x in range[0]..=range[2] {
                let c = y * self.cols as usize + x;
                let ids = &self.data[self.data[c] as usize..self.data[c + 1] as usize];
                result.max_candidates_in_view = result.max_candidates_in_view.max(ids.len());
                result.cell_references_visited += ids.len();
                result.ordered_candidate_ids.extend_from_slice(ids);
            }
        }
        result.ordered_candidate_ids.sort_unstable();
        result.ordered_candidate_ids.dedup();
        result
    }
    /// Bound actual per-cell candidate visits, not the densest cell multiplied
    /// by every pixel in an otherwise sparse viewport. The shader indexes one
    /// cell per sample. The viewport already includes a two-pixel halo for
    /// AA/selected-edge samples, so adding a halo to every intersected cell
    /// would charge the same pixels repeatedly on a fine grid.
    pub fn sample_candidate_work(&self, view: &ViewportRenderSet, ppm: f64) -> f64 {
        let Some([x0, y0, x1, y1]) = view.cell_range else {
            return 0.;
        };
        let mut work = 0.;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let cell = y * self.cols as usize + x;
                let count = self.data[cell + 1] - self.data[cell];
                let mut dimensions = [0.; 2];
                for (axis, coordinate) in [x, y].into_iter().enumerate() {
                    let origin = f64::from(self.grid[axis]);
                    let inverse = f64::from(self.grid[axis + 2]);
                    let lo = origin + coordinate as f64 / inverse;
                    let hi = origin + (coordinate + 1) as f64 / inverse;
                    let visible =
                        hi.min(view.world_bounds[axis + 2]) - lo.max(view.world_bounds[axis]);
                    dimensions[axis] = (visible * ppm).max(0.);
                }
                work += dimensions[0] * dimensions[1] * f64::from(count) * 20.;
            }
        }
        work
    }
    fn range(&self, b: [f32; 4]) -> [usize; 4] {
        // Pad by f32 rounding error at cell boundaries. A whole-cell halo
        // multiplies references for dense flash arrays and can exhaust the
        // bounded index even when each flash occupies only one cell.
        let cell = |v: f32, axis: usize| -> usize {
            (((v - self.grid[axis]) * self.grid[axis + 2])
                .floor()
                .max(0.) as usize)
                .min(if axis == 0 {
                    self.cols as usize - 1
                } else {
                    self.rows as usize - 1
                })
        };
        let pad = |v: f32, axis: usize| {
            let span = (if axis == 0 { self.cols } else { self.rows }) as f32 / self.grid[axis + 2];
            v.abs().max(self.grid[axis].abs()).max(span.abs()).max(1.) * f32::EPSILON * 16.
        };
        [
            cell(b[0] - pad(b[0], 0), 0),
            cell(b[1] - pad(b[1], 1), 1),
            cell(b[2] + pad(b[2], 0), 0),
            cell(b[3] + pad(b[3], 1), 1),
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn objects(n: usize) -> Vec<Object> {
        (0..n)
            .map(|i| {
                let x = (i % 40) as f32 * 3.;
                let y = (i / 40) as f32 * 3.;
                Object {
                    meta: [0, 1, (i % 2) as u32, (i / 500 + 1) as u32],
                    bounds: [x, y, x + 1., y + 1.],
                    ..Default::default()
                }
            })
            .collect()
    }
    #[test]
    fn render_index_preserves_scene_order_and_exposure_layer_isolation() {
        let o = objects(1000);
        let r = RenderIndex::build(&o, &[], [0.; 2]).unwrap();
        for c in 0..(r.cols * r.rows) as usize {
            let ids = &r.data[r.data[c] as usize..r.data[c + 1] as usize];
            assert!(ids.windows(2).all(|x| x[0] < x[1]));
        }
        assert!(r.max_candidates < 40);
    }
    #[test]
    fn object_spanning_cells_appears_in_every_required_cell() {
        let mut o = objects(1000);
        o[0].bounds = [-1., -1., 120., 75.];
        let r = RenderIndex::build(&o, &[], [0.; 2]).unwrap();
        for c in 0..(r.cols * r.rows) as usize {
            assert_eq!(r.data[r.data[c] as usize], 0);
        }
    }
    #[test]
    fn coarsening_keeps_all_candidates_and_budget_is_bounded() {
        let o = objects(1000);
        let r = RenderIndex::bounded(&o, &[], [0.; 2], 1000).unwrap();
        assert!(r.data.len() <= r.cols as usize * r.rows as usize + 1 + 1000);
        assert_eq!(
            r.viewport([-10., -10., 200., 200.]).ordered_candidate_ids,
            (0..1000).collect::<Vec<_>>()
        );
        assert!(
            RenderIndex::bounded(&o, &[], [0.; 2], 999)
                .unwrap_err()
                .contains("resource=cell_references")
        );
        assert!(
            RenderIndex::build(&vec![o[0]; MAX_OBJECTS_PER_CELL + 1], &[], [0.; 2])
                .unwrap_err()
                .contains("objects_per_cell")
        );
    }
    #[test]
    fn preview_translation_updates_candidate_membership() {
        let o = objects(1000);
        let before = o[0].bounds;
        let mut flags = vec![0; 1000];
        flags[0] = 1;
        let r = RenderIndex::build(&o, &flags, [-100., -100.]).unwrap();
        assert_eq!(r.grid[0], -100.);
        assert_eq!(o[0].bounds, before);
        assert!(r.data[r.data[0] as usize..r.data[1] as usize].contains(&0));
    }

    #[test]
    fn dense_flash_grid_keeps_bounded_index_without_a_whole_cell_halo() {
        let objects: Vec<_> = (0..230_409)
            .map(|i| {
                let x = (i % 481) as f32 * 0.3;
                let y = (i / 481) as f32 * 0.3;
                Object {
                    meta: [0, 1, 1, 1],
                    bounds: [x, y, x + 0.09, y + 0.09],
                    ..Default::default()
                }
            })
            .collect();
        let index = RenderIndex::build(&objects, &[], [0.; 2]).unwrap();
        assert!(index.max_candidates < 1_000);
        assert!(index.data.len() <= MAX_CELL_REFERENCES + MAX_GRID_CELLS + 1);
        assert_eq!(
            index
                .viewport([0., 0., 145., 145.])
                .ordered_candidate_ids
                .len(),
            objects.len()
        );
    }
}
