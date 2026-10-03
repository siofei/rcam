//! UI/runtime adapter for editor-core Object Snap.
use crate::{camera::Camera, tools, world_index::WorldIndex};
use editor_core::MmPoint;
use editor_core::snap::{
    SnapCandidate, SnapFeatureProvider, SnapGeometry, SnapGeometryCache, SnapKind, SnapQuery,
    SnapRadiiPx, SnapResolution, SnapResolver, intersection_features,
};
use editor_core::workspace::{aperture_shape_map, classify_object};
use editor_service::{LayerInfo, RenderSnapshot};
use std::collections::HashSet;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub enabled_kinds: Vec<SnapKind>,
    pub radius_px: f64,
    pub manufacturing_boundary: bool,
    pub original_path: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_kinds: vec![
                SnapKind::Endpoint,
                SnapKind::Vertex,
                SnapKind::Midpoint,
                SnapKind::Center,
                SnapKind::Quadrant,
                SnapKind::Intersection,
            ],
            radius_px: 8.,
            manufacturing_boundary: true,
            original_path: false,
        }
    }
}

impl Settings {
    /// Dedicated point/transform policy, never persisted to global preferences.
    pub fn contour(&self) -> Self {
        Self {
            enabled: true,
            enabled_kinds: vec![
                SnapKind::Endpoint,
                SnapKind::Vertex,
                SnapKind::Intersection,
                SnapKind::Midpoint,
                SnapKind::Quadrant,
                SnapKind::Center,
                SnapKind::ArcCenter,
                SnapKind::Nearest,
            ],
            radius_px: 8.,
            manufacturing_boundary: true,
            original_path: false,
        }
    }

    pub fn from_project(value: &rcam_project::SnapSettingsState) -> Self {
        Self {
            enabled: value.enabled,
            enabled_kinds: value.enabled_kinds.clone(),
            radius_px: value.radius_px,
            manufacturing_boundary: value.manufacturing_boundary,
            original_path: value.original_path,
        }
    }

    pub fn write_project(&self, value: &mut rcam_project::SnapSettingsState) {
        value.enabled = self.enabled;
        value.enabled_kinds = self.enabled_kinds.clone();
        value.radius_px = self.radius_px;
        value.manufacturing_boundary = self.manufacturing_boundary;
        value.original_path = self.original_path;
    }

    pub fn set_kind(&mut self, kind: SnapKind, enabled: bool) {
        self.enabled_kinds.retain(|current| *current != kind);
        if enabled {
            self.enabled_kinds.push(kind);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Stats {
    pub nearby_objects: usize,
    pub features_generated: usize,
    pub intersection_pairs: usize,
    pub candidate_query_us: u128,
    pub resolver_us: u128,
    pub retained_previous: bool,
    pub elapsed_us: u128,
}

#[derive(Default)]
pub struct Runtime {
    cache: SnapGeometryCache,
    navigation: Option<[f64; 4]>,
    previous: Option<editor_core::snap::SnapCandidateId>,
    pub current: Option<SnapResolution>,
    pub stats: Stats,
}

struct Nearby {
    layer_id: String,
    object_id: String,
    geometry: SnapGeometry,
}

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_definition(
        &mut self,
        raw: MmPoint,
        definition: &editor_core::block::BlockDefinition,
        apertures: &[editor_core::ApertureDefinition],
        camera: Camera,
        ppp: f32,
        grid: tools::GridSettings,
        alt: bool,
    ) -> Result<SnapResolution, String> {
        let navigation = [
            camera.center.x_mm,
            camera.center.y_mm,
            camera.scale,
            f64::from(ppp),
        ];
        if self.navigation != Some(navigation) {
            self.reset();
            self.navigation = Some(navigation);
        }
        if alt {
            self.reset();
            return Ok(raw_resolution(raw));
        }
        let resolver = SnapResolver::default();
        let query = SnapQuery::from_screen(
            raw,
            SnapRadiiPx {
                acquire: 8.,
                candidate: 11.,
            },
            camera.scale,
            f64::from(ppp),
            Settings::default().contour().enabled_kinds,
            true,
            false,
        )
        .ok_or("定义局部拾取缩放无效")?;
        let instance = editor_core::SemanticGeometry::BlockInstance {
            definition_id: definition.id.clone(),
            transform: editor_core::block::BlockTransform {
                translation: MmPoint::new(0., 0.),
                rotation_deg: 0.,
                mirror: false,
            },
        };
        let geometry = self
            .cache
            .geometry_for(&instance, apertures, std::slice::from_ref(definition))
            .map_err(|e| e.to_string())?;
        let candidates: Vec<_> = geometry
            .snap_features(&query)
            .into_iter()
            .map(|feature| SnapCandidate {
                layer_id: "definition-local".into(),
                object_id: definition.id.0.clone(),
                related_object_id: None,
                distance_mm: raw.distance_mm(feature.point),
                feature,
            })
            .collect();
        let value = resolver.resolve(
            raw,
            camera.scale * f64::from(ppp),
            8.,
            &candidates,
            if grid.snap_enabled {
                Some(grid.point(raw)?)
            } else {
                None
            },
            self.previous.as_ref(),
        );
        self.previous = value.candidate.clone();
        self.current = Some(value.clone());
        Ok(value)
    }
    pub fn reset(&mut self) {
        self.previous = None;
        self.current = None;
        self.stats = Stats::default();
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.reset();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &mut self,
        raw: MmPoint,
        settings: &Settings,
        grid: tools::GridSettings,
        camera: Camera,
        pixels_per_point: f32,
        snapshot: Option<&RenderSnapshot>,
        index: &WorldIndex,
        layers: &[LayerInfo],
        excluded: Option<&HashSet<String>>,
        temporarily_disabled: bool,
    ) -> Result<SnapResolution, String> {
        let navigation = [
            camera.center.x_mm,
            camera.center.y_mm,
            camera.scale,
            f64::from(pixels_per_point),
        ];
        if self.navigation != Some(navigation) {
            self.reset();
            self.navigation = Some(navigation);
        }
        let started = Instant::now();
        if !raw.is_finite() {
            return Err("坐标不是有限值".into());
        }
        let pixels_per_mm = camera.scale * f64::from(pixels_per_point);
        if !pixels_per_mm.is_finite() || pixels_per_mm <= 0. {
            return Err("当前缩放无法计算吸附半径".into());
        }
        if temporarily_disabled {
            self.reset();
            return Ok(raw_resolution(raw));
        }
        let grid_point = if grid.snap_enabled {
            Some(grid.point(raw)?)
        } else {
            None
        };
        let mut candidates = Vec::new();
        let mut nearby = Vec::new();
        let mut intersection_pairs = 0;
        let resolver = SnapResolver::default();
        if settings.enabled
            && (settings.manufacturing_boundary || settings.original_path)
            && let Some(snapshot) = snapshot
        {
            let query = SnapQuery::from_screen(
                raw,
                SnapRadiiPx {
                    acquire: settings.radius_px,
                    candidate: settings.radius_px + resolver.release_extra_px,
                },
                camera.scale,
                f64::from(pixels_per_point),
                settings.enabled_kinds.clone(),
                settings.manufacturing_boundary,
                settings.original_path,
            )
            .ok_or_else(|| "Object Snap 半径或缩放无效".to_string())?;
            let shapes = aperture_shape_map(&snapshot.apertures);
            for (layer_index, object_index) in index.query_indices(query.bounds()) {
                let layer = &snapshot.layers[layer_index];
                let object = &layer.objects[object_index];
                if excluded.is_some_and(|excluded| excluded.contains(&object.object_id)) {
                    continue;
                }
                let Some(workspace) = layers.iter().find(|candidate| {
                    candidate.layer_id == layer.id
                        && candidate.visible
                        && candidate.effective_visible
                        && candidate.selectable
                }) else {
                    continue;
                };
                let class = classify_object(object, &shapes);
                if workspace
                    .classes
                    .iter()
                    .any(|style| style.class == class && (!style.visible || !style.selectable))
                {
                    continue;
                }
                let geometry = self
                    .cache
                    .geometry_for(
                        &object.geometry,
                        &snapshot.apertures,
                        &snapshot.block_definitions,
                    )
                    .map_err(|error| format!("Object Snap geometry: {error}"))?;
                for feature in geometry.snap_features(&query) {
                    candidates.push(SnapCandidate {
                        layer_id: layer.id.clone(),
                        object_id: object.object_id.clone(),
                        related_object_id: None,
                        distance_mm: raw.distance_mm(feature.point),
                        feature,
                    });
                }
                nearby.push(Nearby {
                    layer_id: layer.id.clone(),
                    object_id: object.object_id.clone(),
                    geometry,
                });
            }
            nearby.sort_by(|left, right| {
                left.layer_id
                    .cmp(&right.layer_id)
                    .then(left.object_id.cmp(&right.object_id))
            });
            for first in 0..nearby.len() {
                for second in first + 1..nearby.len() {
                    let (features, pairs) = intersection_features(
                        &nearby[first].geometry,
                        &nearby[second].geometry,
                        &query,
                    );
                    intersection_pairs += pairs;
                    for feature in features {
                        candidates.push(SnapCandidate {
                            layer_id: nearby[first].layer_id.clone(),
                            object_id: nearby[first].object_id.clone(),
                            related_object_id: Some(nearby[second].object_id.clone()),
                            distance_mm: raw.distance_mm(feature.point),
                            feature,
                        });
                    }
                }
            }
        }
        let resolver_started = Instant::now();
        let previous = self.previous.clone();
        let resolution = resolver.resolve(
            raw,
            pixels_per_mm,
            settings.radius_px,
            &candidates,
            grid_point,
            previous.as_ref(),
        );
        let resolver_us = resolver_started.elapsed().as_micros();
        let retained_previous = previous.is_some() && resolution.candidate == previous;
        self.previous = resolution.candidate.clone();
        self.stats = Stats {
            nearby_objects: nearby.len(),
            features_generated: candidates.len(),
            intersection_pairs,
            candidate_query_us: resolver_started.duration_since(started).as_micros(),
            resolver_us,
            retained_previous,
            elapsed_us: started.elapsed().as_micros(),
        };
        static LAST_SLOW: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static LAST_TRANSITION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let fields = [
            ("candidate_query_us", self.stats.candidate_query_us as u64),
            ("resolver_us", resolver_us as u64),
            ("nearby_objects", nearby.len() as u64),
            ("features_generated", candidates.len() as u64),
            ("intersection_pairs", intersection_pairs as u64),
        ];
        if self.stats.candidate_query_us > 5000 || intersection_pairs > 10000 {
            rcam_diagnostics::rate_limited(
                &LAST_SLOW,
                rcam_diagnostics::Level::Warn,
                "snap.query.slow_or_high_pairs",
                &fields,
            );
        }
        if previous != resolution.candidate {
            let event = match (&previous, &resolution.candidate) {
                (None, Some(_)) => "snap.acquire",
                (Some(_), None) => "snap.release",
                _ => "snap.switch",
            };
            rcam_diagnostics::rate_limited(
                &LAST_TRANSITION,
                rcam_diagnostics::Level::Debug,
                event,
                &fields,
            );
        }
        self.current = Some(resolution.clone());
        Ok(resolution)
    }
}

fn raw_resolution(point: MmPoint) -> SnapResolution {
    SnapResolution {
        point,
        kind: None,
        feature: None,
        source: None,
        candidate: None,
        distance_px: None,
        from_grid: false,
    }
}

pub fn kind_label(kind: SnapKind) -> &'static str {
    match kind {
        SnapKind::Endpoint | SnapKind::Vertex => "端点",
        SnapKind::Midpoint => "中点",
        SnapKind::Center | SnapKind::ArcCenter => "圆心",
        SnapKind::Quadrant => "象限点",
        SnapKind::Intersection => "交点",
        SnapKind::Nearest => "最近点",
        SnapKind::Perpendicular => "垂足",
        SnapKind::Tangent => "切点",
    }
}

pub fn paint_marker(
    painter: &eframe::egui::Painter,
    camera: Camera,
    rect: eframe::egui::Rect,
    pixels_per_point: f32,
    resolution: &SnapResolution,
) {
    let Some(kind) = resolution.kind else { return };
    let center = camera.screen(resolution.point, rect);
    let size = 7. / pixels_per_point.max(1.);
    let stroke = eframe::egui::Stroke::new(
        1.5 / pixels_per_point.max(1.),
        eframe::egui::Color32::from_rgb(94, 235, 205),
    );
    use eframe::egui::{Shape, vec2};
    match kind {
        SnapKind::Endpoint | SnapKind::Vertex => painter.rect_stroke(
            eframe::egui::Rect::from_center_size(center, vec2(size * 2., size * 2.)),
            0.,
            stroke,
            eframe::egui::StrokeKind::Middle,
        ),
        SnapKind::Midpoint => painter.add(Shape::closed_line(
            vec![
                center + vec2(0., -size),
                center + vec2(size, size),
                center + vec2(-size, size),
            ],
            stroke,
        )),
        SnapKind::Center | SnapKind::ArcCenter => {
            painter.circle_stroke(center, size, stroke);
            painter.line_segment([center - vec2(size, 0.), center + vec2(size, 0.)], stroke);
            painter.line_segment([center - vec2(0., size), center + vec2(0., size)], stroke)
        }
        SnapKind::Quadrant => painter.add(Shape::closed_line(
            vec![
                center + vec2(0., -size),
                center + vec2(size, 0.),
                center + vec2(0., size),
                center + vec2(-size, 0.),
            ],
            stroke,
        )),
        SnapKind::Intersection => {
            painter.line_segment(
                [center - vec2(size, size), center + vec2(size, size)],
                stroke,
            );
            painter.line_segment(
                [center + vec2(-size, size), center + vec2(size, -size)],
                stroke,
            )
        }
        SnapKind::Nearest => {
            painter.line_segment([center - vec2(size, 0.), center + vec2(size, 0.)], stroke);
            painter.line_segment([center, center + vec2(0., -size)], stroke)
        }
        SnapKind::Perpendicular | SnapKind::Tangent => painter.circle_stroke(center, size, stroke),
    };
    painter.circle_filled(center, 1.5 / pixels_per_point.max(1.), stroke.color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{
        ApertureDefinition, ApertureShape, Exposure, LocalTransform, ObjectOrigin, RegionContour,
        RegionEdge, RegionRole, SemanticGeometry, SemanticLayer, SemanticObject,
    };
    use std::time::Instant;

    #[test]
    fn defaults_match_product_gate_and_project_roundtrip() {
        let settings = Settings::default();
        assert!(settings.manufacturing_boundary && !settings.original_path);
        assert!(!settings.enabled_kinds.contains(&SnapKind::Nearest));
        let mut project = rcam_project::SnapSettingsState {
            enabled: false,
            enabled_kinds: vec![],
            radius_px: 4.,
            manufacturing_boundary: false,
            original_path: true,
        };
        settings.write_project(&mut project);
        assert_eq!(Settings::from_project(&project), settings);
    }

    #[test]
    fn labels_cover_intersection() {
        assert_eq!(kind_label(SnapKind::Intersection), "交点");
    }

    fn circle_object(id: usize, x: f64, y: f64) -> SemanticObject {
        SemanticObject {
            object_id: format!("o-{id}"),
            geometry: SemanticGeometry::Flash {
                center: MmPoint::new(x, y),
                aperture_id: "circle".into(),
                transform: LocalTransform::default(),
            },
            exposure: Exposure::Dark,
            origin: ObjectOrigin::Generated {
                operation_id: "snap-performance".into(),
            },
        }
    }

    fn snapshot(layers: Vec<SemanticLayer>) -> RenderSnapshot {
        RenderSnapshot {
            document_id: "snap-performance".into(),
            revision: "0".into(),
            workspace_revision: "0".into(),
            layers,
            apertures: vec![ApertureDefinition {
                id: "circle".into(),
                source_dcode: 10,
                shape: ApertureShape::Circle {
                    diameter_mm: 0.2,
                    hole_diameter_mm: None,
                },
            }],
            styles: vec![],
            block_definitions: vec![],
        }
    }

    fn region_object(id: &str, edge: RegionEdge) -> SemanticObject {
        SemanticObject {
            object_id: id.into(),
            geometry: SemanticGeometry::Region {
                contours: vec![RegionContour {
                    role: RegionRole::Solid,
                    edges: vec![edge],
                }],
            },
            exposure: Exposure::Dark,
            origin: ObjectOrigin::Generated {
                operation_id: "runtime-hysteresis".into(),
            },
        }
    }

    fn runtime_layers(snapshot: &RenderSnapshot) -> Vec<LayerInfo> {
        snapshot
            .layers
            .iter()
            .enumerate()
            .map(|(z_index, layer)| LayerInfo {
                layer_id: layer.id.clone(),
                z_index,
                ..Default::default()
            })
            .collect()
    }

    fn resolve_runtime(
        runtime: &mut Runtime,
        snapshot: &RenderSnapshot,
        index: &WorldIndex,
        layers: &[LayerInfo],
        settings: &Settings,
        raw: MmPoint,
        grid: tools::GridSettings,
    ) -> SnapResolution {
        runtime
            .resolve(
                raw,
                settings,
                grid,
                Camera {
                    scale: 100.,
                    ..Default::default()
                },
                2.,
                Some(snapshot),
                index,
                layers,
                None,
                false,
            )
            .unwrap()
    }

    #[test]
    fn runtime_static_center_uses_eight_px_acquire_and_eleven_px_release() {
        let snapshot = snapshot(vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![circle_object(0, 0., 0.)],
        }]);
        let index = WorldIndex::build(&snapshot).unwrap();
        let layers = runtime_layers(&snapshot);
        let settings = Settings {
            enabled: true,
            enabled_kinds: vec![SnapKind::Center],
            ..Default::default()
        };
        let mut runtime = Runtime::default();

        let acquired = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(7.5 / 200., 0.),
            tools::GridSettings::default(),
        );
        assert_eq!(acquired.kind, Some(SnapKind::Center));
        let feature = acquired.feature.clone();
        for distance_px in [9.0, 10.9] {
            let retained = resolve_runtime(
                &mut runtime,
                &snapshot,
                &index,
                &layers,
                &settings,
                MmPoint::new(distance_px / 200., 0.),
                tools::GridSettings::default(),
            );
            assert_eq!(retained.feature, feature, "distance_px={distance_px}");
        }
        let released = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(11.1 / 200., 0.),
            tools::GridSettings::default(),
        );
        assert_eq!(released.kind, None);
    }

    #[test]
    fn runtime_does_not_acquire_a_new_object_candidate_beyond_eight_px() {
        let snapshot = snapshot(vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![circle_object(0, 0., 0.)],
        }]);
        let index = WorldIndex::build(&snapshot).unwrap();
        let layers = runtime_layers(&snapshot);
        let settings = Settings {
            enabled: true,
            enabled_kinds: vec![SnapKind::Center],
            ..Default::default()
        };
        let grid = tools::GridSettings {
            snap_enabled: true,
            spacing_mm: 0.1,
            ..Default::default()
        };
        let result = resolve_runtime(
            &mut Runtime::default(),
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(9. / 200., 0.),
            grid,
        );
        assert_eq!(result.kind, None);
        assert!(result.from_grid);
    }

    #[test]
    fn runtime_nearest_retains_the_same_edge_until_release_radius() {
        let snapshot = snapshot(vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![region_object(
                "line",
                RegionEdge::Line {
                    start: MmPoint::new(-1., 0.),
                    end: MmPoint::new(1., 0.),
                },
            )],
        }]);
        let index = WorldIndex::build(&snapshot).unwrap();
        let layers = runtime_layers(&snapshot);
        let settings = Settings {
            enabled: true,
            enabled_kinds: vec![SnapKind::Nearest],
            ..Default::default()
        };
        let mut runtime = Runtime::default();
        let acquired = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(0., 7.5 / 200.),
            tools::GridSettings::default(),
        );
        assert_eq!(acquired.kind, Some(SnapKind::Nearest));
        let feature = acquired.feature.clone();
        for distance_px in [9.0, 10.9] {
            let retained = resolve_runtime(
                &mut runtime,
                &snapshot,
                &index,
                &layers,
                &settings,
                MmPoint::new(0., distance_px / 200.),
                tools::GridSettings::default(),
            );
            assert_eq!(retained.feature, feature, "distance_px={distance_px}");
        }
        assert_eq!(
            resolve_runtime(
                &mut runtime,
                &snapshot,
                &index,
                &layers,
                &settings,
                MmPoint::new(0., 11.1 / 200.),
                tools::GridSettings::default(),
            )
            .kind,
            None
        );
    }

    #[test]
    fn runtime_intersection_hysteresis_covers_line_line_and_line_arc() {
        let settings = Settings {
            enabled: true,
            enabled_kinds: vec![SnapKind::Intersection],
            ..Default::default()
        };
        let cases = [
            (
                vec![
                    region_object(
                        "horizontal",
                        RegionEdge::Line {
                            start: MmPoint::new(-2., 0.),
                            end: MmPoint::new(2., 0.),
                        },
                    ),
                    region_object(
                        "vertical",
                        RegionEdge::Line {
                            start: MmPoint::new(0., -2.),
                            end: MmPoint::new(0., 2.),
                        },
                    ),
                ],
                MmPoint::new(0., 0.),
            ),
            (
                vec![
                    region_object(
                        "line",
                        RegionEdge::Line {
                            start: MmPoint::new(1., -2.),
                            end: MmPoint::new(1., 2.),
                        },
                    ),
                    region_object(
                        "arc",
                        RegionEdge::Arc(editor_core::ArcGeometry {
                            start: MmPoint::new(1., 0.),
                            end: MmPoint::new(1., 0.),
                            center: MmPoint::new(0., 0.),
                            direction: editor_core::ArcDirection::CounterClockwise,
                            full_circle: true,
                            source: None,
                        }),
                    ),
                ],
                MmPoint::new(1., 0.),
            ),
        ];
        for (objects, intersection) in cases {
            let snapshot = snapshot(vec![SemanticLayer {
                id: "layer".into(),
                objects,
            }]);
            let index = WorldIndex::build(&snapshot).unwrap();
            let layers = runtime_layers(&snapshot);
            let mut runtime = Runtime::default();
            let acquired = resolve_runtime(
                &mut runtime,
                &snapshot,
                &index,
                &layers,
                &settings,
                MmPoint::new(intersection.x_mm + 7.5 / 200., intersection.y_mm),
                tools::GridSettings::default(),
            );
            assert_eq!(acquired.kind, Some(SnapKind::Intersection));
            let feature = acquired.feature.clone();
            for distance_px in [9.0, 10.9] {
                let retained = resolve_runtime(
                    &mut runtime,
                    &snapshot,
                    &index,
                    &layers,
                    &settings,
                    MmPoint::new(intersection.x_mm + distance_px / 200., intersection.y_mm),
                    tools::GridSettings::default(),
                );
                assert_eq!(retained.feature, feature, "distance_px={distance_px}");
            }
            assert_eq!(
                resolve_runtime(
                    &mut runtime,
                    &snapshot,
                    &index,
                    &layers,
                    &settings,
                    MmPoint::new(intersection.x_mm + 11.1 / 200., intersection.y_mm),
                    tools::GridSettings::default(),
                )
                .kind,
                None
            );
        }
    }

    #[test]
    fn runtime_retains_previous_candidate_before_switching_to_a_closer_one() {
        let snapshot = snapshot(vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![circle_object(0, 0., 0.), circle_object(1, 0.07, 0.)],
        }]);
        let index = WorldIndex::build(&snapshot).unwrap();
        let layers = runtime_layers(&snapshot);
        let settings = Settings {
            enabled: true,
            enabled_kinds: vec![SnapKind::Center],
            ..Default::default()
        };
        let mut runtime = Runtime::default();
        let acquired = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(0.03, 0.),
            tools::GridSettings::default(),
        );
        assert_eq!(acquired.point, MmPoint::new(0., 0.));
        let retained = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(0.045, 0.),
            tools::GridSettings::default(),
        );
        assert_eq!(retained.point, acquired.point);
        assert_eq!(retained.feature, acquired.feature);
        let switched = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(0.06, 0.),
            tools::GridSettings::default(),
        );
        assert_eq!(switched.point, MmPoint::new(0.07, 0.));
    }

    #[test]
    fn runtime_alt_disable_resets_hysteresis_before_reenable() {
        let snapshot = snapshot(vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![circle_object(0, 0., 0.)],
        }]);
        let index = WorldIndex::build(&snapshot).unwrap();
        let layers = runtime_layers(&snapshot);
        let settings = Settings {
            enabled: true,
            enabled_kinds: vec![SnapKind::Center],
            ..Default::default()
        };
        let camera = Camera {
            scale: 100.,
            ..Default::default()
        };
        let mut runtime = Runtime::default();
        let acquired = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            MmPoint::new(7.5 / 200., 0.),
            tools::GridSettings::default(),
        );
        assert_eq!(acquired.kind, Some(SnapKind::Center));
        let raw = MmPoint::new(9. / 200., 0.);
        let disabled = runtime
            .resolve(
                raw,
                &settings,
                tools::GridSettings::default(),
                camera,
                2.,
                Some(&snapshot),
                &index,
                &layers,
                None,
                true,
            )
            .unwrap();
        assert_eq!(disabled.point, raw);
        assert_eq!(disabled.kind, None);
        let reenabled = resolve_runtime(
            &mut runtime,
            &snapshot,
            &index,
            &layers,
            &settings,
            raw,
            tools::GridSettings::default(),
        );
        assert_eq!(reenabled.kind, None, "9 px must not reacquire after Alt");
    }

    #[test]
    #[ignore = "native Apple Silicon runtime observation evidence"]
    fn s4c1_native_runtime_hysteresis_observations() {
        let output = std::env::var_os("RCAM_S4C1_NATIVE_HYSTERESIS_OUT")
            .expect("RCAM_S4C1_NATIVE_HYSTERESIS_OUT required");
        let pixels_per_point = std::env::var("RCAM_S4C1_NATIVE_PPP")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(2.);
        let camera = Camera {
            scale: 100.,
            ..Default::default()
        };
        let pixels_per_mm = camera.scale * f64::from(pixels_per_point);
        let snapshot = snapshot(vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![circle_object(0, 0., 0.)],
        }]);
        let index = WorldIndex::build(&snapshot).unwrap();
        let layers = runtime_layers(&snapshot);
        let settings = Settings {
            enabled: true,
            enabled_kinds: vec![SnapKind::Center],
            ..Default::default()
        };
        let mut runtime = Runtime::default();
        let mut observations = Vec::new();
        for distance_px in [7.5, 9.0, 10.9, 11.1] {
            let resolution = runtime
                .resolve(
                    MmPoint::new(distance_px / pixels_per_mm, 0.),
                    &settings,
                    tools::GridSettings::default(),
                    camera,
                    pixels_per_point,
                    Some(&snapshot),
                    &index,
                    &layers,
                    None,
                    false,
                )
                .unwrap();
            observations.push(serde_json::json!({
                "raw_distance_px": distance_px,
                "acquire_radius_px": settings.radius_px,
                "release_radius_px": settings.radius_px + SnapResolver::default().release_extra_px,
                "snap_kind": resolution.kind.map(|kind| format!("{kind:?}")),
                "feature_id": resolution.feature.as_ref().map(|feature| format!("{feature:?}")),
                "retained_previous": runtime.stats.retained_previous,
                "resolved_world_point": [resolution.point.x_mm, resolution.point.y_mm],
                "nearby_objects": runtime.stats.nearby_objects,
                "features_generated": runtime.stats.features_generated,
                "intersection_pairs": runtime.stats.intersection_pairs,
                "candidate_query_us": runtime.stats.candidate_query_us,
                "resolver_us": runtime.stats.resolver_us,
            }));
        }
        runtime.reset();
        let resolution = runtime
            .resolve(
                MmPoint::new(9. / pixels_per_mm, 0.),
                &settings,
                tools::GridSettings::default(),
                camera,
                pixels_per_point,
                Some(&snapshot),
                &index,
                &layers,
                None,
                false,
            )
            .unwrap();
        observations.push(serde_json::json!({
            "raw_distance_px": 9.0,
            "acquire_radius_px": settings.radius_px,
            "release_radius_px": settings.radius_px + SnapResolver::default().release_extra_px,
            "snap_kind": resolution.kind.map(|kind| format!("{kind:?}")),
            "feature_id": resolution.feature.as_ref().map(|feature| format!("{feature:?}")),
            "retained_previous": runtime.stats.retained_previous,
            "resolved_world_point": [resolution.point.x_mm, resolution.point.y_mm],
            "case": "no_previous",
        }));
        assert_eq!(observations[0]["snap_kind"], "Center");
        assert_eq!(observations[1]["feature_id"], observations[0]["feature_id"]);
        assert_eq!(observations[2]["feature_id"], observations[0]["feature_id"]);
        assert!(observations[1]["retained_previous"].as_bool().unwrap());
        assert!(observations[2]["retained_previous"].as_bool().unwrap());
        assert!(observations[3]["snap_kind"].is_null());
        assert!(observations[4]["snap_kind"].is_null());

        let report = serde_json::json!({
            "schema_version": 2,
            "status": "PASS",
            "target_arch": std::env::consts::ARCH,
            "target_os": std::env::consts::OS,
            "pixels_per_point": pixels_per_point,
            "camera_points_per_mm": camera.scale,
            "observations": observations,
        });
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(output.parent().expect("output parent")).unwrap();
        std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }

    #[test]
    #[ignore = "release-only S4-C1 bounded Object Snap performance evidence"]
    fn s4c1_release_snap_performance() {
        let camera = Camera {
            scale: 100.,
            ..Default::default()
        };
        let settings = Settings {
            enabled: true,
            ..Default::default()
        };
        let grid = tools::GridSettings::default();

        let ten_by_1k = snapshot(
            (0..10)
                .map(|layer| SemanticLayer {
                    id: format!("layer-{layer}"),
                    objects: (0..1_000)
                        .map(|object| {
                            circle_object(layer * 1_000 + object, object as f64, layer as f64)
                        })
                        .collect(),
                })
                .collect(),
        );
        let started = Instant::now();
        let ten_by_1k_index = WorldIndex::build(&ten_by_1k).unwrap();
        let index_build_us = started.elapsed().as_micros();
        let layer_info: Vec<_> = ten_by_1k
            .layers
            .iter()
            .enumerate()
            .map(|(z_index, layer)| LayerInfo {
                layer_id: layer.id.clone(),
                z_index,
                ..Default::default()
            })
            .collect();
        let mut runtime = Runtime::default();
        let result = runtime
            .resolve(
                MmPoint::new(500.1, 5.),
                &settings,
                grid,
                camera,
                2.,
                Some(&ten_by_1k),
                &ten_by_1k_index,
                &layer_info,
                None,
                false,
            )
            .unwrap();
        assert!(result.kind.is_some());
        assert!(runtime.stats.nearby_objects <= 10);

        let hundred_k = snapshot(vec![SemanticLayer {
            id: "hundred-k".into(),
            objects: (0..100_000)
                .map(|object| circle_object(object, object as f64, 0.))
                .collect(),
        }]);
        let started = Instant::now();
        let hundred_k_index = WorldIndex::build(&hundred_k).unwrap();
        let hundred_k_build_us = started.elapsed().as_micros();
        let hundred_k_layers = vec![LayerInfo {
            layer_id: "hundred-k".into(),
            ..Default::default()
        }];
        let mut hundred_k_runtime = Runtime::default();
        hundred_k_runtime
            .resolve(
                MmPoint::new(50_000.1, 0.),
                &settings,
                grid,
                camera,
                2.,
                Some(&hundred_k),
                &hundred_k_index,
                &hundred_k_layers,
                None,
                false,
            )
            .unwrap();
        assert!(hundred_k_runtime.stats.nearby_objects <= 1);

        let dense = RenderSnapshot {
            layers: vec![SemanticLayer {
                id: "dense-region".into(),
                objects: vec![SemanticObject {
                    object_id: "region".into(),
                    geometry: SemanticGeometry::Region {
                        contours: vec![RegionContour {
                            role: RegionRole::CompatibilitySolid,
                            edges: (0..2_000)
                                .map(|edge| RegionEdge::Line {
                                    start: MmPoint::new(edge as f64, 0.),
                                    end: MmPoint::new(edge as f64 + 0.5, 0.),
                                })
                                .collect(),
                        }],
                    },
                    exposure: Exposure::Dark,
                    origin: ObjectOrigin::Generated {
                        operation_id: "dense-region".into(),
                    },
                }],
            }],
            apertures: vec![],
            ..snapshot(vec![])
        };
        let dense_index = WorldIndex::build(&dense).unwrap();
        let dense_layers = vec![LayerInfo {
            layer_id: "dense-region".into(),
            ..Default::default()
        }];
        let mut dense_runtime = Runtime::default();
        dense_runtime
            .resolve(
                MmPoint::new(1_000., 0.),
                &settings,
                grid,
                camera,
                2.,
                Some(&dense),
                &dense_index,
                &dense_layers,
                None,
                false,
            )
            .unwrap();
        assert_eq!(dense_runtime.stats.nearby_objects, 1);

        let report = serde_json::json!({
            "schema_version": 2,
            "build_profile": "release",
            "ten_layers_x_1k": {
                "document_objects": 10_000,
                "index_build_us": index_build_us,
                "query": runtime.stats,
            },
            "hundred_k_spatial_index": {
                "document_objects": 100_000,
                "index_build_us": hundred_k_build_us,
                "query": hundred_k_runtime.stats,
            },
            "dense_region": {
                "edges": 2_000,
                "query": dense_runtime.stats,
                "note": "first version scans one nearby object's static features; elapsed data decides whether a local tree is warranted",
            },
        });
        println!("{report}");
        if let Some(dir) = std::env::var_os("RCAM_SNAP_EVIDENCE") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                std::path::PathBuf::from(dir).join("snap_performance.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
        }
    }
}
