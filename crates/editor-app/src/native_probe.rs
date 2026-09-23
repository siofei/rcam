//! Opt-in native evidence probe (`RCAM_NATIVE_PROBE_DIR=<dir>`).
//!
//! It only *observes*: every frame whose observable state changed appends one JSON
//! line to `native_observations.jsonl` (layers, revisions, dialogs, the exact screen
//! rectangle of each Layer-row control, pixels-per-point, panel width...), every
//! action sent to the service is appended to `native_actions.log`, and a screenshot
//! is written as `screens/<label>.ppm` when a `shot.request` file containing the label
//! appears in the directory. Nothing here changes application or manufacturing state.
use crate::{EditorApp, state::Action};
use eframe::egui;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Probe {
    dir: PathBuf,
    started: Instant,
    last: String,
    pending_shot: Option<String>,
    polled: Instant,
    pub drops: u32,
}

impl Probe {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("RCAM_NATIVE_PROBE_DIR")?);
        std::fs::create_dir_all(dir.join("screens")).ok()?;
        let probe = Self {
            dir,
            started: Instant::now(),
            last: String::new(),
            pending_shot: None,
            polled: Instant::now(),
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

/// Short, stable description of the action sent to the service.
pub fn action_text(a: &Action) -> String {
    match a {
        Action::Open(p) => format!("Open {}", p.display()),
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
        Action::Move(dx, dy) => format!("Move {dx} {dy}"),
        Action::Rotate(..) => "Rotate".into(),
        Action::Mirror(_) => "Mirror".into(),
        Action::Duplicate => "Duplicate".into(),
        Action::Delete => "DeleteObjects".into(),
        Action::History(redo) => format!("History {}", if *redo { "redo" } else { "undo" }),
        Action::Save(path, layer, _) => format!("ExportLayer {layer} -> {}", path.display()),
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
        let observation = json!({
            "ppp": ctx.pixels_per_point(),
            "screen_points": [ctx.content_rect().width(), ctx.content_rect().height()],
            "focused": raw_focus,
            "layer_panel": rect_json(self.layer_panel_rect),
            "layer_panel_width": self.layer_panel_rect.width(),
            "canvas": rect_json(self.canvas_rect),
            "camera_scale": self.camera.scale,
            "manufacturing_revision": info.map(|i| i.revision.clone()),
            "workspace_revision": info.map(|i| i.workspace_revision.clone()),
            "dirty": info.map(|i| i.dirty),
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
        if let Some(probe) = self.probe.as_mut() {
            probe.observe(observation);
            probe.screenshots(ctx);
        }
    }
}
