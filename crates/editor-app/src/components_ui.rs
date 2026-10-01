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
    pub units_selected: bool,
    pub columns_confirmed: bool,
    pub convention_confirmed: bool,
    pub replace: bool,
    pub query: String,
    pub mode: RefdesMatch,
    pub side: Option<BoardSide>,
    pub footprint: String,
    pub candidates: crate::candidates_ui::UiState,
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
                source: None,
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
            units_selected: false,
            columns_confirmed: false,
            convention_confirmed: false,
            replace: false,
            query: String::new(),
            mode: RefdesMatch::Prefix,
            side: None,
            footprint: String::new(),
            candidates: Default::default(),
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
fn fixed_source() -> PnpSource {
    PnpSource::FixedWidth {
        skip_lines: 1,
        columns: vec![
            PnpColumnSpan {
                start: 0,
                end: Some(20),
            },
            PnpColumnSpan {
                start: 20,
                end: Some(33),
            },
            PnpColumnSpan {
                start: 33,
                end: Some(46),
            },
            PnpColumnSpan {
                start: 46,
                end: Some(51),
            },
            PnpColumnSpan {
                start: 51,
                end: Some(55),
            },
            PnpColumnSpan {
                start: 55,
                end: None,
            },
        ],
    }
}
fn choose_fixed(mapping: &mut PnpMapping) {
    mapping.source = Some(fixed_source());
    mapping.refdes = 0;
    mapping.x = 1;
    mapping.y = 2;
    mapping.rotation = 3;
    mapping.side = 4;
    mapping.footprint = Some(5);
    mapping.value = None;
    mapping.top_token = String::new();
    mapping.bottom_token = "m".into();
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ColumnRole {
    Ignore,
    Refdes,
    X,
    Y,
    Rotation,
    Side,
    Footprint,
    Value,
}
impl ColumnRole {
    const ALL: [Self; 8] = [
        Self::Ignore,
        Self::Refdes,
        Self::X,
        Self::Y,
        Self::Rotation,
        Self::Side,
        Self::Footprint,
        Self::Value,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Ignore => "忽略",
            Self::Refdes => "位号",
            Self::X => "X 坐标",
            Self::Y => "Y 坐标",
            Self::Rotation => "角度",
            Self::Side => "面别",
            Self::Footprint => "封装",
            Self::Value => "数值",
        }
    }
}
fn column_role(m: &PnpMapping, col: usize) -> ColumnRole {
    for (role, index) in [
        (ColumnRole::Refdes, Some(m.refdes)),
        (ColumnRole::X, Some(m.x)),
        (ColumnRole::Y, Some(m.y)),
        (ColumnRole::Rotation, Some(m.rotation)),
        (ColumnRole::Side, Some(m.side)),
        (ColumnRole::Footprint, m.footprint),
        (ColumnRole::Value, m.value),
    ] {
        if index == Some(col) {
            return role;
        }
    }
    ColumnRole::Ignore
}
fn assign_column(m: &mut PnpMapping, col: usize, role: ColumnRole) {
    for index in [
        &mut m.refdes,
        &mut m.x,
        &mut m.y,
        &mut m.rotation,
        &mut m.side,
    ] {
        if *index == col {
            *index = MAX_COLUMNS;
        }
    }
    for index in [&mut m.footprint, &mut m.value] {
        if *index == Some(col) {
            *index = None;
        }
    }
    match role {
        ColumnRole::Ignore => {}
        ColumnRole::Refdes => m.refdes = col,
        ColumnRole::X => m.x = col,
        ColumnRole::Y => m.y = col,
        ColumnRole::Rotation => m.rotation = col,
        ColumnRole::Side => m.side = col,
        ColumnRole::Footprint => m.footprint = Some(col),
        ColumnRole::Value => m.value = Some(col),
    }
}
fn apply_proposal(mapping: &mut PnpMapping, proposed: &PnpMapping) {
    mapping.refdes = proposed.refdes;
    mapping.x = proposed.x;
    mapping.y = proposed.y;
    mapping.rotation = proposed.rotation;
    mapping.side = proposed.side;
    mapping.footprint = proposed.footprint;
    mapping.value = proposed.value;
    mapping.top_token = proposed.top_token.clone();
    mapping.bottom_token = proposed.bottom_token.clone();
}
fn raw_table(ui: &mut egui::Ui, preview: &PnpPreview, mapping: &mut PnpMapping) {
    // Optional roles outside this table have no data to preserve. Mandatory
    // missing roles stay invalid until the user assigns a displayed column.
    for index in [&mut mapping.footprint, &mut mapping.value] {
        if !preview.headers.is_empty() && index.is_some_and(|i| i >= preview.headers.len()) {
            *index = None;
        }
    }
    ui.label("原始数据预览（最多 20 行）：为每列选择用途；忽略列不保存到组件表。");
    egui::ScrollArea::both()
        .id_salt("pnp-raw-table")
        .max_width(ui.available_width())
        .auto_shrink([false, true])
        .max_height(240.)
        .show(ui, |ui| {
            egui::Grid::new("pnp-column-roles")
                .striped(true)
                .min_col_width(115.)
                .show(ui, |ui| {
                    ui.label("源文件行");
                    for (i, header) in preview.headers.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.label(format!("第 {} 列", i + 1));
                            ui.add(egui::Label::new(header).truncate())
                                .on_hover_text(header);
                            let mut role = column_role(mapping, i);
                            let before = role;
                            egui::ComboBox::from_id_salt(("pnp-role", i))
                                .width(105.)
                                .selected_text(role.label())
                                .show_ui(ui, |ui| {
                                    for option in ColumnRole::ALL {
                                        ui.selectable_value(&mut role, option, option.label());
                                    }
                                });
                            if role != before {
                                assign_column(mapping, i, role);
                            }
                        });
                    }
                    ui.end_row();
                    for row in &preview.sample_rows {
                        ui.label(row.line.to_string());
                        for field in &row.fields {
                            ui.add_sized([115., 20.], egui::Label::new(field).truncate())
                                .on_hover_text(field);
                        }
                        ui.end_row();
                    }
                });
        });
}
impl UiState {
    fn import_confirmed(&self, declared_unit: Option<PnpUnit>) -> bool {
        (self.units_selected || declared_unit == Some(self.mapping.unit))
            && self.units_confirmed
            && self.columns_confirmed
            && self.convention_confirmed
    }
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
            match path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("xlsx") => {
                    self.components.mapping.source = Some(PnpSource::Xlsx {
                        worksheet: String::new(),
                        header_row: 1,
                    })
                }
                Some("txt") => choose_fixed(&mut self.components.mapping),
                Some("tsv") => {
                    self.components.mapping.source = Some(PnpSource::Delimited {
                        skip_lines: 0,
                        has_header: true,
                    });
                    self.components.mapping.delimiter = Delimiter::Tsv;
                }
                _ => {
                    self.components.mapping.source = Some(PnpSource::Delimited {
                        skip_lines: 0,
                        has_header: true,
                    });
                    self.components.mapping.delimiter = Delimiter::Csv;
                }
            }
            // Unknown columns start unassigned; defaults never guess their roles.
            if !matches!(
                self.components.mapping.source,
                Some(PnpSource::FixedWidth { .. })
            ) {
                self.components.mapping.refdes = MAX_COLUMNS;
                self.components.mapping.x = MAX_COLUMNS;
                self.components.mapping.y = MAX_COLUMNS;
                self.components.mapping.rotation = MAX_COLUMNS;
                self.components.mapping.side = MAX_COLUMNS;
                self.components.mapping.footprint = None;
                self.components.mapping.value = None;
            }
            self.components.path = Some(path.clone());
            self.components.units_confirmed = false;
            self.components.units_selected = false;
            self.components.columns_confirmed = false;
            self.components.convention_confirmed = false;
            self.components.replace = false;
            self.view.pnp_preview = None;
            if let Some(context) = Context::capture(&self.view) {
                self.send(Action::PnpPreview(PreviewRequest {
                    context,
                    path,
                    mapping: self.components.mapping.clone(),
                }));
            }
        }
    }
    pub(crate) fn pnp_modal(&mut self, ui: &mut egui::Ui) {
        ui.label("明确源格式、列、单位、方向和 Side；XLSX 不运行公式，TXT 保留空白面别。");
        if let Some(path) = &self.components.path {
            ui.label(path.file_name().unwrap_or_default().to_string_lossy());
        }
        let before = self.components.mapping.clone();
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    matches!(
                        self.components.mapping.source,
                        None | Some(PnpSource::Delimited { .. })
                    ),
                    "CSV / TSV",
                )
                .clicked()
            {
                self.components.mapping.source = Some(PnpSource::Delimited {
                    skip_lines: 0,
                    has_header: true,
                });
            }
            if ui
                .selectable_label(
                    matches!(self.components.mapping.source, Some(PnpSource::Xlsx { .. })),
                    "XLSX",
                )
                .clicked()
            {
                self.components.mapping.source = Some(PnpSource::Xlsx {
                    worksheet: String::new(),
                    header_row: 1,
                });
            }
            if ui
                .selectable_label(
                    matches!(
                        self.components.mapping.source,
                        Some(PnpSource::FixedWidth { .. })
                    ),
                    "定宽 TXT",
                )
                .clicked()
            {
                choose_fixed(&mut self.components.mapping);
            }
        });
        let worksheets = self
            .view
            .pnp_preview
            .as_ref()
            .map(|r| r.result.worksheets.clone())
            .unwrap_or_default();
        match &mut self.components.mapping.source {
            None | Some(PnpSource::Delimited { .. }) => {
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
                let (mut skip_lines, mut has_header) = match &self.components.mapping.source {
                    Some(PnpSource::Delimited {
                        skip_lines,
                        has_header,
                    }) => (*skip_lines, *has_header),
                    _ => (0, true),
                };
                let old = (skip_lines, has_header);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut has_header, "首行是表头");
                    ui.label("跳过首部行数");
                    ui.add(egui::DragValue::new(&mut skip_lines).range(0..=MAX_PNP_LINES));
                });
                if old != (skip_lines, has_header) {
                    self.components.mapping.source = Some(PnpSource::Delimited {
                        skip_lines,
                        has_header,
                    });
                }
            }
            Some(PnpSource::Xlsx {
                worksheet,
                header_row,
            }) => {
                ui.label("先预览读取工作表名单，再明确选择工作表与表头行。");
                ui.horizontal(|ui| {
                    ui.label("工作表");
                    ui.text_edit_singleline(worksheet);
                });
                for name in worksheets {
                    ui.selectable_value(worksheet, name.clone(), name);
                }
                ui.horizontal(|ui| {
                    ui.label("表头行（0 表示无表头）");
                    ui.add(egui::DragValue::new(header_row).range(0..=MAX_PNP_LINES));
                });
            }
            Some(PnpSource::FixedWidth {
                skip_lines,
                columns,
            }) => {
                ui.label("当前预设：0–20 RefDes、20–33 X、33–46 Y、46–51 角度、51–55 面别、55 后 Footprint。");
                ui.label("此 TXT 预设：空白=Top / m=Bottom；其他来源请核对面别约定。");
                ui.horizontal(|ui| {
                    ui.label("跳过首部行数");
                    ui.add(egui::DragValue::new(skip_lines).range(0..=MAX_PNP_LINES));
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(columns.len() < MAX_COLUMNS, egui::Button::new("新增数据列"))
                        .clicked()
                        && let Some(last) = columns.last_mut()
                    {
                        let next = last
                            .end
                            .unwrap_or(last.start.saturating_add(10))
                            .min(MAX_FIELD_BYTES);
                        last.end = Some(next);
                        columns.push(PnpColumnSpan {
                            start: next,
                            end: None,
                        });
                    }
                    if ui
                        .add_enabled(columns.len() > 1, egui::Button::new("删除最后一列"))
                        .clicked()
                    {
                        columns.pop();
                        if let Some(last) = columns.last_mut() {
                            last.end = None;
                        }
                    }
                });
                for (i, c) in columns.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("列 {} 起始字节", i + 1));
                        ui.add(egui::DragValue::new(&mut c.start).range(0..=MAX_FIELD_BYTES));
                        if let Some(end) = &mut c.end {
                            ui.label("结束（不含）");
                            ui.add(egui::DragValue::new(end).range(1..=MAX_FIELD_BYTES));
                        } else {
                            ui.label("到行尾");
                        }
                    });
                }
            }
        }
        let table_reply = self.view.pnp_preview.clone().filter(|r| {
            r.request.mapping.source == self.components.mapping.source
                && r.request.mapping.delimiter == self.components.mapping.delimiter
                && Some(&r.request.path) == self.components.path.as_ref()
                && r.request.context.valid(&self.view)
        });
        if let Some(reply) = &table_reply {
            if let Some(proposed) = reply.result.suggested_mapping.clone()
                && ui.button("使用表头识别建议（仍需确认列用途）").clicked()
            {
                apply_proposal(&mut self.components.mapping, &proposed);
            }
            raw_table(ui, &reply.result.preview, &mut self.components.mapping);
        }
        let declared_unit = table_reply.as_ref().and_then(|r| r.result.declared_unit);
        if let Some(unit) = declared_unit {
            ui.label(format!(
                "文件声明单位：{}",
                if unit == PnpUnit::Mm { "mm" } else { "inch" }
            ));
        } else {
            ui.colored_label(
                egui::Color32::YELLOW,
                "源文件未声明单位：必须手动选择 mm / inch，再确认。",
            );
        }
        ui.horizontal(|ui| {
            ui.label("源单位");
            for (unit, label) in [(PnpUnit::Mm, "mm"), (PnpUnit::Inch, "inch")] {
                let selected = self.components.mapping.unit == unit
                    && (self.components.units_selected || declared_unit == Some(unit));
                if ui.selectable_label(selected, label).clicked() {
                    self.components.mapping.unit = unit;
                    self.components.units_selected = true;
                    self.components.units_confirmed = false;
                }
            }
        });
        let unit_chosen =
            self.components.units_selected || declared_unit == Some(self.components.mapping.unit);
        ui.add_enabled_ui(unit_chosen, |ui| {
            ui.checkbox(&mut self.components.units_confirmed, "确认源单位");
        });
        ui.add_enabled_ui(
            table_reply
                .as_ref()
                .is_some_and(|r| !r.result.preview.sample_rows.is_empty())
                && self.components.mapping.validate(),
            |ui| {
                ui.checkbox(
                    &mut self.components.columns_confirmed,
                    "确认列用途：位号、X、Y、角度、面别均已选择；其余列可忽略",
                );
            },
        );
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
            self.components.columns_confirmed = false;
        }
        if ui.button("读取表格 / 按当前选择检查").clicked()
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
            && self.components.import_confirmed(declared_unit)
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
                if ui.button("Import PnP XLSX / TXT / CSV…").clicked(){match crate::platform::choose_path(false,"pnp"){Ok(Some(p))=>self.open_pnp(p),Ok(None)=>{},Err(e)=>self.ui_error=Some(e)}}
                ui.checkbox(&mut self.components.overlay,"显示组件 Overlay");
                let Some(board)=self.view.board.clone() else {ui.label("未导入 PnP");return;};
                ui.label(format!("{} 个组件 · {}",board.components.len(),if board.registration.is_some(){"已校准"}else{"未校准；不提供绝对定位"}));
                let mut search_enter=false;
                ui.horizontal(|ui|{ui.label("RefDes");let response=ui.text_edit_singleline(&mut self.components.query);search_enter=response.lost_focus()&&ui.input(|i|i.key_pressed(egui::Key::Enter));});
                ui.horizontal(|ui|{ui.selectable_value(&mut self.components.mode,RefdesMatch::Exact,"精确");ui.selectable_value(&mut self.components.mode,RefdesMatch::Prefix,"前缀");ui.selectable_value(&mut self.components.mode,RefdesMatch::Substring,"子串");});
                ui.horizontal(|ui|{ui.selectable_value(&mut self.components.side,None,"全部 Side");ui.selectable_value(&mut self.components.side,Some(BoardSide::Top),"Top");ui.selectable_value(&mut self.components.side,Some(BoardSide::Bottom),"Bottom");});
                ui.horizontal(|ui|{ui.label("Footprint（精确，可留空）");ui.text_edit_singleline(&mut self.components.footprint);});
                if (ui.button("搜索 / 应用筛选").clicked()||search_enter) && let Some(context)=Context::capture(&self.view){self.send(Action::ComponentSearch(context.clone(),ComponentQuery{revision:context.revision,query:self.components.query.clone(),mode:self.components.mode,side:self.components.side,footprint:(!self.components.footprint.is_empty()).then(||self.components.footprint.clone()),offset:0,limit:500}));}
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
                self.candidate_controls(ui);
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
        let unregistered = info.world_position.is_none();
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
        self.components.candidates.requested = None;
        self.request_candidates();
        if unregistered {
            self.ui_error = Some("Board 未校准：请先完成 Board→World 配准".into());
        }
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
            self.components.candidates.requested = None;
            self.components.candidates.layers.clear();
            self.components.focused = None;
            self.components.pick = None;
        }
        let Some(info) = self.view.info.as_ref() else {
            return;
        };
        if info.revision != self.components.focus_revision {
            self.components.candidates.requested = None;
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
    fn opening_unknown_table_resets_roles_units_and_confirmations() {
        let mut app = crate::modal::tests::app();
        app.components.units_selected = true;
        app.components.units_confirmed = true;
        app.components.columns_confirmed = true;
        app.components.convention_confirmed = true;
        app.open_pnp(PathBuf::from("five-columns.csv"));
        assert_eq!(app.modal, Some(ActiveModal::Pnp));
        assert!(!app.components.units_selected);
        assert!(!app.components.units_confirmed);
        assert!(!app.components.columns_confirmed);
        assert!(!app.components.convention_confirmed);
        assert_eq!(app.components.mapping.refdes, MAX_COLUMNS);
        assert_eq!(app.components.mapping.x, MAX_COLUMNS);
        assert_eq!(app.components.mapping.y, MAX_COLUMNS);
        assert_eq!(app.components.mapping.rotation, MAX_COLUMNS);
        assert_eq!(app.components.mapping.side, MAX_COLUMNS);
        assert!(app.components.mapping.footprint.is_none());
        assert!(app.components.mapping.value.is_none());
    }
    #[test]
    fn column_roles_ignore_reassignment_and_manual_confirmations() {
        let mut state = UiState::default();
        assert!(!state.import_confirmed(None));
        state.units_confirmed = true;
        state.convention_confirmed = true;
        state.columns_confirmed = true;
        assert!(!state.import_confirmed(None));
        assert!(state.import_confirmed(Some(PnpUnit::Mm)));
        state.units_selected = true;
        assert!(state.import_confirmed(None));
        state.columns_confirmed = false;
        assert!(!state.import_confirmed(None));
        state.mapping.unit = PnpUnit::Inch;
        state.mapping.clockwise = true;
        state.mapping.rotation_offset_deg = 15.;
        state.mapping.invert_y = true;
        let proposed = UiState::default().mapping;
        apply_proposal(&mut state.mapping, &proposed);
        assert_eq!(state.mapping.unit, PnpUnit::Inch);
        assert!(state.mapping.clockwise);
        assert!(state.mapping.invert_y);
        assert_eq!(state.mapping.rotation_offset_deg, 15.);
        let m = &mut state.mapping;
        assign_column(m, 1, ColumnRole::Refdes);
        assert_eq!(m.refdes, 1);
        assert_eq!(m.x, MAX_COLUMNS);
        assert_eq!(column_role(m, 0), ColumnRole::Ignore);
        assert!(!m.validate());
        assign_column(m, 0, ColumnRole::X);
        assert!(m.validate());
        assign_column(m, 5, ColumnRole::Ignore);
        assign_column(m, 6, ColumnRole::Ignore);
        assert_eq!(m.footprint, None);
        assert_eq!(m.value, None);
        assert!(m.validate());
    }
    #[test]
    fn mapping_table_renders_invalid_roles_and_keeps_original_sample() {
        let mut m = UiState::default().mapping;
        m.footprint = None;
        m.value = None;
        m.refdes = MAX_COLUMNS;
        let p = parse_pnp(b"A,B,C,D,E\nR1,1,2,90,Top\n", &m);
        assert!(!p.valid());
        assert_eq!(p.sample_rows[0].fields[0], "R1");
        let before = p.clone();
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| raw_table(ui, &p, &mut m));
        });
        assert!(!output.shapes.is_empty());
        assert_eq!(p, before);
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
