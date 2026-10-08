//! Transient candidate worker, explicitly adopted selection and bounds overlay.
use crate::{
    EditorApp,
    block_ui::Context,
    state::{Action, Classifier, Model},
    ui::tokens,
};
use editor_core::workspace::LayerKind;
use editor_service::*;
use eframe::egui;
use std::{collections::HashSet, sync::Arc};
#[derive(Clone, PartialEq)]
pub struct Request {
    pub context: Context,
    pub query: NearbyManufacturingQuery,
}
#[derive(Clone)]
pub struct Reply {
    pub request: Request,
    pub page: ManufacturingCandidatePage,
    pub objects: Vec<Vec<ObjectInfo>>,
}
pub struct UiState {
    #[cfg(feature = "internal-evidence")]
    pub native: Option<crate::native_d2::Run>,
    pub width_mm: f64,
    pub height_mm: f64,
    pub layers: Vec<String>,
    pub requested: Option<Request>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            #[cfg(feature = "internal-evidence")]
            native: crate::native_d2::Run::from_env(),
            width_mm: 10.,
            height_mm: 10.,
            layers: vec![],
            requested: None,
        }
    }
}
fn stale() -> ServiceError {
    ServiceError {
        code: "REVISION_CONFLICT".into(),
        message: "候选查询已过期，请重新查找".into(),
        details: serde_json::json!({}),
    }
}
impl Model {
    pub fn candidate_query(&mut self, request: Request) -> Result<(), ServiceError> {
        if !request.context.valid(&self.view) {
            return Err(stale());
        }
        self.view.candidate_reply = None;
        let mut q = request.query.clone();
        q.offset = 0;
        let mut page = self
            .service
            .components_nearby_manufacturing(&request.context.document, &q)?;
        while page.items.len() < page.total {
            q.offset = page.items.len();
            let next = self
                .service
                .components_nearby_manufacturing(&request.context.document, &q)?;
            if next.items.is_empty() {
                return Err(stale());
            }
            page.items.extend(next.items);
        }
        let objects = self.service.candidate_member_objects(
            &request.context.document,
            &q.revision,
            &page.items,
        )?;
        self.view.candidate_reply = Some(Arc::new(Reply {
            request,
            page,
            objects,
        }));
        Ok(())
    }
    pub fn candidate_select(&mut self, request: Request, add: bool) -> Result<(), ServiceError> {
        if !request.context.valid(&self.view) {
            return Err(stale());
        }
        let reply = self
            .view
            .candidate_reply
            .as_ref()
            .filter(|r| r.request == request)
            .ok_or_else(stale)?;
        let classifier = Classifier::new(&self.view.layers, &self.view.apertures);
        let mut selected = if add {
            self.view.selected.ordered.clone()
        } else {
            Default::default()
        };
        let mut seen: HashSet<_> = selected
            .iter()
            .map(|o| (o.layer_id.clone(), o.object.object_id.clone()))
            .collect();
        let mut skipped = 0;
        let mut accepted = 0;
        for group in &reply.objects {
            // Whole text group or none: no partial glyph selection.
            if !group.iter().all(|o| classifier.selectable(o)) {
                skipped += 1;
                continue;
            }
            accepted += 1;
            for o in group {
                if seen.insert((o.layer_id.clone(), o.object.object_id.clone())) {
                    selected.push(o.clone());
                }
            }
        }
        // Existing tools expect source exposure order and complete text spans.
        if let Some(snapshot) = &self.view.snap_snapshot {
            let order: std::collections::HashMap<_, _> = snapshot
                .layers
                .iter()
                .enumerate()
                .flat_map(|(l, layer)| {
                    let seen = &seen;
                    layer.objects.iter().enumerate().filter_map(move |(i, o)| {
                        seen.contains(&(layer.id.clone(), o.object_id.clone()))
                            .then_some(((layer.id.clone(), o.object_id.clone()), (l, i)))
                    })
                })
                .collect();
            selected.sort_by_key(|o| {
                order
                    .get(&(o.layer_id.clone(), o.object.object_id.clone()))
                    .copied()
            });
        }
        self.view.selected.ordered = selected;
        rcam_diagnostics::measurements(
            rcam_diagnostics::Level::Info,
            "components.candidates.select",
            &[
                ("requested_count", reply.objects.len() as u64),
                ("selected_count", accepted),
                ("skipped_nonselectable", skipped),
                ("mode_add", u64::from(add)),
            ],
        );
        Ok(())
    }
}
impl EditorApp {
    fn candidate_request(&self) -> Option<Request> {
        let context = Context::capture(&self.view)?;
        let c = self.components.focused.as_ref()?;
        let layers = if self.components.candidates.layers.is_empty() {
            self.view
                .layers
                .iter()
                .find(|l| {
                    l.is_active && l.effective_visible && l.visible && l.kind == LayerKind::Gerber
                })
                .map(|l| vec![l.layer_id.clone()])
                .unwrap_or_default()
        } else {
            self.components.candidates.layers.clone()
        };
        Some(Request {
            query: NearbyManufacturingQuery {
                revision: context.revision.clone(),
                component_id: c.component.id.0.clone(),
                layer_ids: layers,
                window: ManufacturingSearchWindow::ComponentLocalRect {
                    width_mm: self.components.candidates.width_mm,
                    height_mm: self.components.candidates.height_mm,
                },
                offset: 0,
                limit: 500,
            },
            context,
        })
    }
    pub(crate) fn request_candidates(&mut self) {
        if let Some(request) = self.candidate_request() {
            self.components.candidates.requested = Some(request.clone());
            self.send(Action::CandidateQuery(request));
        }
    }
    fn candidate_reply(&self) -> Option<Arc<Reply>> {
        self.view
            .candidate_reply
            .as_ref()
            .filter(|r| {
                r.request.context.valid(&self.view)
                    && Some(&r.request) == self.components.candidates.requested.as_ref()
                    && Some(r.request.clone()) == self.candidate_request()
            })
            .cloned()
    }
    pub(crate) fn candidate_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label("附近制造候选 · 包围盒候选，不代表最终开口或元件归属");
        let before = (
            self.components.candidates.width_mm,
            self.components.candidates.height_mm,
            self.components.candidates.layers.clone(),
        );
        ui.horizontal(|ui| {
            for (label, mm) in [
                ("宽度", &mut self.components.candidates.width_mm),
                ("高度", &mut self.components.candidates.height_mm),
            ] {
                let mut value = *mm / self.display_unit.mm_per_unit();
                ui.label(label);
                if ui
                    .add(
                        egui::DragValue::new(&mut value)
                            .speed(0.1)
                            .suffix(self.display_unit.suffix()),
                    )
                    .changed()
                {
                    *mm = value * self.display_unit.mm_per_unit();
                }
            }
        });
        ui.label("图层范围（默认当前活动层；可明确选择可见层）");
        for layer in self
            .view
            .layers
            .clone()
            .into_iter()
            .filter(|l| l.visible && l.effective_visible && l.kind == LayerKind::Gerber)
        {
            let mut selected = self.components.candidates.layers.contains(&layer.layer_id);
            if ui.checkbox(&mut selected, layer.display_name).changed() {
                if selected {
                    self.components.candidates.layers.push(layer.layer_id);
                } else {
                    self.components
                        .candidates
                        .layers
                        .retain(|id| id != &layer.layer_id);
                }
            }
        }
        if before
            != (
                self.components.candidates.width_mm,
                self.components.candidates.height_mm,
                self.components.candidates.layers.clone(),
            )
        {
            self.components.candidates.requested = None;
        }
        if ui
            .add_enabled(
                self.components.focused.is_some(),
                egui::Button::new("查找附近制造候选"),
            )
            .clicked()
        {
            self.request_candidates();
        }
        if let Some(r) = self.candidate_reply() {
            ui.label(format!("{} 个候选 · 未建立关联", r.page.total));
            ui.horizontal(|ui| {
                if ui.button("替换选择（全部候选）").clicked() {
                    self.send(Action::CandidateSelect(r.request.clone(), false));
                }
                if ui.button("添加到选择").clicked() {
                    self.send(Action::CandidateSelect(r.request.clone(), true));
                }
                if ui.button("适合组件区域").clicked() {
                    self.camera
                        .fit(Some(r.page.window.bounds()), self.canvas_rect);
                    self.fit = false;
                }
            });
            let classifier = Classifier::new(&self.view.layers, &self.view.apertures);
            egui::ScrollArea::vertical()
                .id_salt("manufacturing-candidates")
                .max_height(160.)
                .show_rows(ui, 24., r.page.items.len(), |ui, range| {
                    for i in range {
                        let c = &r.page.items[i];
                        let selectable = r.objects[i].iter().all(|o| classifier.selectable(o));
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} · {} · {:?} · {:.3} mm{}",
                                c.object_id,
                                c.geometry_kind,
                                c.exposure,
                                c.center_distance_mm,
                                if selectable { "" } else { " · 不可选择" }
                            ));
                            if ui.small_button("定位").clicked() {
                                self.camera.fit(Some(c.world_bounds), self.canvas_rect);
                                self.fit = false;
                            }
                        });
                    }
                });
        }
    }
    pub(crate) fn paint_candidates(&self, painter: &egui::Painter, rect: egui::Rect, ppp: f32) {
        let Some(r) = self.candidate_reply() else {
            return;
        };
        let stroke =
            egui::Stroke::new(tokens::CANDIDATE_OUTLINE_PX / ppp, tokens::CANDIDATE_WINDOW);
        let corners = r.page.window.corners().map(|p| self.camera.screen(p, rect));
        for i in 0..4 {
            painter.line_segment([corners[i], corners[(i + 1) % 4]], stroke);
        }
        for c in &r.page.items {
            let b = c.world_bounds;
            let screen = egui::Rect::from_two_pos(
                self.camera
                    .screen(editor_core::MmPoint::new(b.min_x_mm, b.min_y_mm), rect),
                self.camera
                    .screen(editor_core::MmPoint::new(b.max_x_mm, b.max_y_mm), rect),
            );
            if screen.intersects(rect) {
                painter.rect_stroke(
                    screen,
                    0.,
                    egui::Stroke::new(tokens::CANDIDATE_OUTLINE_PX / ppp, tokens::CANDIDATE_BOUNDS),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{board::*, pnp::*};
    use std::path::PathBuf;
    fn setup() -> Model {
        let mut m = Model::default();
        m.run(Action::NewWorkspace);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/s4d1");
        m.run(Action::ImportGerbers(vec![root.join("board.gbr")]));
        let context = Context::capture(&m.view).unwrap();
        let req = crate::components_ui::PreviewRequest {
            context: context.clone(),
            path: root.join("pnp.csv"),
            mapping: crate::components_ui::UiState::default().mapping,
        };
        m.run(Action::PnpPreview(req.clone()));
        let hash = m.view.pnp_preview.as_ref().unwrap().result.sha256.clone();
        m.run(Action::PnpImport(
            context,
            ImportPnpParams {
                path: req.path.to_string_lossy().into(),
                mapping: req.mapping,
                preview_sha256: hash,
                allow_replace: false,
            },
        ));
        assert!(m.view.error.is_none());
        m
    }
    fn request(m: &Model) -> Request {
        let context = Context::capture(&m.view).unwrap();
        Request {
            query: NearbyManufacturingQuery {
                revision: context.revision.clone(),
                component_id: m.view.board.as_ref().unwrap().components[0].id.0.clone(),
                layer_ids: vec![m.view.layers[0].layer_id.clone()],
                window: ManufacturingSearchWindow::ComponentLocalRect {
                    width_mm: 30.,
                    height_mm: 30.,
                },
                offset: 0,
                limit: 500,
            },
            context,
        }
    }
    fn register(m: &mut Model) {
        m.run(Action::BoardRegistration(
            Context::capture(&m.view).unwrap(),
            RegistrationInput::Manual {
                transform: CoordinateTransform2D::IDENTITY,
            },
        ));
        assert!(m.view.error.is_none());
    }
    #[test]
    fn candidate_worker_permissions_replace_add_move_undo_and_revision_fence() {
        let mut m = setup();
        let before = m.view.info.clone();
        m.run(Action::CandidateQuery(request(&m)));
        assert_eq!(m.view.error.as_ref().unwrap().code, "REGISTRATION_REQUIRED");
        assert_eq!(m.view.info, before);
        assert!(m.view.selected.ordered.is_empty());
        register(&mut m);
        let req = request(&m);
        m.run(Action::CandidateQuery(req.clone()));
        assert!(m.view.error.is_none());
        assert_eq!(m.view.candidate_reply.as_ref().unwrap().page.total, 4);
        assert!(m.view.selected.ordered.is_empty());
        let before = m.view.info.clone();
        m.run(Action::CandidateSelect(req.clone(), false));
        assert_eq!(m.view.selected.ordered.len(), 4);
        assert_eq!(m.view.info, before);
        m.run(Action::CandidateSelect(req.clone(), true));
        assert_eq!(m.view.selected.ordered.len(), 4);
        m.run(Action::Move("1".into(), "0".into()));
        assert!(m.view.error.is_none());
        assert!(m.view.candidate_reply.is_none());
        m.run(Action::History(false));
        assert!(m.view.error.is_none());
        assert_eq!(
            m.view.info.as_ref().unwrap().dirty,
            before.as_ref().unwrap().dirty
        );
        m.run(Action::CandidateSelect(req, false));
        assert_eq!(m.view.error.as_ref().unwrap().code, "REVISION_CONFLICT");
        let info = m.view.info.clone().unwrap();
        m.service
            .layers_update_many(
                &info.document_id,
                &info.revision,
                UpdateLayersParams {
                    expected_workspace_revision: info.workspace_revision,
                    updates: vec![LayerPatch {
                        layer_id: m.view.layers[0].layer_id.clone(),
                        selectable: Some(false),
                        ..Default::default()
                    }],
                },
            )
            .unwrap();
        m.refresh(false).unwrap();
        let req = request(&m);
        m.run(Action::CandidateQuery(req.clone()));
        assert_eq!(m.view.candidate_reply.as_ref().unwrap().page.total, 4);
        m.run(Action::CandidateSelect(req, false));
        assert!(m.view.selected.ordered.is_empty());
        let info = m.view.info.clone().unwrap();
        m.service
            .layers_update_many(
                &info.document_id,
                &info.revision,
                UpdateLayersParams {
                    expected_workspace_revision: info.workspace_revision,
                    updates: vec![LayerPatch {
                        layer_id: m.view.layers[0].layer_id.clone(),
                        visible: Some(false),
                        selectable: Some(true),
                        ..Default::default()
                    }],
                },
            )
            .unwrap();
        m.refresh(false).unwrap();
        let req = request(&m);
        m.run(Action::CandidateQuery(req.clone()));
        assert_eq!(m.view.candidate_reply.as_ref().unwrap().page.total, 4);
        m.run(Action::CandidateSelect(req, false));
        assert!(m.view.selected.ordered.is_empty());
        let info = m.view.info.clone().unwrap();
        m.service
            .layers_update_many(
                &info.document_id,
                &info.revision,
                UpdateLayersParams {
                    expected_workspace_revision: info.workspace_revision,
                    updates: vec![LayerPatch {
                        layer_id: m.view.layers[0].layer_id.clone(),
                        selectable: Some(true),
                        visible: Some(true),
                        locked: Some(true),
                        ..Default::default()
                    }],
                },
            )
            .unwrap();
        m.refresh(false).unwrap();
        let req = request(&m);
        m.run(Action::CandidateQuery(req.clone()));
        m.run(Action::CandidateSelect(req, false));
        assert_eq!(m.view.selected.ordered.len(), 4);
        let before = m.view.info.clone();
        m.run(Action::Move("1".into(), "0".into()));
        assert_eq!(m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
        assert_eq!(m.view.info, before);
    }
    #[test]
    fn candidate_ui_discards_settings_component_and_document_stale_and_overlay_is_pure() {
        let mut m = setup();
        register(&mut m);
        m.run(Action::CandidateQuery(request(&m)));
        let reply = m.view.candidate_reply.clone().unwrap();
        let mut app = crate::modal::tests::app();
        app.view = m.view.clone();
        app.components.focused = Some(component_info(
            &app.view.board.as_ref().unwrap().components[0],
            app.view.board.as_ref().unwrap(),
        ));
        app.components.candidates.width_mm = 30.;
        app.components.candidates.height_mm = 30.;
        app.components.candidates.requested = Some(reply.request.clone());
        assert!(app.candidate_reply().is_some());
        let before = app.view.info.clone();
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        for ppp in [1., 2., 3.] {
            for scale in [1., 100., 1000.] {
                app.camera.scale = scale;
                let output = ctx.run(Default::default(), |ctx| {
                    let painter = ctx.layer_painter(egui::LayerId::background());
                    app.paint_candidates(&painter, rect, ppp);
                });
                let outlines: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| {
                        if let egui::Shape::LineSegment { stroke, .. } = &s.shape {
                            (stroke.color == tokens::CANDIDATE_WINDOW).then_some(stroke.width)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(outlines.len(), 4);
                assert!(
                    outlines
                        .iter()
                        .all(|w| (w * ppp - tokens::CANDIDATE_OUTLINE_PX).abs() < 1e-6)
                );
            }
        }
        assert_eq!(app.view.info, before);
        assert!(app.view.selected.ordered.is_empty());
        app.components.candidates.width_mm = 20.;
        assert!(app.candidate_reply().is_none());
        app.components.candidates.width_mm = 30.;
        app.components.focused = Some(component_info(
            &app.view.board.as_ref().unwrap().components[1],
            app.view.board.as_ref().unwrap(),
        ));
        assert!(app.candidate_reply().is_none());
        app.view.info.as_mut().unwrap().document_id = "other".into();
        assert!(app.candidate_reply().is_none());
    }
    #[test]
    fn candidate_selection_drives_existing_align_array_block_and_grip_transactions() {
        for tool in ["align", "array", "block"] {
            let mut m = setup();
            register(&mut m);
            let req = request(&m);
            m.run(Action::CandidateQuery(req.clone()));
            m.run(Action::CandidateSelect(req, false));
            assert_eq!(m.view.selected.ordered.len(), 4);
            let baseline = m.view.snap_snapshot.clone().unwrap();
            let before = m.view.info.clone().unwrap();
            let ids = m
                .view
                .selected
                .ids()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let layer = m.view.layers[0].layer_id.clone();
            match tool {
                "align" => {
                    assert!(crate::state::arrangement_eligibility(&m.view).align);
                    m.run(Action::Align(AlignmentMode::Left));
                }
                "array" => {
                    assert!(crate::array_ui::eligible(&m.view));
                    m.run(Action::ArrayApply(Box::new(crate::array_ui::Request {
                        context: Context::capture(&m.view).unwrap(),
                        params: ArrayRectangularParams {
                            layer_id: layer,
                            object_ids: ids,
                            rows: 2,
                            columns: 2,
                            pitch_x_mm: 40.,
                            pitch_y_mm: 40.,
                        },
                        ppm: m.ppm,
                    })));
                }
                _ => m.run(Action::BlockEdit(Box::new(crate::block_ui::Request {
                    context: Context::capture(&m.view).unwrap(),
                    edit: crate::block_ui::Edit::Create(CreateBlockDefinitionParams {
                        layer_id: layer,
                        object_ids: ids,
                        local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
                        name: "Candidates".into(),
                    }),
                }))),
            }
            assert!(m.view.error.is_none(), "{tool}: {:?}", m.view.error);
            assert_eq!(
                m.view.info.as_ref().unwrap().undo_entries,
                before.undo_entries + 1
            );
            m.run(Action::History(false));
            assert!(m.view.error.is_none());
            assert_eq!(
                m.view.snap_snapshot.as_ref().unwrap().layers,
                baseline.layers
            );
        }
        let mut m = setup();
        register(&mut m);
        let mut req = request(&m);
        req.query.component_id = m.view.board.as_ref().unwrap().components[1].id.0.clone();
        req.query.window = ManufacturingSearchWindow::ComponentLocalRect {
            width_mm: 2.,
            height_mm: 2.,
        };
        m.run(Action::CandidateQuery(req.clone()));
        m.run(Action::CandidateSelect(req, false));
        assert_eq!(m.view.selected.ordered.len(), 1);
        let features = crate::grip::features(&m.view).unwrap();
        let center = features
            .iter()
            .find(|f| f.id == editor_core::grip::GripFeatureId::Radius)
            .unwrap();
        let mut session = crate::grip::Session::arm(&m.view, center.id).unwrap();
        let before = m.view.info.clone().unwrap();
        let baseline = m.view.snap_snapshot.clone().unwrap();
        session.update(editor_core::MmPoint::new(
            center.position_mm.x_mm + 1.,
            center.position_mm.y_mm,
        ));
        m.run(session.release().unwrap());
        assert!(m.view.error.is_none());
        assert_eq!(
            m.view.info.as_ref().unwrap().undo_entries,
            before.undo_entries + 1
        );
        m.run(Action::History(false));
        assert_eq!(
            m.view.snap_snapshot.as_ref().unwrap().layers,
            baseline.layers
        );
    }
}
