use crate::{
    EditorApp,
    modal::ActiveModal,
    state::{Action, PivotInput},
    tools::DisplayUnit,
};
use editor_service::ManufacturingPrecision;
use eframe::egui;
impl EditorApp {
    pub(crate) fn precision(&self) -> ManufacturingPrecision {
        self.view
            .info
            .as_ref()
            .map_or_else(Default::default, |d| d.manufacturing_precision)
    }
    pub(crate) fn length(&self, mm: f64) -> String {
        self.display_unit
            .format_length(mm, self.precision().resolution_mm)
    }
    pub(crate) fn length_action(&self, action: Action) -> Result<Action, String> {
        let mm = |s: &str| self.display_unit.parse_length(s).map(|v| v.to_string());
        Ok(match action {
            Action::Move(x, y) => Action::Move(mm(&x)?, mm(&y)?),
            Action::SetFlashSize(w, h) => {
                Action::SetFlashSize(mm(&w)?, h.map(|s| mm(&s)).transpose()?)
            }
            Action::Rotate(a, PivotInput::Custom(x, y)) => {
                Action::Rotate(a, PivotInput::Custom(mm(&x)?, mm(&y)?))
            }
            other => other,
        })
    }
    pub(crate) fn unit_controls(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(format!("单位 / 精度… {}", self.display_unit.suffix()))
            .clicked()
        {
            self.open_modal(ActiveModal::Units);
        }
    }
    pub(crate) fn units_modal(&mut self, ui: &mut egui::Ui) {
        ui.label("显示单位（仅界面；不修改制造内容）");
        ui.horizontal(|ui| {
            for unit in DisplayUnit::ALL {
                if ui
                    .selectable_label(self.display_unit == unit, unit.suffix())
                    .clicked()
                {
                    match self.text.change_unit(unit) {
                        Ok(()) => {
                            self.display_unit = unit;
                            self.persist_project_view();
                            self.size_aperture_id = None;
                        }
                        Err(e) => {
                            self.ui_error =
                                Some(format!("保留文字草稿含未完成的长度输入，请先修正：{e}"))
                        }
                    }
                }
            }
        });
        ui.label("显示小数位：Auto（由单位和制造分辨率推导）");
        ui.separator();
        ui.label("制造分辨率（µm；只影响导出规范化和新文字误差预算）");
        ui.horizontal(|ui| {
            for value in ["1", "0.5", "0.1"] {
                if ui
                    .selectable_label(self.spacing == value, format!("{value} µm"))
                    .clicked()
                {
                    self.spacing = value.into();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Custom µm");
            ui.text_edit_singleline(&mut self.spacing);
        });
        ui.small("0.001–1000 µm，步进 0.001 µm。网格步长独立；不改变已有几何或撤销记录。");
        if ui
            .add_enabled(self.view.info.is_some(), egui::Button::new("应用制造精度"))
            .clicked()
            || (self.view.info.is_some() && self.dialog_enter(ui))
        {
            match DisplayUnit::Micrometer
                .parse_length(&self.spacing)
                .and_then(|resolution_mm| ManufacturingPrecision { resolution_mm }.validate())
            {
                Ok(p) => self.send(Action::Precision(p)),
                Err(e) => self.ui_error = Some(e),
            }
        }
        if self
            .view
            .info
            .as_ref()
            .is_some_and(|d| d.export_policy_dirty)
        {
            ui.label("导出策略有未保存更改");
        }
    }
}
