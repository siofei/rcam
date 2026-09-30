//! Transient PnP mapping, query, registration picking and nonmanufacturing overlay.
use crate::{
    EditorApp,
    block_ui::Context,
    modal::ActiveModal,
    state::{Action, Model},
};
use editor_core::{MmPoint, board::*, pnp::*};
use editor_service::*;
use eframe::egui;
use std::{path::PathBuf, sync::Arc};
#[derive(Clone, PartialEq)]
pub struct PreviewRequest {
    pub context: Context,
    pub path: PathBuf,
    pub mapping: PnpMapping,
}
#[derive(Clone)]
pub struct PreviewReply {
    pub request: PreviewRequest,
    pub result: PnpFilePreview,
}
pub struct UiState {
    #[cfg(feature = "internal-evidence")]
    pub native: Option<crate::native_d1::Run>,
    pub open: bool,
    pub overlay: bool,
    pub path: Option<PathBuf>,
    pub mapping: PnpMapping,
    pub units_confirmed: bool,
    pub convention_confirmed: bool,
    pub replace: bool,
    pub query: String,
    pub mode: RefdesMatch,
    pub side: Option<BoardSide>,
    pub footprint: String,
    pub focused: Option<ComponentInfo>,
    pub focus_revision: String,
    pub manual: bool,
    pub reflect: bool,
    pub angle: f64,
    pub translation: [f64; 2],
    pub board_points: [[f64; 2]; 2],
    pub world_points: [[f64; 2]; 2],
    pub pick: Option<usize>,
    pub registration_confirmed: bool,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            #[cfg(feature = "internal-evidence")]
            native: crate::native_d1::Run::from_env(),
            open: false,
            overlay: true,
            path: None,
            mapping: PnpMapping {
                delimiter: Delimiter::Csv,
                unit: PnpUnit::Mm,
                refdes: 0,
                x: 1,
                y: 2,
                rotation: 3,
                side: 4,
                footprint: Some(5),
                value: Some(6),
                top_token: "Top".into(),
                bottom_token: "Bottom".into(),
                clockwise: false,
                rotation_offset_deg: 0.,
                invert_y: false,
            },
            units_confirmed: false,
            convention_confirmed: false,
            replace: false,
            query: String::new(),
            mode: RefdesMatch::Prefix,
            side: None,
            footprint: String::new(),
            focused: None,
            focus_revision: String::new(),
            manual: true,
            reflect: false,
            angle: 0.,
            translation: [0., 0.],
            board_points: [[0., 0.], [10., 0.]],
            world_points: [[0., 0.], [10., 0.]],
            pick: None,
            registration_confirmed: false,
        }
    }
}
impl UiState {
    pub fn registration_input(&self) -> RegistrationInput {
        if self.manual {
            RegistrationInput::Manual {
                transform: CoordinateTransform2D {
                    reflect_x: self.reflect,
                    rotation_deg: self.angle,
                    translation: MmPoint::new(self.translation[0], self.translation[1]),
                },
            }
        } else {
            RegistrationInput::TwoPoint {
                board: self.board_points.map(|p| MmPoint::new(p[0], p[1])),
                world: self.world_points.map(|p| MmPoint::new(p[0], p[1])),
                reflect_x: self.reflect,
            }
        }
    }
}
impl Model {
    pub fn pnp_preview(&mut self, request: PreviewRequest) -> Result<(), ServiceError> {
        if !request.context.valid(&self.view) {
            return Err(ServiceError {
                code: "REVISION_CONFLICT".into(),
                message: "PnP preview is stale".into(),
                details: serde_json::json!({}),
            });
        }
        self.service.grant_file_access(&request.path, false)?;
        let result = self
            .service
            .components_preview_pnp(&request.path.to_string_lossy(), &request.mapping)?;
        self.view.pnp_preview = Some(Arc::new(PreviewReply { request, result }));
        Ok(())
    }
    pub fn pnp_import(
        &mut self,
        context: Context,
        params: ImportPnpParams,
    ) -> Result<(), ServiceError> {
        self.service
            .components_import_pnp(&context.document, &context.revision, params)?;
        self.view.pnp_preview = None;
        self.refresh(true)?;
        self.view.message = "PnP 已导入；Board 未校准".into();
        Ok(())
    }
    pub fn registration_apply(
        &mut self,
        context: Context,
        input: RegistrationInput,
    ) -> Result<(), ServiceError> {
        self.service
            .board_set_registration(&context.document, &context.revision, input)?;
        self.refresh(true)?;
        self.view.message = "Board 配准已提交".into();
        Ok(())
    }
    pub fn component_search(
        &mut self,
        context: Context,
        query: ComponentQuery,
    ) -> Result<(), ServiceError> {
        if !context.valid(&self.view) {
            return Err(ServiceError {
                code: "REVISION_CONFLICT".into(),
                message: "Component query is stale".into(),
                details: serde_json::json!({}),
            });
        }
        self.view.component_indices = self.service.component_indices(&context.document, &query)?;
        self.view.component_query = Some(query);
        Ok(())
    }
}
impl EditorApp {
    pub(crate) fn open_pnp(&mut self, path: PathBuf) {
        self.open_modal(ActiveModal::Pnp);
        if self.modal == Some(ActiveModal::Pnp) {
            self.components.pick = None;
            self.components.path = Some(path);
            self.components.units_confirmed = false;
            self.components.convention_confirmed = false;
            self.components.replace = false;
            self.view.pnp_preview = None;
        }
    }
    pub(crate) fn pnp_modal(&mut self, ui: &mut egui::Ui) {
        ui.label("CSV/TSV · UTF-8 · 首行为列名；必须明确每列、单位、方向和 Side。");
        if let Some(path) = &self.components.path {
            ui.label(path.file_name().unwrap_or_default().to_string_lossy());
        }
        let before = self.components.mapping.clone();
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.components.mapping.delimiter,
                Delimiter::Csv,
                "CSV",
            );
            ui.selectable_value(
                &mut self.components.mapping.delimiter,
                Delimiter::Tsv,
                "TSV",
            );
        });
        ui.horizontal(|ui| {
            ui.label("源单位");
            ui.selectable_value(&mut self.components.mapping.unit, PnpUnit::Mm, "mm");
            ui.selectable_value(&mut self.components.mapping.unit, PnpUnit::Inch, "inch");
        });
        ui.checkbox(&mut self.components.units_confirmed, "确认源单位");
        let headers = self
            .view
            .pnp_preview
            .as_ref()
            .map(|r| r.result.preview.headers.clone())
            .unwrap_or_default();
        ui.label("列号从 1 开始（先预览可查看列名）");
        for (label, index) in [
            ("RefDes", &mut self.components.mapping.refdes),
            ("X", &mut self.components.mapping.x),
            ("Y", &mut self.components.mapping.y),
            ("Rotation", &mut self.components.mapping.rotation),
            ("Side", &mut self.components.mapping.side),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                let mut one = *index + 1;
                ui.add(egui::DragValue::new(&mut one).range(1..=MAX_COLUMNS));
                *index = one - 1;
                if let Some(h) = headers.get(*index) {
                    ui.label(h);
                }
            });
        }
        for (label, index) in [
            ("Footprint", &mut self.components.mapping.footprint),
            ("Value", &mut self.components.mapping.value),
        ] {
            ui.horizontal(|ui| {
                let mut enabled = index.is_some();
                if ui.checkbox(&mut enabled, label).changed() {
                    *index = enabled.then_some(0);
                }
                if let Some(index) = index {
                    let mut one = *index + 1;
                    ui.add(egui::DragValue::new(&mut one).range(1..=MAX_COLUMNS));
                    *index = one - 1;
                    if let Some(h) = headers.get(*index) {
                        ui.label(h);
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Top 值");
            ui.text_edit_singleline(&mut self.components.mapping.top_token);
        });
        ui.horizontal(|ui| {
            ui.label("Bottom 值");
            ui.text_edit_singleline(&mut self.components.mapping.bottom_token);
        });
        ui.checkbox(
            &mut self.components.mapping.clockwise,
            "源旋转为顺时针（转换为 Board CCW）",
        );
        ui.checkbox(
            &mut self.components.mapping.invert_y,
            "源 Y 向下（显式反转 Y 与角度）",
        );
        ui.horizontal(|ui| {
            ui.label("方向偏置 °");
            ui.add(egui::DragValue::new(
                &mut self.components.mapping.rotation_offset_deg,
            ));
        });
        ui.checkbox(
            &mut self.components.convention_confirmed,
            "确认坐标 / 旋转 / Top-Bottom 约定；不自动镜像 Bottom",
        );
        if before != self.components.mapping {
            self.components.units_confirmed = false;
            self.components.convention_confirmed = false;
            self.view.pnp_preview = None;
        }
        if ui.button("预览 / 检查").clicked()
            && let (Some(context), Some(path)) =
                (Context::capture(&self.view), self.components.path.clone())
        {
            self.send(Action::PnpPreview(PreviewRequest {
                context,
                path,
                mapping: self.components.mapping.clone(),
            }));
        }
        let reply = self.view.pnp_preview.clone().filter(|r| {
            r.request.context.valid(&self.view)
                && Some(&r.request.path) == self.components.path.as_ref()
                && r.request.mapping == self.components.mapping
        });
        if let Some(reply) = &reply {
            ui.label(format!(
                "{} 行 · {} 个错误",
                reply.result.preview.row_count, reply.result.preview.diagnostic_count
            ));
            for (i, h) in reply.result.preview.headers.iter().enumerate() {
                ui.label(format!("{}: {}", i + 1, h));
            }
            for d in &reply.result.preview.diagnostics {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("行 {} · {} · {}", d.line, d.field, d.code),
                );
            }
            for c in reply.result.preview.components.iter().take(8) {
                ui.label(format!(
                    "{} {:?} ({:.6}, {:.6}) mm {:.3}°",
                    c.refdes, c.side, c.position.x_mm, c.position.y_mm, c.rotation_deg
                ));
            }
        }
        if self.view.board.is_some() {
            ui.checkbox(
                &mut self.components.replace,
                "确认替换现有组件表（清除旧配准）",
            );
        }
        let can_apply = reply.as_ref().is_some_and(|r| r.result.preview.valid())
            && self.components.units_confirmed
            && self.components.convention_confirmed
            && (self.view.board.is_none() || self.components.replace);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_apply, egui::Button::new("确认导入"))
                .clicked()
                && let Some(reply) = reply
            {
                self.send(Action::PnpImport(
                    reply.request.context.clone(),
                    ImportPnpParams {
                        path: reply.request.path.to_string_lossy().into(),
                        mapping: reply.request.mapping.clone(),
                        preview_sha256: reply.result.sha256.clone(),
                        allow_replace: self.components.replace,
                    },
                ));
                self.modal_pending = Some(self.sequence);
            }
            if ui.button("取消").clicked() {
                self.cancel_modal();
            }
        });
    }
    pub(crate) fn component_window(&mut self, ctx: &egui::Context) {
        if !self.components.open {
            return;
        }
        let mut open = true;
        egui::Window::new("PCB / PnP / RefDes").open(&mut open).default_width(480.).default_height(520.).show(ctx,|ui|{
            ui.add_enabled_ui(!self.busy&&self.modal.is_none(),|ui|{
                if ui.button("Import PnP CSV / TSV…").clicked(){match crate::platform::choose_path(false,"pnp"){Ok(Some(p))=>self.open_pnp(p),Ok(None)=>{},Err(e)=>self.ui_error=Some(e)}}
                ui.checkbox(&mut self.components.overlay,"显示组件 Overlay");
                let Some(board)=self.view.board.clone() else {ui.label("未导入 PnP");return;};
                ui.label(format!("{} 个组件 · {}",board.components.len(),if board.registration.is_some(){"已校准"}else{"未校准；不提供绝对定位"}));
                ui.horizontal(|ui|{ui.label("RefDes");ui.text_edit_singleline(&mut self.components.query);});
                ui.horizontal(|ui|{ui.selectable_value(&mut self.components.mode,RefdesMatch::Exact,"精确");ui.selectable_value(&mut self.components.mode,RefdesMatch::Prefix,"前缀");ui.selectable_value(&mut self.components.mode,RefdesMatch::Substring,"子串");});
                ui.horizontal(|ui|{ui.selectable_value(&mut self.components.side,None,"全部 Side");ui.selectable_value(&mut self.components.side,Some(BoardSide::Top),"Top");ui.selectable_value(&mut self.components.side,Some(BoardSide::Bottom),"Bottom");});
                ui.horizontal(|ui|{ui.label("Footprint（精确，可留空）");ui.text_edit_singleline(&mut self.components.footprint);});
                if ui.button("搜索 / 应用筛选").clicked() && let Some(context)=Context::capture(&self.view){self.send(Action::ComponentSearch(context.clone(),ComponentQuery{revision:context.revision,query:self.components.query.clone(),mode:self.components.mode,side:self.components.side,footprint:(!self.components.footprint.is_empty()).then(||self.components.footprint.clone()),offset:0,limit:500}));}
                ui.label(format!("{} 个结果",self.view.component_indices.len()));
                let indices=self.view.component_indices.clone();
                egui::ScrollArea::vertical().id_salt("component-results").max_height(180.).show_rows(ui,24.,indices.len(),|ui,range|{
                    for row in range {
                        let Some(c)=indices.get(row).and_then(|i|board.components.get(*i)) else {continue;};
                        let label=format!("{} · {:?} · {}",c.refdes,c.side,c.footprint.as_deref().unwrap_or(""));
                        if ui.selectable_label(self.components.focused.as_ref().is_some_and(|f|f.component.id==c.id),label).clicked(){
                            self.focus_component(component_info(c,&board));
                        }
                    }
                });
                ui.collapsing("Board → Manufacturing World 配准",|ui|{
                    let previous=self.components.registration_input();
                    ui.horizontal(|ui|{ui.selectable_value(&mut self.components.manual,true,"Identity / 手工");ui.selectable_value(&mut self.components.manual,false,"两点配准");});
                    ui.checkbox(&mut self.components.reflect,"显式反射 Board X（围绕 Board Y 轴）");
                    if self.components.manual {
                        ui.horizontal(|ui|{ui.label("CCW °");ui.add(egui::DragValue::new(&mut self.components.angle));});
                        ui.label("平移 mm");point_controls(ui,&mut self.components.translation);
                    }else{
                        for i in 0..2{ui.label(format!("Board 点 {}（mm）",i+1));point_controls(ui,&mut self.components.board_points[i]);ui.label(format!("World 点 {}（mm）",i+1));point_controls(ui,&mut self.components.world_points[i]);if ui.button(format!("在 Canvas 捕捉 World 点 {}",i+1)).clicked(){self.components.pick=Some(i);self.drag=None;self.grip=None;self.tool=crate::tools::ActiveTool::Select;self.components.registration_confirmed=false;}}
                    }
                    let candidate=registration(self.components.registration_input());
                    match &candidate{Ok(r)=>{ui.label(format!("Board distance {:?} mm · World distance {:?} mm · residual {:.6} mm",r.board_distance_mm,r.world_distance_mm,r.residual_mm));},Err(e)=>{ui.colored_label(egui::Color32::LIGHT_RED,*e);}}
                    if previous!=self.components.registration_input(){self.components.registration_confirmed=false;}
                    ui.checkbox(&mut self.components.registration_confirmed,"确认 Board / World 约定和配准结果");
                    if ui.add_enabled(candidate.is_ok()&&self.components.registration_confirmed,egui::Button::new("提交配准（可撤销）")).clicked()&&let Some(context)=Context::capture(&self.view){self.send(Action::BoardRegistration(context,self.components.registration_input()));self.components.registration_confirmed=false;}
                    if let Some(r)=&board.registration{ui.label(format!("当前：{:?}; residual {:.6} mm",r.transform,r.residual_mm));}
                });
            });
        });
        self.components.open = open;
        if !open {
            self.components.pick = None;
        }
    }
    pub(crate) fn component_canvas(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        rect: egui::Rect,
    ) {
        if let Some(i) = self.components.pick {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.components.pick = None;
                return;
            }
            if response.clicked()
                && !ctx.wants_keyboard_input()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let raw = self.camera.world(pos, rect);
                match self.object_snap_runtime.resolve(
                    raw,
                    &self.object_snap,
                    self.grid,
                    self.camera,
                    ctx.pixels_per_point(),
                    self.view.snap_snapshot.as_deref(),
                    &self.view.snap_index,
                    &self.view.layers,
                    None,
                    ctx.input(|i| i.modifiers.alt),
                ) {
                    Ok(r) => {
                        self.components.world_points[i] = [r.point.x_mm, r.point.y_mm];
                        self.components.pick = None;
                    }
                    Err(e) => self.ui_error = Some(e),
                }
            }
        }
    }
    pub(crate) fn focus_component(&mut self, info: ComponentInfo) {
        if let Some(point) = info.world_position {
            self.ui_error = None;
            self.camera.center = point;
            self.fit = false;
        } else {
            self.ui_error = Some("Board 未校准：请先明确 Identity/手工或两点配准".into());
        }
        self.components.focused = Some(info);
        self.components.focus_revision = self
            .view
            .info
            .as_ref()
            .map_or(String::new(), |i| i.revision.clone());
        rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info, "view.focus_component");
    }
    pub(crate) fn paint_component(&self, painter: &egui::Painter, rect: egui::Rect, ppp: f32) {
        if !self.components.overlay {
            return;
        }
        let Some(info) = self.components.focused.as_ref().filter(|_| {
            self.view
                .info
                .as_ref()
                .is_some_and(|i| i.revision == self.components.focus_revision)
        }) else {
            return;
        };
        let (Some(p), Some(angle)) = (info.world_position, info.world_rotation_deg) else {
            return;
        };
        let center = self.camera.screen(p, rect);
        if !rect.expand(30.).contains(center) {
            return;
        }
        let color = egui::Color32::from_rgb(255, 200, 40);
        let radius = 8. / ppp;
        painter.circle_stroke(center, radius, egui::Stroke::new(2. / ppp, color));
        let (s, c) = angle.to_radians().sin_cos();
        let end = center + egui::vec2(c as f32, -s as f32) * (24. / ppp);
        painter.arrow(center, end - center, egui::Stroke::new(2. / ppp, color));
        painter.text(
            center + egui::vec2(radius + 3., -radius),
            egui::Align2::LEFT_BOTTOM,
            &info.component.refdes,
            egui::FontId::proportional(13.),
            color,
        );
    }
    pub(crate) fn accept_component_reply(&mut self, changed: bool) {
        if changed {
            self.components.focused = None;
            self.components.pick = None;
        }
        let Some(info) = self.view.info.as_ref() else {
            return;
        };
        if info.revision != self.components.focus_revision {
            let id = self
                .components
                .focused
                .as_ref()
                .map(|f| f.component.id.clone());
            self.components.focused = id.and_then(|id| {
                self.view.board.as_ref().and_then(|b| {
                    b.components
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| component_info(c, b))
                })
            });
            self.components.focus_revision = info.revision.clone();
        }
    }
}
fn point_controls(ui: &mut egui::Ui, point: &mut [f64; 2]) {
    ui.horizontal(|ui| {
        ui.label("X");
        ui.add(
            egui::DragValue::new(&mut point[0])
                .speed(0.1)
                .range(-MAX_BOARD_MM..=MAX_BOARD_MM),
        );
        ui.label("Y");
        ui.add(
            egui::DragValue::new(&mut point[1])
                .speed(0.1)
                .range(-MAX_BOARD_MM..=MAX_BOARD_MM),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Model {
        let mut m = Model::default();
        m.run(Action::NewWorkspace);
        m.run(Action::ImportGerbers(vec![
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/s4d1/board.gbr"),
        ]));
        assert!(m.view.error.is_none());
        m
    }
    #[test]
    fn worker_mapping_stale_preview_whole_import_and_pure_focus_overlay() {
        let mut m = model();
        let c = Context::capture(&m.view).unwrap();
        let request = PreviewRequest {
            context: c.clone(),
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/s4d1/pnp.csv"),
            mapping: UiState::default().mapping,
        };
        m.run(Action::PnpPreview(request.clone()));
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
        let reply = m.view.pnp_preview.clone().unwrap();
        assert!(reply.result.preview.valid());
        let params = ImportPnpParams {
            path: request.path.to_string_lossy().into(),
            mapping: request.mapping.clone(),
            preview_sha256: reply.result.sha256.clone(),
            allow_replace: false,
        };
        m.run(Action::PnpImport(c.clone(), params));
        assert!(m.view.error.is_none());
        assert_eq!(m.view.component_indices.len(), 3);
        let info = m.view.info.clone().unwrap();
        let geometry = m.service.render_snapshot(&info.document_id).unwrap().layers;
        m.run(Action::PnpPreview(request));
        assert!(m.view.error.is_some());
        assert_eq!(m.view.info.as_ref().unwrap().revision, info.revision);
        let context = Context::capture(&m.view).unwrap();
        m.run(Action::BoardRegistration(
            context,
            RegistrationInput::Manual {
                transform: CoordinateTransform2D {
                    rotation_deg: 90.,
                    translation: MmPoint::new(20., 30.),
                    reflect_x: false,
                },
            },
        ));
        assert!(m.view.error.is_none());
        let info = m.view.info.clone().unwrap();
        let board = m.view.board.clone().unwrap();
        let focus = component_info(&board.components[1], &board);
        assert_eq!(focus.world_position, Some(MmPoint::new(20., 40.)));
        assert_eq!(focus.world_rotation_deg, Some(180.));
        assert_eq!(
            m.service.render_snapshot(&info.document_id).unwrap().layers,
            geometry
        );
        let mut app = crate::modal::tests::app();
        app.view = m.view.clone();
        let mut uncalibrated = focus.clone();
        uncalibrated.world_position = None;
        uncalibrated.world_rotation_deg = None;
        let camera_before = app.camera;
        app.focus_component(uncalibrated);
        assert!(app.ui_error.as_ref().is_some_and(|e| e.contains("未校准")));
        assert_eq!(app.camera.center, camera_before.center);
        app.focus_component(focus.clone());
        assert!(app.ui_error.is_none());
        assert_eq!(app.camera.center, focus.world_position.unwrap());
        assert!(!app.fit);
        app.components.overlay = true;
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::pos2(100., 80.), egui::vec2(800., 600.));
        for ppp in [1., 2., 3.] {
            ctx.set_pixels_per_point(ppp);
            for scale in [0.1, 1., 10., 1000.] {
                app.camera = crate::camera::Camera {
                    center: focus.world_position.unwrap(),
                    scale,
                };
                let screen = app.camera.screen(focus.world_position.unwrap(), rect);
                assert_eq!(screen, rect.center());
                assert!(
                    app.camera
                        .world(screen, rect)
                        .distance_mm(focus.world_position.unwrap())
                        < 1e-9
                );
                let _ = ctx.run(Default::default(), |ctx| {
                    let painter = ctx.layer_painter(egui::LayerId::background());
                    app.paint_component(&painter, rect, ppp);
                });
            }
        }
        assert_eq!(app.view.info.as_ref().unwrap(), &info);
        assert_eq!(m.service.document_get(&info.document_id).unwrap(), info);
    }
    #[test]
    fn virtualized_100k_rows_only_construct_visible_range_and_cancel_is_pure() {
        let ctx = egui::Context::default();
        let mut constructed = 0;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 400.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().show_rows(ui, 24., 100000, |ui, range| {
                        constructed = range.len();
                        for i in range {
                            ui.label(format!("R{i}"));
                        }
                    });
                });
            },
        );
        assert!(constructed < 30, "{constructed}");
        let mut app = crate::modal::tests::app();
        app.open_modal(ActiveModal::Pnp);
        app.components.pick = Some(1);
        let before = app.view.info.clone();
        app.cancel_modal();
        assert_eq!(app.view.info, before);
        assert!(app.view.pnp_preview.is_none());
    }
}
