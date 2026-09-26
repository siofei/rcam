//! Opt-in native evidence probe (`RCAM_NATIVE_PROBE_DIR=<dir>`).
//! S4-C2 geometry observation additionally requires `RCAM_NATIVE_SYNTHETIC_GRIP_SHA256`
//! to equal the embedded public fixture hash and matching import provenance.
//!
//! It only *observes*: every frame whose observable state changed appends one JSON
//! line to `native_observations.jsonl` (layers, revisions, dialogs, the exact screen
//! rectangle of each Layer-row control, pixels-per-point, panel width...), every
//! action sent to the service is appended to `native_actions.log`, and a screenshot
//! is written as `screens/<label>.ppm` when a `shot.request` file containing the label
//! appears in the directory. Nothing here changes application or manufacturing state.
use crate::{
    EditorApp, grip,
    state::{Action, View},
};
use eframe::egui;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

const S4C2_GRIP_FIXTURE: &[u8] = include_bytes!("../../../fixtures/synthetic/s4c2/grips.gbr");
const S4C2_GRIP_FIXTURE_NAME: &str = "grips.gbr";

fn fixture_sha256() -> String {
    editor_core::hash::sha256_hex(S4C2_GRIP_FIXTURE)
}

fn synthetic_fixture_gate(
    opt_in_sha256: Option<&str>,
    expected_sha256: &str,
    original_file_name: &str,
    layer_sha256: &str,
    layer_count: usize,
) -> bool {
    opt_in_sha256 == Some(expected_sha256)
        && original_file_name == S4C2_GRIP_FIXTURE_NAME
        && layer_sha256 == expected_sha256
        && layer_count == 1
}

pub struct Probe {
    dir: PathBuf,
    started: Instant,
    last: String,
    pending_shot: Option<String>,
    polled: Instant,
    fixture_sha256: String,
    synthetic_grip_opt_in: bool,
    pub drops: u32,
}

impl Probe {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("RCAM_NATIVE_PROBE_DIR")?);
        std::fs::create_dir_all(dir.join("screens")).ok()?;
        let fixture_sha256 = fixture_sha256();
        let synthetic_grip_opt_in = std::env::var("RCAM_NATIVE_SYNTHETIC_GRIP_SHA256")
            .is_ok_and(|value| value == fixture_sha256);
        let probe = Self {
            dir,
            started: Instant::now(),
            last: String::new(),
            pending_shot: None,
            polled: Instant::now(),
            fixture_sha256,
            synthetic_grip_opt_in,
            drops: 0,
        };
        probe.action("PROBE_START");
        Some(probe)
    }

    fn elapsed_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }

    pub fn action(&self, text: &str) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("native_actions.log"))
        {
            let _ = writeln!(f, "{}\t{text}", self.elapsed_ms());
        }
    }

    fn observe(&mut self, mut observation: Value) {
        let key = observation.to_string();
        if key == self.last {
            return;
        }
        self.last = key;
        observation["t_ms"] = json!(self.elapsed_ms());
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("native_observations.jsonl"))
        {
            let _ = writeln!(f, "{observation}");
        }
    }

    fn allows_synthetic_grip(&self, view: &View) -> bool {
        if !self.synthetic_grip_opt_in {
            return false;
        }
        let (Some(info), Some(layer), Some(snapshot)) = (
            view.info.as_ref(),
            view.layers.first(),
            view.snap_snapshot.as_deref(),
        ) else {
            return false;
        };
        let provenance = layer.provenance.as_ref();
        synthetic_fixture_gate(
            Some(&self.fixture_sha256),
            &self.fixture_sha256,
            provenance.map_or("", |p| p.original_file_name.as_str()),
            provenance.map_or("", |p| p.imported_sha256.as_str()),
            view.layers.len(),
        ) && snapshot.document_id == info.document_id
            && snapshot.revision == info.revision
            && snapshot.layers.len() == 1
            && snapshot.layers[0].id == layer.layer_id
    }

    pub(crate) fn synthetic_grip_observation(
        &self,
        view: &View,
        active: Option<&grip::Session>,
        raw_target: Option<editor_core::MmPoint>,
        snap: Option<&editor_core::snap::SnapResolution>,
    ) -> Option<Value> {
        self.allows_synthetic_grip(view).then(|| {
            synthetic_grip_observation(view, active, raw_target, snap, &self.fixture_sha256)
        })
    }

    pub(crate) fn record_grip_release(
        &mut self,
        view: &View,
        session: &grip::Session,
        raw_target: Option<editor_core::MmPoint>,
        snap: Option<&editor_core::snap::SnapResolution>,
    ) {
        let Some(mut observation) =
            self.synthetic_grip_observation(view, Some(session), raw_target, snap)
        else {
            return;
        };
        let Some(info) = view.info.as_ref() else {
            return;
        };
        if session.document != info.document_id
            || !matches!(
                session.object.origin,
                editor_core::ObjectOrigin::Imported { .. }
            )
        {
            return;
        }
        let submit_eligible = session.moved && session.preview.is_ok();
        let revision_after_expected = submit_eligible
            .then(|| {
                info.revision
                    .parse::<u64>()
                    .ok()
                    .and_then(|revision| revision.checked_add(1))
                    .map(|revision| revision.to_string())
            })
            .flatten();
        let event = json!({
            "schema_version": 2,
            "event": "grip_pointer_release",
            "fixture_sha256": self.fixture_sha256,
            "document_id": info.document_id,
            "revision_before": info.revision,
            "submit_eligible": submit_eligible,
            "revision_after_expected": revision_after_expected,
            "state_before": state_json(info),
            "grip": observation["active"].take(),
            "selected_geometry_before": selected_geometry(view),
            "fixture_flash_instances_before": fixture_flash_instances(view),
        });
        self.observe(event);
    }

    /// Screenshot requests / results. Returns true while the probe wants repaints.
    fn screenshots(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Screenshot { image, .. } = event {
                let label = self.pending_shot.take().unwrap_or_else(|| "unnamed".into());
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                let path = self.dir.join("screens").join(format!("{label}.ppm"));
                let _ = std::fs::write(&path, bytes);
                self.action(&format!(
                    "SCREENSHOT {label} {}x{}",
                    image.size[0], image.size[1]
                ));
            }
        }
        if self.pending_shot.is_none() && self.polled.elapsed() > Duration::from_millis(150) {
            self.polled = Instant::now();
            let request = self.dir.join("shot.request");
            if let Ok(text) = std::fs::read_to_string(&request) {
                let label: String = text
                    .trim()
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                    .collect();
                let _ = std::fs::remove_file(&request);
                if !label.is_empty() {
                    self.pending_shot = Some(label);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn point_json(point: editor_core::MmPoint) -> Value {
    json!([point.x_mm, point.y_mm])
}

fn state_json(info: &editor_service::DocumentInfo) -> Value {
    json!({
        "revision": info.revision,
        "workspace_revision": info.workspace_revision,
        "dirty": info.dirty,
        "undo_entries": info.undo_entries,
        "redo_entries": info.redo_entries,
    })
}

fn aperture_shape_json(shape: &editor_core::ApertureShape) -> Value {
    use editor_core::ApertureShape;
    match shape {
        ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm,
        } => json!({
            "kind": "circle", "diameter_mm": diameter_mm, "hole_diameter_mm": hole_diameter_mm,
        }),
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => json!({
            "kind": "rectangle", "width_mm": width_mm, "height_mm": height_mm,
            "hole_diameter_mm": hole_diameter_mm,
        }),
        ApertureShape::Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => json!({
            "kind": "obround", "width_mm": width_mm, "height_mm": height_mm,
            "hole_diameter_mm": hole_diameter_mm,
        }),
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => json!({
            "kind": "polygon", "diameter_mm": diameter_mm, "vertices": vertices,
            "rotation_deg": rotation_deg, "hole_diameter_mm": hole_diameter_mm,
        }),
        ApertureShape::Macro { primitives } => json!({
            "kind": "macro", "primitive_count": primitives.len(),
        }),
    }
}

fn geometry_json(
    object: &editor_core::SemanticObject,
    apertures: &[editor_core::ApertureDefinition],
    aperture_override: Option<&editor_core::ApertureShape>,
) -> Value {
    use editor_core::{RegionEdge, SemanticGeometry};
    let geometry = match &object.geometry {
        SemanticGeometry::Flash {
            center,
            aperture_id,
            transform,
        } => {
            let shape = aperture_override.or_else(|| {
                apertures
                    .iter()
                    .find(|aperture| aperture.id == *aperture_id)
                    .map(|a| &a.shape)
            });
            json!({
                "kind": "flash", "center_mm": point_json(*center), "aperture_id": aperture_id,
                "transform": {"mirror": format!("{:?}", transform.mirror),
                    "rotation_deg": transform.rotation_deg, "scale": transform.scale},
                "aperture": shape.map(aperture_shape_json),
            })
        }
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => json!({
            "kind": "line", "start_mm": point_json(*start), "end_mm": point_json(*end),
            "width_mm": width_mm,
        }),
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => json!({
            "kind": "rectangular_sweep", "start_mm": point_json(*start), "end_mm": point_json(*end),
            "width_mm": width_mm, "height_mm": height_mm,
        }),
        SemanticGeometry::Arc { path, width_mm } => json!({
            "kind": "arc", "start_mm": point_json(path.start), "end_mm": point_json(path.end),
            "center_mm": point_json(path.center), "radius_mm": path.radius(),
            "direction": format!("{:?}", path.direction), "full_circle": path.full_circle,
            "arc_source": path.source.map(|source| json!({
                "resolution_mm": source.resolution_mm, "single_quadrant": source.single_quadrant,
            })),
            "width_mm": width_mm,
        }),
        SemanticGeometry::Region { contours } => json!({
            "kind": "region",
            "contours": contours.iter().map(|contour| json!({
                "role": format!("{:?}", contour.role),
                "line_edges": contour.edges.iter().filter_map(|edge| match edge {
                    RegionEdge::Line { start, end } => Some(json!({
                        "start_mm": point_json(*start), "end_mm": point_json(*end),
                    })),
                    RegionEdge::Arc(_) => None,
                }).collect::<Vec<_>>(),
                "arc_edge_count": contour.edges.iter().filter(|edge| matches!(edge, RegionEdge::Arc(_))).count(),
            })).collect::<Vec<_>>(),
        }),
        SemanticGeometry::BlockInstance { .. } => json!({"kind": "block_instance"}),
    };
    json!({
        "object_id": object.object_id,
        "exposure": format!("{:?}", object.exposure),
        "geometry": geometry,
    })
}

fn selected_geometry(view: &View) -> Value {
    let (Some(selected), Some(snapshot)) = (view.selected.primary(), view.snap_snapshot.as_ref())
    else {
        return Value::Null;
    };
    if !matches!(
        selected.object.origin,
        editor_core::ObjectOrigin::Imported { .. }
    ) {
        return Value::Null;
    }
    json!({
        "layer_id": selected.layer_id,
        "object": geometry_json(&selected.object, &snapshot.apertures, None),
    })
}

fn fixture_flash_instances(view: &View) -> Value {
    let Some(snapshot) = view.snap_snapshot.as_ref() else {
        return Value::Null;
    };
    json!(
        snapshot
            .layers
            .iter()
            .flat_map(|layer| {
                layer
                    .objects
                    .iter()
                    .filter(|object| {
                        matches!(object.origin, editor_core::ObjectOrigin::Imported { .. })
                            && matches!(
                                object.geometry,
                                editor_core::SemanticGeometry::Flash { .. }
                            )
                    })
                    .map(move |object| {
                        json!({
                            "layer_id": layer.id,
                            "object": geometry_json(object, &snapshot.apertures, None),
                        })
                    })
            })
            .collect::<Vec<_>>()
    )
}

fn snap_json(snap: Option<&editor_core::snap::SnapResolution>) -> Value {
    let Some(snap) = snap else { return Value::Null };
    json!({
        "resolved_mm": point_json(snap.point),
        "kind": snap.kind.map(|kind| format!("{:?}", kind)),
        "feature": snap.feature.as_ref().map(|feature| format!("{:?}", feature)),
        "source": snap.source.map(|source| format!("{:?}", source)),
        "candidate": snap.candidate.as_ref().map(|candidate| json!({
            "layer_id": candidate.layer_id, "object_id": candidate.object_id,
            "related_object_id": candidate.related_object_id,
            "feature_id": format!("{:?}", candidate.feature_id),
        })),
        "distance_px": snap.distance_px,
        "from_grid": snap.from_grid,
    })
}

fn synthetic_grip_observation(
    view: &View,
    active: Option<&grip::Session>,
    raw_target: Option<editor_core::MmPoint>,
    snap: Option<&editor_core::snap::SnapResolution>,
    fixture_sha256: &str,
) -> Value {
    let Some(info) = view.info.as_ref() else {
        return Value::Null;
    };
    let active = active.filter(|session| {
        session.document == info.document_id
            && matches!(
                session.object.origin,
                editor_core::ObjectOrigin::Imported { .. }
            )
    });
    let active_json = active.map(|session| {
        let original = geometry_json(&session.object, &view.apertures, None);
        let preview = session.preview.as_ref().ok().map(|preview| {
            let object = editor_core::SemanticObject {
                geometry: preview.geometry.clone(),
                ..session.object.clone()
            };
            geometry_json(&object, &view.apertures, preview.aperture_shape.as_ref())
        });
        let mut excluded: Vec<_> = session.excluded.iter().cloned().collect();
        excluded.sort();
        json!({
            "id": format!("{:?}", session.id),
            "layer_id": session.layer,
            "object_id": session.object.object_id,
            "aperture_id_before": match &session.object.geometry {
                editor_core::SemanticGeometry::Flash { aperture_id, .. } => Some(aperture_id),
                _ => None,
            },
            "raw_target_mm": raw_target.map(point_json),
            "resolved_target_mm": point_json(session.target),
            "snap": snap_json(snap),
            "excluded_object_ids": excluded,
            "moved": session.moved,
            "preview_valid": session.preview.is_ok(),
            "preview_error": session.preview.as_ref().err(),
            "geometry_before": original,
            "geometry_preview": preview,
        })
    });
    json!({
        "fixture_sha256": fixture_sha256,
        "document_id": info.document_id,
        "revision": info.revision,
        "state": state_json(info),
        "selected_geometry": selected_geometry(view),
        "fixture_flash_instances": fixture_flash_instances(view),
        "active": active_json,
    })
}

/// Short, stable description of the action sent to the service.
pub fn action_text(a: &Action) -> String {
    let basename = |path: &std::path::Path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    match a {
        Action::Open(p) => format!("Open {}", basename(p)),
        Action::OpenProject(p, _) => format!("OpenProject {}", basename(p)),
        Action::SaveProject(path, replace, _) => format!(
            "SaveProject {} replace={replace}",
            path.as_deref().map_or("current".into(), basename)
        ),
        Action::RestoreProject(_) => "RestoreProject".into(),
        Action::RecoveryWrite(_) => "RecoveryWrite".into(),
        Action::NewWorkspace => "NewWorkspace".into(),
        Action::DiscardNewWorkspace => "DiscardNewWorkspace".into(),
        Action::ImportGerbers(paths) => format!(
            "ImportGerbers n={} [{}]",
            paths.len(),
            paths
                .iter()
                .map(|p| p
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Action::CreateEmptyLayer(n) => format!("CreateEmptyLayer {n:?}"),
        Action::LayerSummary(id) => format!("LayerSummary {id}"),
        Action::RemoveLayer(id, force) => format!("RemoveLayer {id} allow_non_empty={force}"),
        Action::ReorderLayers(order) => format!("ReorderLayers {}", order.join(",")),
        Action::SetActiveLayer(id) => format!("SetActiveLayer {id:?}"),
        Action::SetSoloLayer(id) => format!("SetSoloLayer {id:?}"),
        Action::SetAllLayersVisible(v) => format!("SetAllLayersVisible {v}"),
        Action::ResetLayerColors => "ResetLayerColors".into(),
        Action::FitLayer(id) => format!("FitLayer {id}"),
        Action::Layer(p) => format!("Layer {p:?}"),
        Action::Select(pt, tol, mode) => format!(
            "Select ({:.3},{:.3}) tol={tol} mode={mode:?}",
            pt.x_mm, pt.y_mm
        ),
        Action::SelectRect(..) => "SelectRect".into(),
        Action::GripEdit(grip) => format!("GripEdit {:?}", grip.id),
        Action::DragMove(drag) => format!(
            "DragMove objects={} delta=({:.6},{:.6})",
            drag.objects.len(),
            drag.delta.x_mm,
            drag.delta.y_mm
        ),
        Action::Move(dx, dy) => format!("Move {dx} {dy}"),
        Action::Rotate(..) => "Rotate".into(),
        Action::Mirror(_) => "Mirror".into(),
        Action::Duplicate => "Duplicate".into(),
        Action::Delete => "DeleteObjects".into(),
        Action::History(redo) => format!("History {}", if *redo { "redo" } else { "undo" }),
        Action::Save(path, layer, _) => format!("ExportLayer {layer} -> {}", basename(path)),
        Action::SaveWithPrecision(path, layer, _, q) => {
            format!("ExportLayer {layer} -> {} at {q}mm", basename(path))
        }
        Action::Close(discard) => format!("Close discard={discard}"),
        Action::TextCreate(_) => "TextCreate".into(),
        _ => "Other".into(),
    }
}

fn rect_json(r: egui::Rect) -> Value {
    json!([r.min.x, r.min.y, r.max.x, r.max.y])
}

impl EditorApp {
    /// Called by the Layer row while drawing (only when the probe is active).
    pub(crate) fn record_row_probe(
        &self,
        l: &editor_service::LayerInfo,
        row: egui::Rect,
        rects: &[(&'static str, egui::Rect)],
        tooltip_shown: bool,
    ) {
        // Controls that must never overlap each other.
        const CONTROLS: [&str; 8] = [
            "indicator",
            "drag",
            "color",
            "name",
            "visible",
            "locked",
            "mode",
            "more",
        ];
        let find = |k: &str| rects.iter().find(|(n, _)| *n == k).map(|(_, r)| *r);
        let mut overlaps = Vec::new();
        for (i, a) in CONTROLS.iter().enumerate() {
            for b in &CONTROLS[i + 1..] {
                if let (Some(ra), Some(rb)) = (find(a), find(b)) {
                    let i = ra.intersect(rb);
                    if i.width() > 0.5 && i.height() > 0.5 {
                        overlaps.push(format!("{a}/{b}"));
                    }
                }
            }
        }
        let name = find("name");
        let text = find("name_text");
        let ellipsized = matches!((name, text), (Some(n), Some(t)) if t.width() > n.width() + 0.5);
        let mut controls = serde_json::Map::new();
        for (k, r) in rects {
            controls.insert((*k).into(), rect_json(*r));
        }
        let all_inside = rects
            .iter()
            .filter(|(k, _)| CONTROLS.contains(k))
            .all(|(_, r)| row.expand(0.5).contains_rect(*r));
        self.row_probes.borrow_mut().push(json!({
            "layer_id": l.layer_id,
            "name": l.display_name,
            "name_chars": l.display_name.chars().count(),
            "row": rect_json(row),
            "row_height": row.height(),
            "controls": controls,
            "overlaps": overlaps,
            "all_controls_inside_row": all_inside,
            "name_ellipsized": ellipsized,
            "tooltip_shown": tooltip_shown,
        }));
    }

    /// End of every frame: observe, then serve screenshot requests.
    pub(crate) fn probe_frame(&mut self, ctx: &egui::Context) {
        if self.probe.is_none() {
            return;
        }
        let rows: Vec<Value> = self.row_probes.borrow_mut().drain(..).collect();
        let dialog = self.layer_dialog.as_ref().map(|d| match d {
            crate::layer_panel::LayerDialog::Rename { layer, text } => {
                json!({"kind": "Rename", "layer": layer, "text": text})
            }
            crate::layer_panel::LayerDialog::Settings { layer, name } => {
                json!({"kind": "Settings", "layer": layer, "name": name})
            }
            crate::layer_panel::LayerDialog::Categories { layer } => {
                json!({"kind": "Categories", "layer": layer})
            }
            crate::layer_panel::LayerDialog::DeletePending { layer } => {
                json!({"kind": "DeletePending", "layer": layer})
            }
            crate::layer_panel::LayerDialog::Delete {
                layer,
                acknowledged,
            } => {
                let summary = self.pending_summary.as_ref();
                json!({
                    "kind": "Delete", "layer": layer, "acknowledged": acknowledged,
                    "risk": summary.map(|s| format!("{:?}", s.risk)),
                    "objects": summary.map(|s| s.summary.object_count),
                    "modified": summary.map(|s| s.summary.modified_object_count),
                    "generated": summary.map(|s| s.summary.generated_object_count),
                })
            }
        });
        let layers: Vec<Value> = self
            .view
            .layers
            .iter()
            .map(|l| {
                json!({
                    "layer_id": l.layer_id, "name": l.display_name, "z_index": l.z_index,
                    "active": l.is_active, "solo": l.is_solo,
                    "visible": l.visible, "effective_visible": l.effective_visible,
                    "locked": l.locked, "selectable": l.selectable,
                    "display_mode": format!("{:?}", l.display_mode),
                    "color_mode": format!("{:?}", l.color_mode),
                    "color": l.base_color.to_hex(), "objects": l.object_count,
                    "classes": l.classes.iter().map(|c| json!({
                        "class": format!("{:?}", c.class), "visible": c.visible,
                        "selectable": c.selectable, "locked": c.locked,
                        "color_override": c.color_override.as_ref().map(|c| c.to_hex()),
                        "effective_color": c.effective_color.to_hex(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let info = self.view.info.as_ref();
        let raw_focus = ctx.input(|i| i.viewport().focused);
        let block_zero_width = self.view.layers.iter().any(|layer| {
            layer.display_name == "block-fixture"
                && layer.display_mode == editor_core::workspace::LayerDisplayMode::ZeroWidth
        });
        let block_zero_width_modes =
            self.view
                .scene
                .as_ref()
                .filter(|_| block_zero_width)
                .map(|scene| {
                    let centerline = scene
                        .objects
                        .iter()
                        .filter(|object| object.style[1] == crate::display::MODE_CENTERLINE)
                        .count();
                    let edge = scene
                        .objects
                        .iter()
                        .filter(|object| object.style[1] == crate::display::MODE_EDGE)
                        .count();
                    json!({"centerline": centerline, "edge": edge})
                });
        let mut observation = json!({
            "ppp": ctx.pixels_per_point(),
            "screen_points": [ctx.content_rect().width(), ctx.content_rect().height()],
            "focused": raw_focus,
            "layer_panel": rect_json(self.layer_panel_rect),
            "layer_panel_width": self.layer_panel_rect.width(),
            "canvas": rect_json(self.canvas_rect),
            "camera_scale": self.camera.scale,
            "camera_center_mm": [self.camera.center.x_mm, self.camera.center.y_mm],
            "manufacturing_revision": info.map(|i| i.revision.clone()),
            "workspace_revision": info.map(|i| i.workspace_revision.clone()),
            "dirty": info.map(|i| i.dirty),
            "project_dirty": info.map(|i| i.project_dirty),
            "project_filename": info.and_then(|i| i.project_path.as_ref()).and_then(|p| {
                std::path::Path::new(p).file_name().map(|name| name.to_string_lossy().into_owned())
            }),
            "close_prompt": self.close_prompt,
            "recovery_prompt": self.recovery_candidate.is_some(),
            "undo_entries": info.map(|i| i.undo_entries),
            "redo_entries": info.map(|i| i.redo_entries),
            "active_layer": info.and_then(|i| i.active_layer_id.clone()),
            "solo_layer": info.and_then(|i| i.solo_layer_id.clone()),
            "display_order": info.map(|i| i.display_order.clone()),
            "layers": layers,
            "rows": rows,
            "layer_dialog": dialog,
            "modal_open": self.modal.is_some(),
            "recent_colors": self.recent_colors,
            "toast": self.toast.as_ref().map(|t| t.0.clone()),
            "selected_objects": self.view.selected.ordered.len(),
            "selected_object_ids": self.view.selected.ids(),
            "selected_block_instances": self.view.selected.ordered.iter().filter(|item| {
                matches!(item.object.geometry, editor_core::SemanticGeometry::BlockInstance { .. })
            }).count(),
            "block_zero_width_stroke_centerline": block_zero_width_modes
                .as_ref()
                .is_some_and(|counts| counts["centerline"].as_u64().is_some_and(|count| count > 0)),
            "block_zero_width_flash_region_edge": block_zero_width_modes
                .as_ref()
                .is_some_and(|counts| counts["edge"].as_u64().is_some_and(|count| count > 0)),
            "block_zero_width_mode_counts": block_zero_width_modes,
            "blocked": self.view.blocked,
            "display_error": self.display_error,
            "has_last_good_frame": self.last_good.is_some(),
            "service_error": self.view.error.as_ref().map(|e| format!("{}: {}", e.code, e.message)),
            "busy": self.busy,
            "dropped_file_batches": self.probe.as_ref().map_or(0, |p| p.drops),
        });
        observation["grip"] = json!({
            "features": crate::grip::features(&self.view).ok().map(|features| features.into_iter().map(|f| {
                let p = self.camera.screen(f.position_mm, self.canvas_rect);
                json!({"id": f.id, "screen": [p.x,p.y]})
            }).collect::<Vec<_>>()),
            "active": self.grip.as_ref().map(|g| json!({"id": g.id, "valid_preview":g.preview.is_ok(), "moved":g.moved})),
            "marker_physical_px": crate::grip::MARKER_PX,
            "hit_physical_px": crate::grip::HIT_PX,
        });
        observation["display_unit"] = json!(self.display_unit.suffix());
        observation["grid_visible"] = json!(self.grid.visible);
        observation["grid_spacing_mm"] = json!(self.grid.spacing_mm);
        observation["object_snap"] = json!({
            "enabled": self.object_snap.enabled,
            "enabled_kinds": self.object_snap.enabled_kinds.iter().map(|kind| format!("{kind:?}")).collect::<Vec<_>>(),
            "radius_physical_px": self.object_snap.radius_px,
            "manufacturing_boundary": self.object_snap.manufacturing_boundary,
            "original_path": self.object_snap.original_path,
            "current": self.object_snap_runtime.current.as_ref().map(|snap| json!({
                "kind": snap.kind.map(|kind| format!("{kind:?}")),
                "source": snap.source.map(|source| format!("{source:?}")),
                "world_mm": [snap.point.x_mm, snap.point.y_mm],
                "screen_distance_px": snap.distance_px,
                "from_grid": snap.from_grid,
                "candidate": snap.candidate.as_ref().map(|candidate| format!("{candidate:?}")),
            })),
            "stats": {
                "nearby_objects": self.object_snap_runtime.stats.nearby_objects,
                "features_generated": self.object_snap_runtime.stats.features_generated,
                "intersection_pairs": self.object_snap_runtime.stats.intersection_pairs,
                "acquire_radius_px": self.object_snap.radius_px,
                "release_radius_px": self.object_snap.radius_px + editor_core::snap::SnapResolver::default().release_extra_px,
                "candidate_query_us": self.object_snap_runtime.stats.candidate_query_us,
                "resolver_us": self.object_snap_runtime.stats.resolver_us,
                "retained_previous": self.object_snap_runtime.stats.retained_previous,
                "elapsed_us": self.object_snap_runtime.stats.elapsed_us,
            }
        });
        observation["text_display_unit"] = json!(self.text.display_unit.suffix());
        observation["measure_label"] = json!(self.measure.label_with_resolution(
            self.display_unit,
            info.map_or(0.0001, |i| i.manufacturing_precision.resolution_mm),
        ));
        let raw_grip_target = self
            .grip
            .as_ref()
            .and_then(|_| ctx.input(|input| input.pointer.interact_pos()))
            .map(|position| self.camera.world(position, self.canvas_rect));
        if let Some(synthetic) = self.probe.as_ref().and_then(|probe| {
            probe.synthetic_grip_observation(
                &self.view,
                self.grip.as_ref(),
                raw_grip_target,
                self.object_snap_runtime.current.as_ref(),
            )
        }) {
            observation["synthetic_grip"] = synthetic;
        }
        if let Some(probe) = self.probe.as_mut() {
            probe.observe(observation);
            probe.screenshots(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn synthetic_grip_observation_requires_explicit_fixture_provenance() {
        let hash = fixture_sha256();
        let file_name = "grips.gbr";
        assert!(synthetic_fixture_gate(
            Some(&hash),
            &hash,
            file_name,
            &hash,
            1,
        ));
        assert!(!synthetic_fixture_gate(None, &hash, file_name, &hash, 1));
        assert!(!synthetic_fixture_gate(
            Some("wrong"),
            &hash,
            file_name,
            &hash,
            1
        ));
        assert!(!synthetic_fixture_gate(
            Some(&hash),
            &hash,
            "customer.gbr",
            &hash,
            1,
        ));
        assert!(!synthetic_fixture_gate(
            Some(&hash),
            &hash,
            file_name,
            "different-layer-hash",
            1,
        ));
        assert!(!synthetic_fixture_gate(
            Some(&hash),
            &hash,
            file_name,
            &hash,
            2,
        ));
    }

    #[test]
    fn model_import_of_embedded_grip_fixture_enables_observation_log() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s4c2/grips.gbr");
        let mut model = crate::state::Model::default();
        model.import_gerbers(&[fixture]).unwrap();

        let layer = &model.view.layers[0];
        let provenance = layer.provenance.as_ref().unwrap();
        assert_eq!(provenance.original_file_name, S4C2_GRIP_FIXTURE_NAME);
        assert_eq!(provenance.imported_sha256, fixture_sha256());
        assert!(model.view.info.as_ref().unwrap().source_path.is_empty());

        let dir = std::env::temp_dir().join(format!(
            "rcam-native-probe-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut probe = Probe {
            dir: dir.clone(),
            started: Instant::now(),
            last: String::new(),
            pending_shot: None,
            polled: Instant::now(),
            fixture_sha256: fixture_sha256(),
            synthetic_grip_opt_in: true,
            drops: 0,
        };
        let synthetic = probe
            .synthetic_grip_observation(&model.view, None, None, None)
            .expect("GUI-style fixture import should enable synthetic observations");
        probe.observe(json!({"synthetic_grip": synthetic}));
        let log = std::fs::read_to_string(dir.join("native_observations.jsonl")).unwrap();
        assert!(log.contains("\"synthetic_grip\""));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
