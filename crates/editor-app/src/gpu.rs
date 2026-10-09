use crate::{
    camera::Camera,
    display::{Object, Primitive, Scene},
};
use eframe::egui;
use egui_wgpu::wgpu::{self, util::DeviceExt};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub view: [f32; 4],
    pub camera: [f32; 4],
    pub counts: [u32; 4],
    pub preview: [f32; 4],
    pub grid: [f32; 4],
    pub world: [f32; 4],
    pub selection_bounds: [f32; 4],
}
pub fn uniforms(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[u32],
) -> Result<Uniforms, String> {
    uniforms_preview(
        scene,
        camera,
        rect,
        ppp,
        selected,
        editor_core::MmPoint::new(0., 0.),
    )
}
pub fn uniforms_preview(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[u32],
    delta: editor_core::MmPoint,
) -> Result<Uniforms, String> {
    prepare(scene, camera, rect, ppp, selected, delta).map(|v| v.0)
}
pub fn prepare(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[u32],
    delta: editor_core::MmPoint,
) -> Result<(Uniforms, Arc<crate::render_index::RenderIndex>), String> {
    prepare_measured(scene, camera, rect, ppp, selected, delta).map(|p| (p.uniforms, p.index))
}
#[derive(Default, Clone, Debug)]
pub struct PrepareStats {
    pub candidate_count: usize,
    pub object_visits: usize,
    pub cell_references_visited: usize,
    pub max_candidates_in_view: usize,
    pub preview_index_ms: f64,
    pub cpu_prepare_ms: f64,
    pub estimated_work: f64,
}
pub struct Prepared {
    pub uniforms: Uniforms,
    pub index: Arc<crate::render_index::RenderIndex>,
    pub stats: PrepareStats,
}
/// One immutable-view entry. Delta-dependent queries and validation are never
/// memoized: only the original base query and unselected binned costs are reused.
#[derive(Default)]
pub(crate) struct PrepareWorkCache {
    entry: Option<PrepareWorkEntry>,
}
struct PrepareWorkEntry {
    // Strong identities prevent address reuse and fence Arc::make_mut edits.
    scene: Arc<Scene>,
    selected: Arc<Vec<u32>>,
    camera_bits: [u64; 3],
    viewport_bits: [u32; 5],
    work: StationaryWork,
}
#[derive(Default)]
struct StationaryWork {
    base: Option<(crate::render_index::ViewportRenderSet, f64)>,
    // Bounds/style belong to the object, even when primitive ranges overlap.
    binned: std::collections::HashMap<(usize, usize), Option<f64>>,
}
// This is a memoization budget, never a rendering/admission limit. Above it,
// original computation continues; existing entries remain usable.
const MAX_STATIONARY_BINNED_COSTS: usize = 65_536;
struct WorkContext<'a> {
    started: std::time::Instant,
    cache: Option<&'a mut StationaryWork>,
}
impl PrepareWorkCache {
    pub(crate) fn clear(&mut self) {
        self.entry = None;
    }
    pub(crate) fn invalidate_if_scene_changed(&mut self, scene: Option<&Arc<Scene>>) {
        if self
            .entry
            .as_ref()
            .is_some_and(|entry| scene.is_none_or(|scene| !Arc::ptr_eq(&entry.scene, scene)))
        {
            self.clear();
        }
    }
    pub(crate) fn prepare_measured(
        &mut self,
        scene: &Arc<Scene>,
        camera: Camera,
        rect: egui::Rect,
        ppp: f32,
        selected: &Arc<Vec<u32>>,
        delta: editor_core::MmPoint,
    ) -> Result<Prepared, String> {
        let started = std::time::Instant::now();
        let camera_bits = [
            camera.center.x_mm.to_bits(),
            camera.center.y_mm.to_bits(),
            camera.scale.to_bits(),
        ];
        let viewport_bits = [
            rect.min.x.to_bits(),
            rect.min.y.to_bits(),
            rect.max.x.to_bits(),
            rect.max.y.to_bits(),
            ppp.to_bits(),
        ];
        if self.entry.as_ref().is_none_or(|entry| {
            !Arc::ptr_eq(&entry.scene, scene)
                || !Arc::ptr_eq(&entry.selected, selected)
                || entry.camera_bits != camera_bits
                || entry.viewport_bits != viewport_bits
        }) {
            self.entry = Some(PrepareWorkEntry {
                scene: scene.clone(),
                selected: selected.clone(),
                camera_bits,
                viewport_bits,
                work: StationaryWork::default(),
            });
        }
        // Delta is deliberately absent from the stationary key. Its scalar
        // checks and every selected-object/shifted/halo calculation run below.
        prepare_measured_impl(
            scene,
            camera,
            rect,
            ppp,
            selected,
            delta,
            WorkContext {
                started,
                cache: Some(&mut self.entry.as_mut().unwrap().work),
            },
        )
    }
}
#[cfg(test)]
thread_local! {
    static PREPARE_BASE_QUERIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static BINNED_WORK_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// Conservative per-row work for a binned polygon. The shader only visits
/// edges in the current Y bin. Charge each physical row the densest bin that
/// row can reach, including one adjacent row and bin for sample phase and
/// rounding. This avoids charging every row the densest bin in the Region.
fn binned_polygon_work(
    scene: &Scene,
    primitive: &Primitive,
    visible_x: [f64; 2],
    visible_y: [f64; 2],
    ppm: f64,
    shifted_x: f64,
    shifted_y: f64,
) -> Option<f64> {
    #[cfg(test)]
    BINNED_WORK_CALLS.with(|calls| calls.set(calls.get() + 1));
    let bins = primitive.b[0] as usize;
    let inverse = f64::from(primitive.a[2]);
    if bins == 0 || !inverse.is_finite() || inverse <= 0. || !ppm.is_finite() || ppm <= 0. {
        return None;
    }
    let split = f64::from(primitive.b[2]) + shifted_x;
    let side = if visible_x[1] < split {
        Some(true)
    } else if visible_x[0] >= split {
        Some(false)
    } else {
        None
    };
    let headers = if side == Some(true) {
        primitive.b[3] as usize
    } else {
        primitive.meta[2] as usize + primitive.meta[3] as usize
    };
    let counts = scene.points.get(headers..headers.checked_add(bins)?)?;
    let origin = f64::from(primitive.a[1]) + shifted_y;
    let rows = ((visible_y[1] - visible_y[0]).max(0.) * ppm).ceil() as usize;
    if rows > 16_384 {
        return None;
    }
    let mut sum = 0.;
    let mut max_count = 0f64;
    let mut prefix_cache = vec![None; bins];
    let bin_at = |y: f64| (((y - origin) * inverse).floor().max(0.) as usize).min(bins - 1);
    for row in 0..rows {
        let lo = visible_y[0] + (row as f64 - 1.) / ppm;
        let hi = (visible_y[0] + (row + 2) as f64 / ppm).min(visible_y[1] + 1. / ppm);
        let first = bin_at(lo).saturating_sub(1);
        let last = (bin_at(hi) + 1).min(bins - 1);
        let mut count = 0f64;
        for bin in first..=last {
            let active = if let Some(active) = prefix_cache[bin] {
                active
            } else {
                let header = counts[bin];
                let edge_count = header[1] as usize;
                let active = if let Some(left) = side {
                    let start = header[0] as usize;
                    let end = start.checked_add(edge_count.checked_mul(2)?)?;
                    let edges = scene.points.get(start..end)?;
                    let cutoff = if left {
                        visible_x[1] - shifted_x
                    } else {
                        visible_x[0] - shifted_x
                    };
                    edges
                        .chunks_exact(2)
                        .take_while(|edge| {
                            if left {
                                f64::from(edge[0][0].min(edge[1][0])) <= cutoff
                            } else {
                                f64::from(edge[0][0].max(edge[1][0])) >= cutoff
                            }
                        })
                        .count() as f64
                        + 1.
                } else {
                    // The shader chooses its ray per sample, then stops at
                    // the first edge past that sample's X. Integrate a
                    // conservative pixel-column count for each edge; charging
                    // the whole row the densest prefix overcounts broad copper.
                    let width = (visible_x[1] - visible_x[0]) * ppm;
                    if width <= 0. {
                        return None;
                    }
                    let mut columns = width.ceil() + 1.; // failing edge / break
                    for left in [true, false] {
                        let header_index = if left {
                            primitive.b[3] as usize + bin
                        } else {
                            primitive.meta[2] as usize + primitive.meta[3] as usize + bin
                        };
                        let h = *scene.points.get(header_index)?;
                        let edges = scene.points.get(
                            h[0] as usize
                                ..(h[0] as usize).checked_add((h[1] as usize).checked_mul(2)?)?,
                        )?;
                        let lo = visible_x[0].max(if left { f64::NEG_INFINITY } else { split });
                        let hi = visible_x[1].min(if left { split } else { f64::INFINITY });
                        if hi < lo {
                            continue;
                        }
                        for edge in edges.chunks_exact(2) {
                            let threshold = f64::from(if left {
                                edge[0][0].min(edge[1][0])
                            } else {
                                edge[0][0].max(edge[1][0])
                            }) + shifted_x;
                            let span = if left {
                                hi - lo.max(threshold)
                            } else {
                                hi.min(threshold) - lo
                            };
                            if span >= 0. {
                                columns += (span * ppm).ceil() + 1.;
                            }
                        }
                    }
                    columns / width
                };
                prefix_cache[bin] = Some(active);
                active
            };
            count = count.max(active);
        }
        sum += count;
        max_count = max_count.max(count);
    }
    Some(sum + 4. * max_count)
}
pub fn viewport_bounds(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
) -> Result<[f64; 4], String> {
    let ppm = camera.scale * f64::from(ppp);
    if !ppm.is_finite() || ppm <= 0. || !rect.is_positive() {
        return Err("VALIDATION_FAILED: viewport".into());
    }
    // 1.5px selection edge + 0.25px AA + rounding allowance. Query adds a cell halo too.
    let margin = 2. / ppm;
    let half_x = f64::from(rect.width()) / camera.scale / 2. + margin;
    let half_y = f64::from(rect.height()) / camera.scale / 2. + margin;
    let cx = camera.center.x_mm - scene.anchor.x_mm;
    let cy = camera.center.y_mm - scene.anchor.y_mm;
    let bounds = [cx - half_x, cy - half_y, cx + half_x, cy + half_y];
    if bounds.iter().any(|v| !v.is_finite()) {
        return Err("VALIDATION_FAILED: viewport bounds".into());
    }
    Ok(bounds)
}
pub fn prepare_measured(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[u32],
    delta: editor_core::MmPoint,
) -> Result<Prepared, String> {
    prepare_measured_impl(
        scene,
        camera,
        rect,
        ppp,
        selected,
        delta,
        WorkContext {
            started: std::time::Instant::now(),
            cache: None,
        },
    )
}
fn prepare_measured_impl(
    scene: &Scene,
    camera: Camera,
    rect: egui::Rect,
    ppp: f32,
    selected: &[u32],
    delta: editor_core::MmPoint,
    context: WorkContext<'_>,
) -> Result<Prepared, String> {
    let WorkContext { started, mut cache } = context;
    if selected.len() != scene.objects.len() {
        return Err("VALIDATION_FAILED: selection flags length".into());
    }
    let bounds = viewport_bounds(scene, camera, rect, ppp)?;
    let preview = [scene.scalar(delta.x_mm)?, scene.scalar(delta.y_mm)?, 0., 0.];
    let selected_flags = selected;
    let mut stats = PrepareStats::default();
    let width = rect.width() * ppp;
    let height = rect.height() * ppp;
    let ppm = camera.scale * f64::from(ppp);
    let cx = camera.center.x_mm - scene.anchor.x_mm;
    let cy = camera.center.y_mm - scene.anchor.y_mm;
    // Immutable bins serve both stationary and translated objects. Query the
    // original coordinates, then merge by scene ID to preserve exposure order.
    let index = scene.index.clone();
    let base_query = || {
        #[cfg(test)]
        PREPARE_BASE_QUERIES.with(|calls| calls.set(calls.get() + 1));
        let viewport = index.viewport(bounds);
        let work = index.sample_candidate_work(&viewport, ppm);
        (viewport, work)
    };
    // Cache the full original set before filtering; shifted work still uses
    // the full grid counts, and exposure order/diagnostic counts stay intact.
    let (mut viewport, mut work) = if let Some(cache) = cache.as_deref_mut() {
        cache.base.get_or_insert_with(base_query).clone()
    } else {
        base_query()
    };
    stats.max_candidates_in_view = viewport.max_candidates_in_view;
    stats.cell_references_visited = viewport.cell_references_visited;
    if preview[0] != 0. || preview[1] != 0. {
        viewport
            .ordered_candidate_ids
            .retain(|id| selected[*id as usize] == 0);
        let shifted = index.viewport(std::array::from_fn(|k| {
            bounds[k] - f64::from(preview[k % 2])
        }));
        work += index.sample_candidate_work(&shifted, ppm);
        stats.max_candidates_in_view += shifted.max_candidates_in_view;
        stats.cell_references_visited += shifted.cell_references_visited;
        viewport.ordered_candidate_ids.extend(
            shifted
                .ordered_candidate_ids
                .into_iter()
                .filter(|id| selected[*id as usize] != 0),
        );
        viewport.ordered_candidate_ids.sort_unstable();
        // Streams are disjoint, but retain set semantics for future index queries.
        viewport.ordered_candidate_ids.dedup();
    }
    stats.candidate_count = viewport.ordered_candidate_ids.len();
    let mut selection_bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for (object, flag) in scene.objects.iter().zip(selected_flags) {
        if *flag == 0 || object.meta[3] == 0 {
            continue;
        }
        for k in 0..2 {
            selection_bounds[k] = selection_bounds[k].min(object.bounds[k] + preview[k]);
            selection_bounds[k + 2] =
                selection_bounds[k + 2].max(object.bounds[k + 2] + preview[k]);
        }
    }
    let has_selection = selection_bounds[0].is_finite();
    if has_selection {
        // Halo evaluation is restricted to selected envelopes. Query probes
        // need an additional halo beyond the shader's pixel early-out box.
        let selected_bounds = std::array::from_fn(|k| {
            let v = f64::from(selection_bounds[k]) + if k < 2 { -4. / ppm } else { 4. / ppm };
            (if k < 2 {
                v.max(bounds[k])
            } else {
                v.min(bounds[k])
            }) - f64::from(preview[k % 2])
        });
        work += index.sample_candidate_work_in_bounds(selected_bounds, ppm);
        for (k, v) in selection_bounds.iter_mut().enumerate() {
            *v = if k < 2 {
                (*v - (2. / ppm) as f32).next_down()
            } else {
                (*v + (2. / ppm) as f32).next_up()
            };
        }
    } else {
        selection_bounds = [0.; 4];
    }
    for id in &viewport.ordered_candidate_ids {
        let index = *id as usize;
        let object = &scene.objects[index];
        stats.object_visits += 1;
        if object.meta[3] == 0 {
            continue;
        }
        let mut b = object.bounds;
        if selected_flags[index] != 0 {
            for i in 0..4 {
                b[i] += preview[i % 2];
            }
            for v in b {
                scene.scalar(f64::from(v))?;
            }
        }
        if (0..2).any(|k| {
            f64::from(b[k + 2]) < viewport.world_bounds[k]
                || f64::from(b[k]) > viewport.world_bounds[k + 2]
        }) {
            continue;
        }
        let left =
            ((f64::from(b[0]) - cx) * ppm + f64::from(width) / 2.).clamp(0., f64::from(width));
        let right =
            ((f64::from(b[2]) - cx) * ppm + f64::from(width) / 2.).clamp(0., f64::from(width));
        let bottom =
            ((f64::from(b[1]) - cy) * ppm + f64::from(height) / 2.).clamp(0., f64::from(height));
        let top =
            ((f64::from(b[3]) - cy) * ppm + f64::from(height) / 2.).clamp(0., f64::from(height));
        // Filled objects only need the quarter-pixel AA probes. A two-pixel
        // halo is needed for selected edges and outline/centre-line display;
        // charging it to every filled Region overstates the zoomed view.
        let sample_margin_px = if selected_flags[index] != 0 || object.style[1] != 0 {
            2.
        } else {
            0.5
        };
        let horizontal = (right - left).max(0.) + sample_margin_px * 2.;
        let vertical = (top - bottom).max(0.) + sample_margin_px * 2.;
        let visible_y = [
            f64::from(b[1]).max(viewport.world_bounds[1]) - sample_margin_px / ppm,
            f64::from(b[3]).min(viewport.world_bounds[3]) + sample_margin_px / ppm,
        ];
        let visible_x = [
            f64::from(b[0]).max(viewport.world_bounds[0]) - sample_margin_px / ppm,
            f64::from(b[2]).min(viewport.world_bounds[2]) + sample_margin_px / ppm,
        ];
        let shifted_x = if selected_flags[index] != 0 {
            f64::from(preview[0])
        } else {
            0.
        };
        let shifted_y = if selected_flags[index] != 0 {
            f64::from(preview[1])
        } else {
            0.
        };
        for (offset, primitive) in scene.primitives
            [object.meta[0] as usize..object.meta[1] as usize]
            .iter()
            .enumerate()
        {
            let polygon = matches!(primitive.meta[0], 1 | 3);
            let px = if polygon {
                [
                    visible_x[0]
                        .max(f64::from(primitive.bounds[0]) + shifted_x - sample_margin_px / ppm),
                    visible_x[1]
                        .min(f64::from(primitive.bounds[2]) + shifted_x + sample_margin_px / ppm),
                ]
            } else {
                visible_x
            };
            let py = if polygon {
                [
                    visible_y[0]
                        .max(f64::from(primitive.bounds[1]) + shifted_y - sample_margin_px / ppm),
                    visible_y[1]
                        .min(f64::from(primitive.bounds[3]) + shifted_y + sample_margin_px / ppm),
                ]
            } else {
                visible_y
            };
            let columns = if polygon {
                ((px[1] - px[0]).max(0.) * ppm).ceil() + 1.
            } else {
                horizontal
            };
            let rows = ((py[1] - py[0]).max(0.) * ppm).ceil() + 1.;
            let cost = if px[1] < px[0] || py[1] < py[0] {
                0.
            } else if primitive.meta[0] == 1 {
                rows * f64::from(primitive.meta[3])
            } else if primitive.meta[0] == 3 {
                let calculate =
                    || binned_polygon_work(scene, primitive, px, py, ppm, shifted_x, shifted_y);
                let cost = if selected_flags[index] == 0 {
                    if let Some(cache) = cache.as_deref_mut() {
                        let key = (index, object.meta[0] as usize + offset);
                        if let Some(cost) = cache.binned.get(&key) {
                            *cost
                        } else {
                            let cost = calculate();
                            if cache.binned.len() < MAX_STATIONARY_BINNED_COSTS {
                                cache.binned.insert(key, cost);
                            }
                            cost
                        }
                    } else {
                        calculate()
                    }
                } else {
                    calculate()
                };
                // Cached None preserves the original conservative fallback.
                cost.unwrap_or(rows * f64::from(primitive.b[1]))
            } else {
                vertical
            };
            // Four AA material samples. Outline calls material four times
            // per AA sample; only selected objects add four halo samples.
            let samples = if object.style[1] == crate::display::MODE_EDGE {
                16.
            } else {
                4.
            } + if selected_flags[index] != 0 { 4. } else { 0. };
            // Polygon bounds checks still execute throughout the object's box;
            // only the edge scan is restricted to the individual contour.
            work += (columns * cost + if polygon { horizontal * vertical } else { 0. }) * samples;
        }
    }
    // Work is diagnostic, not an admission limit: large workspaces may render slowly.
    if !work.is_finite() {
        return Err("VALIDATION_FAILED: non-finite display work estimate".into());
    }
    stats.estimated_work = work;
    stats.cpu_prepare_ms = started.elapsed().as_secs_f64() * 1000.;
    Ok(Prepared {
        stats,
        uniforms: Uniforms {
            world: index.world,
            grid: index.grid,
            preview,
            view: [rect.left() * ppp, rect.top() * ppp, width, height],
            camera: [
                scene.scalar(camera.center.x_mm - scene.anchor.x_mm)?,
                scene.scalar(camera.center.y_mm - scene.anchor.y_mm)?,
                (camera.scale * f64::from(ppp)) as f32,
                // Selection-halo probe distance: about 1.5 device px, rounded once on the CPU.
                // The extra 1.23 % keeps probes off exact pixel-centre ties: geometry aligned
                // to half-pixel positions would otherwise land on a shape boundary, where the
                // in/out answer depends on last-bit rounding of the shader compiler.
                (1.5 * 1.0123 / (camera.scale * f64::from(ppp))) as f32,
            ],
            counts: [
                scene.objects.len() as u32,
                index.cols,
                index.rows,
                u32::from(has_selection),
            ],
            selection_bounds,
        },
        index,
    })
}
pub fn selection_flags(scene: &Scene, selected: &[&str]) -> Vec<u32> {
    let ids: std::collections::HashSet<_> = selected.iter().copied().collect();
    scene
        .ids
        .iter()
        .map(|id| u32::from(ids.contains(id.as_str())))
        .collect()
}
pub struct Resources {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub selected: wgpu::Buffer,
    selection_identity: Option<Arc<Vec<u32>>>,
    pub bind: wgpu::BindGroup,
    pub serial: u64,
    pub index: Arc<crate::render_index::RenderIndex>,
    pub bins: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    objects: wgpu::Buffer,
    shapes: wgpu::Buffer,
    points: wgpu::Buffer,
}
impl Resources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, scene: &Scene) -> Self {
        #[cfg(feature = "internal-evidence")]
        let _span =
            crate::native_pmix::spans::enter(crate::native_pmix::spans::Stage::GpuResourcesNew);
        let storage = |name, bytes: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(name),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let dummy_o = [Object::default()];
        let dummy_p = [Primitive::default()];
        let dummy_v = [[0f32; 2]];
        let objects = storage(
            "semantic-objects",
            bytemuck::cast_slice(if scene.objects.is_empty() {
                &dummy_o
            } else {
                &scene.objects
            }),
        );
        let shapes = storage(
            "local-material",
            bytemuck::cast_slice(if scene.primitives.is_empty() {
                &dummy_p
            } else {
                &scene.primitives
            }),
        );
        let points = storage(
            "display-contours",
            bytemuck::cast_slice(if scene.points.is_empty() {
                &dummy_v
            } else {
                &scene.points
            }),
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let selected = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("selected-object-flags"),
            size: (scene.objects.len().max(1) * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bins = storage("world-bins", bytemuck::cast_slice(&scene.index.data));
        let entries: Vec<_> = (0..6)
            .map(|i| wgpu::BindGroupLayoutEntry {
                binding: i,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if i == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("editor-display"),
            entries: &entries,
        });
        let entries: Vec<_> = [&uniform, &objects, &shapes, &points, &selected, &bins]
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor-display"),
            layout: &layout,
            entries: &entries,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("editor-display"),
            source: wgpu::ShaderSource::Wgsl(include_str!("editor.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("editor-display"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let resources = Self {
            pipeline,
            uniform,
            selected,
            selection_identity: None,
            bind,
            serial: scene.serial,
            index: scene.index.clone(),
            bins,
            layout,
            objects,
            shapes,
            points,
        };
        #[cfg(feature = "internal-evidence")]
        {
            crate::native_s5m1::gpu_event(
                "geometry-storage-init-upload",
                [
                    &resources.objects,
                    &resources.shapes,
                    &resources.points,
                    &resources.bins,
                ]
                .iter()
                .map(|b| b.size())
                .sum(),
            );
            crate::native_s5m1::gpu_device_allocation(device);
        }
        resources
    }
}
pub struct Callback {
    pub trace: Option<crate::frame_trace::CallbackBinding>,
    pub painted: Option<(Arc<std::sync::atomic::AtomicU64>, u64)>,
    pub scene: Arc<Scene>,
    pub index: Arc<crate::render_index::RenderIndex>,
    pub uniforms: Uniforms,
    pub selected: Arc<Vec<u32>>,
    pub format: wgpu::TextureFormat,
}
impl egui_wgpu::CallbackTrait for Callback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &egui_wgpu::ScreenDescriptor,
        _: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let _trace = self
            .trace
            .as_ref()
            .map(|trace| trace.span(crate::frame_trace::Stage::CallbackPrepare));
        #[cfg(feature = "internal-evidence")]
        let _span = crate::native_pmix::spans::enter_frame(
            crate::native_pmix::spans::Stage::GpuCallbackPrepare,
            self.painted.as_ref().map_or(0, |p| p.1),
        );
        if resources
            .get::<Resources>()
            .is_none_or(|r| r.serial != self.scene.serial)
        {
            resources.insert(Resources::new(device, self.format, &self.scene));
            #[cfg(feature = "internal-evidence")]
            crate::native_s5m1::gpu_event("scene-allocation", 1);
        }
        if let Some(r) = resources.get_mut::<Resources>() {
            if !Arc::ptr_eq(&r.index, &self.index) {
                #[cfg(feature = "internal-evidence")]
                crate::native_s5m1::gpu_event("index-allocation", 1);
                r.bins = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview-world-bins"),
                    contents: bytemuck::cast_slice(&self.index.data),
                    usage: wgpu::BufferUsages::STORAGE,
                });
                let entries: Vec<_> = [
                    &r.uniform,
                    &r.objects,
                    &r.shapes,
                    &r.points,
                    &r.selected,
                    &r.bins,
                ]
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect();
                r.bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("preview-display"),
                    layout: &r.layout,
                    entries: &entries,
                });
                #[cfg(feature = "internal-evidence")]
                crate::native_s5m1::gpu_event("index-storage-init-upload", r.bins.size());
                r.index = self.index.clone();
            }
            queue.write_buffer(&r.uniform, 0, bytemuck::bytes_of(&self.uniforms));
            #[cfg(feature = "internal-evidence")]
            crate::native_s5m1::gpu_event("uniform-upload", std::mem::size_of::<Uniforms>() as u64);
            if !self.selected.is_empty()
                && r.selection_identity
                    .as_ref()
                    .is_none_or(|v| !Arc::ptr_eq(v, &self.selected))
            {
                queue.write_buffer(&r.selected, 0, bytemuck::cast_slice(&self.selected));
                #[cfg(feature = "internal-evidence")]
                crate::native_s5m1::gpu_event("selection-upload", (self.selected.len() * 4) as u64);
                r.selection_identity = Some(self.selected.clone());
            }
            #[cfg(feature = "internal-evidence")]
            crate::native_s5m1::gpu_bytes(
                [
                    &r.uniform,
                    &r.objects,
                    &r.shapes,
                    &r.points,
                    &r.selected,
                    &r.bins,
                ]
                .iter()
                .map(|b| b.size())
                .sum(),
            );
        }
        #[cfg(feature = "internal-evidence")]
        crate::native_s5m1::gpu_device_allocation(device);
        vec![]
    }
    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let mut trace_paint = self
            .trace
            .as_ref()
            .map(|trace| trace.span(crate::frame_trace::Stage::CallbackPaint));
        if let Some(span) = &mut trace_paint {
            span.outcome = "clipped_or_no_resources";
        }
        #[cfg(feature = "internal-evidence")]
        let _span = crate::native_pmix::spans::enter_frame(
            crate::native_pmix::spans::Stage::GpuCallbackPaint,
            self.painted.as_ref().map_or(0, |p| p.1),
        );
        #[cfg(feature = "internal-evidence")]
        crate::native_ui::callback(&info);
        let v = info.viewport_in_pixels();
        let c = info.clip_rect_in_pixels();
        let x = v.left_px.max(c.left_px).max(0) as u32;
        let y = v.top_px.max(c.top_px).max(0) as u32;
        let w = (v.left_px + v.width_px).min(c.left_px + c.width_px).max(0) as u32;
        let h = (v.top_px + v.height_px).min(c.top_px + c.height_px).max(0) as u32;
        if w <= x || h <= y {
            return;
        }
        pass.set_scissor_rect(x, y, w - x, h - y);
        if let Some(r) = resources.get::<Resources>() {
            pass.set_pipeline(&r.pipeline);
            pass.set_bind_group(0, &r.bind, &[]);
            pass.draw(0..3, 0..1);
            if let Some(span) = &mut trace_paint {
                span.outcome = "draw_encoded";
            }
            #[cfg(feature = "internal-evidence")]
            crate::native_s5m1::gpu_event("draw", 1);
            if let Some((stamp, id)) = &self.painted {
                stamp.store(*id, std::sync::atomic::Ordering::Release);
                #[cfg(feature = "internal-evidence")]
                crate::native_i1::paint(*id);
            }
        }
    }
}

#[cfg(test)]
mod binned_work_tests {
    use super::*;

    #[test]
    fn split_ray_work_bounds_all_horizontal_sample_phases_and_translation() {
        let edges: Vec<_> = (0..20)
            .map(|i| {
                let x = -1.5 + i as f32 * 0.15;
                ([x, 0.], [x + 0.3, 1.])
            })
            .collect();
        let mut points = vec![[0.; 2]; 2];
        let mut right = edges.clone();
        right.reverse();
        for (header, list) in [right, edges].into_iter().enumerate() {
            points[header] = [points.len() as f32, list.len() as f32];
            for (a, b) in list {
                points.push(a);
                points.push(b);
            }
        }
        let scene = Scene {
            serial: 0,
            index: Arc::default(),
            anchor: editor_core::MmPoint::new(0., 0.),
            objects: vec![],
            primitives: vec![],
            points,
            ids: vec![],
            ppm: 20.,
        };
        let primitive = Primitive {
            meta: [3, 1, 0, 0],
            a: [1., 0., 1., 0.],
            b: [1., 20., 0., 1.],
            bounds: [0.; 4],
        };
        for shift in [0., 10.] {
            let estimate = binned_polygon_work(
                &scene,
                &primitive,
                [-1. + shift, 1. + shift],
                [0., 1.],
                20.,
                shift,
                0.,
            )
            .unwrap()
                * 40.;
            for phase in 0..40 {
                let mut visits = 0;
                for column in 0..40 {
                    let x = -1. + (column as f64 + phase as f64 / 40.) / 20.;
                    let left = x < 0.;
                    let h = scene.points[usize::from(left)];
                    for i in 0..h[1] as usize {
                        visits += 20;
                        let a = scene.points[h[0] as usize + i * 2];
                        let b = scene.points[h[0] as usize + i * 2 + 1];
                        if (left && f64::from(a[0].min(b[0])) > x)
                            || (!left && f64::from(a[0].max(b[0])) < x)
                        {
                            break;
                        }
                    }
                }
                assert!(
                    estimate >= f64::from(visits),
                    "phase={phase} shift={shift} estimated={estimate} visits={visits}"
                );
            }
        }
    }

    #[test]
    fn subpixel_bins_cover_every_sample_phase() {
        let counts: Vec<u32> = (0..100)
            .map(|bin| if bin % 13 == 0 { 50 } else { 1 })
            .collect();
        let mut points: Vec<_> = counts.iter().map(|&count| [0., count as f32]).collect();
        for (bin, &count) in counts.iter().enumerate() {
            points[bin][0] = points.len() as f32;
            for _ in 0..count {
                points.push([-2., 0.]);
                points.push([2., 1.]);
            }
        }
        let scene = Scene {
            serial: 0,
            index: Arc::default(),
            anchor: editor_core::MmPoint::new(0., 0.),
            objects: Vec::new(),
            primitives: Vec::new(),
            points,
            ids: Vec::new(),
            ppm: 10.,
        };
        let primitive = Primitive {
            meta: [3, 1, 0, 0],
            a: [1., 0., 100., 0.],
            b: [100., 50., 0., 0.],
            bounds: [0.; 4],
        };
        let estimate =
            binned_polygon_work(&scene, &primitive, [-1., 1.], [0., 1.], 10., 0., 0.).unwrap();
        for phase in 0..40 {
            let actual: u32 = (0..10)
                .map(|row| {
                    let y = (row as f64 + phase as f64 / 40.) / 10.;
                    counts[(y * 100.).floor().min(99.) as usize]
                })
                .sum();
            assert!(estimate >= f64::from(actual), "phase={phase}");
        }
    }
}

#[cfg(test)]
mod prepare_work_cache_tests {
    use super::*;
    use editor_core::MmPoint;

    fn scene() -> Arc<Scene> {
        let edges: Vec<_> = (0..20)
            .map(|i| {
                let x = -1.5 + i as f32 * 0.15;
                ([x, -1.], [x + 0.3, 1.])
            })
            .collect();
        let mut points = vec![[0.; 2]; 2];
        let mut right = edges.clone();
        right.reverse();
        for (header, list) in [right, edges].into_iter().enumerate() {
            points[header] = [points.len() as f32, list.len() as f32];
            for (a, b) in list {
                points.push(a);
                points.push(b);
            }
        }
        // Shared primitive ranges intentionally need distinct object keys:
        // clipping bounds and styles differ. Include Clear/layer metadata.
        let objects = vec![
            Object {
                meta: [0, 3, 0, 1],
                bounds: [-2., -1., -0.25, 1.],
                style: [0, 0, 0, 0],
            },
            Object {
                meta: [0, 3, 1, 1],
                bounds: [0.25, -0.5, 1., 0.5],
                style: [0, crate::display::MODE_EDGE, 0, 0],
            },
            Object {
                meta: [0, 3, 0, 1],
                bounds: [-1., -0.5, 1., 0.5],
                style: [0, 0, 0, 0],
            },
            Object {
                meta: [0, 3, 1, 0],
                bounds: [-2., -1., 2., 1.],
                style: [0, 0, 0, 0],
            },
        ];
        Arc::new(Scene {
            serial: 7,
            index: Arc::new(
                crate::render_index::RenderIndex::build(&objects, &[], [0.; 2]).unwrap(),
            ),
            anchor: MmPoint::new(0., 0.),
            objects,
            primitives: vec![
                Primitive {
                    meta: [0, 0, 0, 0],
                    bounds: [-2., -1., 2., 1.],
                    ..Default::default()
                },
                Primitive {
                    meta: [1, 0, 0, 20],
                    bounds: [-2., -1., 2., 1.],
                    ..Default::default()
                },
                Primitive {
                    meta: [3, 1, 0, 0],
                    a: [1., -1., 0.5, 0.],
                    b: [1., 20., 0., 1.],
                    bounds: [-2., -1., 2., 1.],
                },
            ],
            points,
            ids: vec![],
            ppm: 20.,
        })
    }
    fn camera() -> Camera {
        Camera {
            center: MmPoint::new(0., 0.),
            scale: 10.,
        }
    }
    fn rect() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(10., 20.), egui::pos2(90., 80.))
    }
    fn flags() -> Arc<Vec<u32>> {
        Arc::new(vec![0, 0, 1, 1])
    }
    fn same(a: Result<Prepared, String>, b: Result<Prepared, String>) {
        match (a, b) {
            (Ok(a), Ok(b)) => {
                assert_eq!(
                    bytemuck::bytes_of(&a.uniforms),
                    bytemuck::bytes_of(&b.uniforms)
                );
                assert!(Arc::ptr_eq(&a.index, &b.index));
                let counts = |s: &PrepareStats| {
                    (
                        s.candidate_count,
                        s.object_visits,
                        s.cell_references_visited,
                        s.max_candidates_in_view,
                        s.preview_index_ms.to_bits(),
                        s.estimated_work.to_bits(),
                    )
                };
                assert_eq!(counts(&a.stats), counts(&b.stats));
            }
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("cached/reference success differs"),
        }
    }
    fn equivalent(
        cache: &mut PrepareWorkCache,
        scene: &Arc<Scene>,
        camera: Camera,
        rect: egui::Rect,
        ppp: f32,
        flags: &Arc<Vec<u32>>,
        delta: MmPoint,
    ) {
        same(
            cache.prepare_measured(scene, camera, rect, ppp, flags, delta),
            prepare_measured(scene, camera, rect, ppp, flags, delta),
        );
    }
    fn calls() -> usize {
        BINNED_WORK_CALLS.with(std::cell::Cell::get)
    }

    #[test]
    fn repeated_views_skip_base_and_unselected_bins_but_recompute_delta() {
        let (scene, flags) = (scene(), flags());
        let mut cache = PrepareWorkCache::default();
        let mut last_preview = None;
        PREPARE_BASE_QUERIES.with(|v| v.set(0));
        BINNED_WORK_CALLS.with(|v| v.set(0));
        for delta in [
            MmPoint::new(0., 0.),
            MmPoint::new(0.125, 0.25),
            MmPoint::new(-0.25, -0.125),
        ] {
            let before = calls();
            let prepared = cache
                .prepare_measured(&scene, camera(), rect(), 2., &flags, delta)
                .unwrap();
            assert_eq!(calls() - before, if last_preview.is_none() { 3 } else { 1 });
            assert_eq!(PREPARE_BASE_QUERIES.with(std::cell::Cell::get), 1);
            assert!(last_preview.is_none_or(|preview| preview != prepared.uniforms.preview));
            last_preview = Some(prepared.uniforms.preview);
        }
        assert_eq!(cache.entry.as_ref().unwrap().work.binned.len(), 2);
        for delta in [
            MmPoint::new(0., 0.),
            MmPoint::new(0.125, 0.25),
            MmPoint::new(100., 100.),
            MmPoint::new(-100., -100.),
            MmPoint::new(4., 0.),
            MmPoint::new(-0., -0.),
        ] {
            equivalent(&mut cache, &scene, camera(), rect(), 2., &flags, delta);
        }
    }

    #[test]
    fn exact_view_bits_scene_selection_and_cow_invalidate() {
        let (mut scene, mut flags) = (scene(), flags());
        let mut cache = PrepareWorkCache::default();
        for field in 0..8 {
            let (mut c, mut r, mut ppp) = (camera(), rect(), 2f32);
            equivalent(&mut cache, &scene, c, r, ppp, &flags, MmPoint::new(0., 0.));
            match field {
                0 => c.center.x_mm = f64::from_bits(1),
                1 => c.center.y_mm = f64::from_bits(1),
                2 => c.scale = c.scale.next_up(),
                3 => r.min.x = r.min.x.next_up(),
                4 => r.min.y = r.min.y.next_up(),
                5 => r.max.x = r.max.x.next_up(),
                6 => r.max.y = r.max.y.next_up(),
                7 => ppp = ppp.next_up(),
                _ => unreachable!(),
            }
            let before = PREPARE_BASE_QUERIES.with(std::cell::Cell::get);
            equivalent(&mut cache, &scene, c, r, ppp, &flags, MmPoint::new(0., 0.));
            assert_eq!(
                PREPARE_BASE_QUERIES.with(std::cell::Cell::get) - before,
                2,
                "cache miss and reference: field {field}"
            );
        }
        for field in 0..2 {
            equivalent(
                &mut cache,
                &scene,
                camera(),
                rect(),
                2.,
                &flags,
                MmPoint::new(0., 0.),
            );
            let before = PREPARE_BASE_QUERIES.with(std::cell::Cell::get);
            if field == 0 {
                scene = Arc::new((*scene).clone());
            } else {
                flags = Arc::new((*flags).clone());
            }
            equivalent(
                &mut cache,
                &scene,
                camera(),
                rect(),
                2.,
                &flags,
                MmPoint::new(0., 0.),
            );
            assert_eq!(PREPARE_BASE_QUERIES.with(std::cell::Cell::get) - before, 2);
        }
        let old_scene = Arc::downgrade(&scene);
        Arc::make_mut(&mut scene).objects[0].style[1] = crate::display::MODE_EDGE;
        assert!(old_scene.upgrade().is_some());
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        assert!(old_scene.upgrade().is_none());
        let old_flags = Arc::downgrade(&flags);
        Arc::make_mut(&mut flags)[0] = 1;
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        assert!(old_flags.upgrade().is_none());
        assert!(
            !cache
                .entry
                .as_ref()
                .unwrap()
                .work
                .binned
                .contains_key(&(0, 2))
        );
        for bits in [
            (-0f64).to_bits(),
            f64::NAN.to_bits(),
            f64::NAN.to_bits() + 1,
        ] {
            let mut c = camera();
            c.center.x_mm = f64::from_bits(bits);
            equivalent(
                &mut cache,
                &scene,
                c,
                rect(),
                2.,
                &flags,
                MmPoint::new(0., 0.),
            );
            assert_eq!(cache.entry.as_ref().unwrap().camera_bits[0], bits);
        }
    }

    #[test]
    fn shared_primitive_ranges_keep_object_clipping_and_addition_order() {
        let (scene, flags) = (scene(), flags());
        let mut cache = PrepareWorkCache::default();
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        let work = &cache.entry.as_ref().unwrap().work;
        assert!(work.binned.contains_key(&(0, 2)) && work.binned.contains_key(&(1, 2)));
        assert_ne!(work.binned[&(0, 2)], work.binned[&(1, 2)]);
        for dx in [-20., -2., 0.125, 2., 20.] {
            for dy in [-20., -2., 0.125, 2., 20.] {
                equivalent(
                    &mut cache,
                    &scene,
                    camera(),
                    rect(),
                    2.,
                    &flags,
                    MmPoint::new(dx, dy),
                );
            }
        }
        for flags in [
            Arc::new(vec![0; 4]),
            Arc::new(vec![1; 4]),
            Arc::new(vec![2, 0, 7, 3]),
        ] {
            for delta in [
                MmPoint::new(0., 0.),
                MmPoint::new(1., -1.),
                MmPoint::new(10., 10.),
            ] {
                equivalent(&mut cache, &scene, camera(), rect(), 2., &flags, delta);
            }
        }
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        let raw_base = &cache.entry.as_ref().unwrap().work.base.as_ref().unwrap().0;
        assert_eq!(
            raw_base.ordered_candidate_ids,
            scene
                .index
                .viewport(viewport_bounds(&scene, camera(), rect(), 2.).unwrap())
                .ordered_candidate_ids
        );
        assert!(
            raw_base.ordered_candidate_ids.contains(&2),
            "cached base must retain selected objects"
        );
    }

    #[test]
    fn cached_none_and_full_budget_fall_back_without_new_errors() {
        let (mut scene, flags) = (scene(), flags());
        Arc::make_mut(&mut scene).primitives[2].b[0] = 0.;
        let mut cache = PrepareWorkCache::default();
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        assert_eq!(cache.entry.as_ref().unwrap().work.binned[&(0, 2)], None);
        let before = calls();
        cache
            .prepare_measured(
                &scene,
                camera(),
                rect(),
                2.,
                &flags,
                MmPoint::new(0.125, 0.),
            )
            .unwrap();
        assert_eq!(
            calls() - before,
            1,
            "cached None must hit; selected None is recalculated"
        );
        let work = &mut cache.entry.as_mut().unwrap().work;
        for i in 0..MAX_STATIONARY_BINNED_COSTS - 2 {
            work.binned.insert((usize::MAX, i), Some(0.));
        }
        let before = calls();
        cache
            .prepare_measured(&scene, camera(), rect(), 2., &flags, MmPoint::new(0., 0.))
            .unwrap();
        assert_eq!(calls() - before, 1, "existing keys hit at full capacity");
        let work = &mut cache.entry.as_mut().unwrap().work;
        work.binned.remove(&(0, 2));
        work.binned
            .insert((usize::MAX, MAX_STATIONARY_BINNED_COSTS), Some(0.));
        let before = calls();
        cache
            .prepare_measured(&scene, camera(), rect(), 2., &flags, MmPoint::new(0., 0.))
            .unwrap();
        assert_eq!(
            calls() - before,
            2,
            "uncached stationary entry uses original calculation at capacity"
        );
        assert_eq!(
            cache.entry.as_ref().unwrap().work.binned.len(),
            MAX_STATIONARY_BINNED_COSTS
        );
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
    }

    #[test]
    fn errors_keep_precedence_on_miss_hit_and_changed_delta() {
        let mut cache = PrepareWorkCache::default();
        for failure in 0..9 {
            let (mut scene, mut flags, mut c, mut r, mut ppp, mut delta) = (
                scene(),
                flags(),
                camera(),
                rect(),
                2f32,
                MmPoint::new(0., 0.),
            );
            let expected = match failure {
                0 => {
                    flags = Arc::new(vec![]);
                    ppp = f32::NAN;
                    delta.x_mm = f64::INFINITY;
                    "VALIDATION_FAILED: selection flags length"
                }
                1 => {
                    ppp = f32::NAN;
                    delta.x_mm = f64::INFINITY;
                    "VALIDATION_FAILED: viewport"
                }
                2 => {
                    c.center.x_mm = f64::INFINITY;
                    delta.x_mm = f64::INFINITY;
                    "VALIDATION_FAILED: viewport bounds"
                }
                3 => {
                    delta.x_mm = f64::INFINITY;
                    "DISPLAY_PRECISION: local coordinate error exceeds 0.1 physical pixel"
                }
                4 => {
                    delta.y_mm = f64::from_bits(1);
                    "DISPLAY_PRECISION: local coordinate error exceeds 0.1 physical pixel"
                }
                5 => {
                    Arc::make_mut(&mut scene).ppm = 1e10;
                    "DISPLAY_PRECISION: local coordinate error exceeds 0.1 physical pixel"
                }
                6 => {
                    Arc::make_mut(&mut scene).primitives[2].b = [0., f32::INFINITY, 0., 1.];
                    "VALIDATION_FAILED: non-finite display work estimate"
                }
                7 => {
                    Arc::make_mut(&mut scene).primitives[2].b = [0., f32::INFINITY, 0., 1.];
                    c.center.x_mm = 1e10;
                    r.max.x = 4e12;
                    "VALIDATION_FAILED: non-finite display work estimate"
                }
                8 => {
                    c.center.x_mm = 1e10;
                    "DISPLAY_PRECISION: local coordinate error exceeds 0.1 physical pixel"
                }
                _ => unreachable!(),
            };
            let original = prepare_measured(&scene, c, r, ppp, &flags, delta);
            assert_eq!(
                original.err().as_deref(),
                Some(expected),
                "failure {failure}"
            );
            for _ in 0..3 {
                equivalent(&mut cache, &scene, c, r, ppp, &flags, delta);
            }
        }
        let (scene, flags) = (scene(), flags());
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        for delta in [
            MmPoint::new(f64::INFINITY, 0.),
            MmPoint::new(0., f64::NAN),
            MmPoint::new(0.125, 0.25),
        ] {
            equivalent(&mut cache, &scene, camera(), rect(), 2., &flags, delta);
        }
    }

    #[test]
    fn none_replacement_and_clear_release_owned_inputs() {
        let (scene, flags) = (scene(), flags());
        let mut cache = PrepareWorkCache::default();
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        let (weak_scene, weak_flags) = (Arc::downgrade(&scene), Arc::downgrade(&flags));
        drop(scene);
        drop(flags);
        assert!(weak_scene.upgrade().is_some() && weak_flags.upgrade().is_some());
        cache.invalidate_if_scene_changed(None);
        assert!(weak_scene.upgrade().is_none() && weak_flags.upgrade().is_none());
        let (scene, flags) = (self::scene(), self::flags());
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        cache.invalidate_if_scene_changed(Some(&Arc::new((*scene).clone())));
        assert!(cache.entry.is_none());
        equivalent(
            &mut cache,
            &scene,
            camera(),
            rect(),
            2.,
            &flags,
            MmPoint::new(0., 0.),
        );
        cache.clear();
        assert!(cache.entry.is_none());
    }
}
