//! Bounded, ordered, display-only world-space bins. No manufacturing state.
use crate::display::Object;

pub const MAX_GRID_CELLS: usize = 16_384;
pub const MAX_CELL_REFERENCES: usize = 1_000_000;
const MAX_OBJECTS_PER_CELL: usize = 16_384;
#[derive(Clone, Debug, Default)]
pub struct RenderIndex {
    pub grid: [f32; 4], // local-world origin and inverse cell size
    pub cols: u32,
    pub rows: u32,
    pub data: Vec<u32>, // offsets (absolute), then ordered object indices
    pub max_candidates: usize,
}
fn limit(resource: &str, limit: usize, actual: usize) -> String {
    format!("RESOURCE_LIMIT: resource={resource} limit={limit} actual={actual}")
}
impl RenderIndex {
    pub fn build(objects: &[Object], selected: &[u32], delta: [f32; 2]) -> Result<Self, String> {
        Self::bounded(objects, selected, delta, MAX_CELL_REFERENCES)
    }
    fn bounded(
        objects: &[Object],
        selected: &[u32],
        delta: [f32; 2],
        budget: usize,
    ) -> Result<Self, String> {
        if objects.len() > 200_000 {
            return Err(limit("render_objects", 200_000, objects.len()));
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
        let cells = bounds.len().clamp(1, MAX_GRID_CELLS);
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
    fn range(&self, b: [f32; 4]) -> [usize; 4] {
        // One-cell halo protects CPU/GPU f32 rounding at grid boundaries.
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
        [
            cell(b[0], 0).saturating_sub(1),
            cell(b[1], 1).saturating_sub(1),
            (cell(b[2], 0) + 1).min(self.cols as usize - 1),
            (cell(b[3], 1) + 1).min(self.rows as usize - 1),
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
        assert_eq!(r.cols * r.rows, 1);
        assert_eq!(&r.data[2..], &(0..1000).collect::<Vec<_>>());
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
}
