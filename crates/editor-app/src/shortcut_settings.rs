//! Single-command candidates, immutable import preview and one independent I/O task.
use crate::{
    shortcut_config::{self, Config, Error, Validated},
    shortcut_store::{self, Fingerprint},
};
use editor_core::command::{CommandId, Key, Modifiers, PhysicalModifiers, Platform, Shortcut};
use eframe::egui;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

pub(crate) struct Preview {
    pub candidate: Validated,
    generation: u64,
    hash: String,
    catalogue_hash: String,
    defaults_hash: String,
    platform: Platform,
}
pub(crate) enum ResultMessage {
    Load(shortcut_store::Startup),
    Save(Result<shortcut_store::Committed, Error>),
    Import(Result<Validated, Error>),
    Export(Result<Option<String>, Error>),
}
pub(crate) struct Settings {
    pub current: Validated,
    pub loading: bool,
    pub legacy_warning: bool,
    pub path: Option<PathBuf>,
    pub fingerprint: Option<Fingerprint>,
    pub protected: bool,
    pub warning: Option<String>,
    pub message: Option<String>,
    pub generation: u64,
    pub open: bool,
    pub search: String,
    pub editing: Option<CommandId>,
    pub keys: Vec<Shortcut>,
    pub candidate: Option<Shortcut>,
    pub recording: bool,
    pub preview: Option<Preview>,
    pub reset_confirm: bool,
    pub popup_at_event: bool,
    pub presses: Vec<(egui::Key, egui::Modifiers)>,
    pub pending: Option<Receiver<ResultMessage>>,
    held: BTreeSet<egui::Key>,
    barrier: bool,
    modifiers_down: bool,
}
impl Settings {
    pub fn load(path: Option<PathBuf>, platform: Platform) -> Self {
        let mut startup = shortcut_store::load(path.as_deref(), platform);
        if !startup.current.missing.is_empty() || !startup.current.default_conflicts.is_empty() {
            startup.warning = Some(format!(
                "已补充 {} 项缺失命令；{} 项新增默认与用户绑定冲突，保持未绑定：{}",
                startup.current.missing.len(),
                startup.current.default_conflicts.len(),
                startup.current.default_conflicts.join(", ")
            ));
        }
        Self {
            current: startup.current,
            loading: false,
            legacy_warning: false,
            path,
            fingerprint: startup.fingerprint,
            protected: startup.protected,
            warning: startup.warning,
            message: None,
            generation: 0,
            open: false,
            search: String::new(),
            editing: None,
            keys: vec![],
            candidate: None,
            recording: false,
            preview: None,
            reset_confirm: false,
            popup_at_event: false,
            presses: vec![],
            pending: None,
            held: BTreeSet::new(),
            barrier: false,
            modifiers_down: false,
        }
    }
    pub fn load_background(
        ctx: &egui::Context,
        path: Option<PathBuf>,
        platform: Platform,
        legacy_warning: bool,
    ) -> Self {
        let mut settings = Self::load(None, platform);
        settings.path = path.clone();
        settings.warning = None;
        settings.loading = true;
        settings.legacy_warning = legacy_warning;
        settings.start(ctx, move || {
            ResultMessage::Load(shortcut_store::load(path.as_deref(), platform))
        });
        settings
    }
    pub fn poll(&mut self) {
        let result = self.pending.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(ResultMessage::Export(Err(Error::new(
                "WorkerStopped",
                "配置任务中断；请重启核对磁盘配置",
            )))),
        });
        let Some(result) = result else {
            return;
        };
        self.pending = None;
        match result {
            ResultMessage::Load(startup) => {
                self.current = startup.current;
                self.fingerprint = startup.fingerprint;
                self.protected = startup.protected;
                self.warning = startup.warning;
                self.loading = false;
                if !self.current.missing.is_empty() || !self.current.default_conflicts.is_empty() {
                    self.warning = Some(format!(
                        "已补充 {} 项缺失命令；默认退让并未绑定：{}",
                        self.current.missing.len(),
                        self.current.default_conflicts.join(", ")
                    ));
                }
                if self.legacy_warning {
                    self.warning = Some(format!(
                        "{}旧preferences快捷键占位字段未猜测迁移；请在此重新设置。",
                        self.warning.as_deref().unwrap_or("")
                    ));
                }
            }
            ResultMessage::Save(Ok(c)) => {
                exit_phase("before-ui-install", self);
                self.current = c.current;
                self.fingerprint = Some(c.fingerprint);
                self.generation += 1;
                self.protected = false;
                self.warning = c.durability_warning;
                self.message = Some(
                    if self.warning.is_some() {
                        "配置已提交并生效；耐久性有警告"
                    } else {
                        "已自动保存，快捷键立即生效"
                    }
                    .into(),
                );
                self.editing = None;
                self.preview = None;
                self.reset_confirm = false;
                self.barrier = !self.held.is_empty() || self.modifiers_down;
                exit_phase("after-ui-install", self);
            }
            ResultMessage::Import(Ok(candidate)) => {
                let hash = editor_core::hash::sha256_hex(
                    &candidate.config.bytes().expect("validated snapshot"),
                );
                self.preview = Some(Preview {
                    candidate,
                    generation: self.generation,
                    hash,
                    catalogue_hash: catalogue_hash(),
                    defaults_hash: defaults_hash(Platform::current()),
                    platform: Platform::current(),
                });
                self.editing = None;
                self.message = None;
            }
            ResultMessage::Export(Ok(warning)) => {
                self.message = Some(warning.unwrap_or_else(|| "快捷键文件已导出".into()))
            }
            ResultMessage::Save(Err(e)) => {
                self.message = Some(format!("保存失败，原绑定仍生效：{e}"))
            }
            ResultMessage::Import(Err(e)) => {
                self.message = Some(format!("导入失败，现有配置完整保留：{e}"))
            }
            ResultMessage::Export(Err(e)) => self.message = Some(format!("导出失败：{e}")),
        }
    }
    fn start(
        &mut self,
        ctx: &egui::Context,
        task: impl FnOnce() -> ResultMessage + Send + 'static,
    ) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let result = task();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub fn save(&mut self, ctx: &egui::Context, candidate: Config, reestablish: bool) {
        if self.protected && !reestablish {
            self.message = Some("原配置受保护；请先明确确认重新建立默认配置".into());
            return;
        }
        let (Some(path), Some(expected)) = (self.path.clone(), self.fingerprint.clone()) else {
            self.message = Some("配置路径不可用或不可读取；请解决文件权限后重启".into());
            return;
        };
        let platform = Platform::current();
        if let Err(e) = shortcut_config::validate(candidate.clone(), platform) {
            self.message = Some(e.to_string());
            return;
        }
        self.start(ctx, move || {
            ResultMessage::Save(shortcut_store::save(&path, candidate, &expected, platform))
        });
    }
    pub fn apply_preview(&mut self, ctx: &egui::Context) {
        let Some(preview) = &self.preview else {
            return;
        };
        if preview.generation != self.generation
            || preview.catalogue_hash != catalogue_hash()
            || preview.defaults_hash != defaults_hash(Platform::current())
            || preview.platform != Platform::current()
            || editor_core::hash::sha256_hex(
                &preview.candidate.config.bytes().expect("validated preview"),
            ) != preview.hash
        {
            self.message = Some("导入预览已过期，请重新导入".into());
            return;
        }
        self.save(ctx, preview.candidate.config.clone(), false);
    }
    pub fn close(&mut self) {
        self.open = false;
        self.editing = None;
        self.recording = false;
        self.candidate = None;
        self.preview = None;
        self.reset_confirm = false;
        self.barrier = !self.held.is_empty() || self.modifiers_down;
    }
    pub fn raw_input(&mut self, raw: &mut egui::RawInput, ime: bool) {
        self.modifiers_down = !raw.modifiers.is_none();
        self.presses.clear();
        // One physical press owns one dispatch, even if a backend duplicates non-repeat events.
        raw.events.retain(|event| {
            if let egui::Event::Key {
                key,
                pressed,
                repeat,
                ..
            } = event
            {
                if *pressed {
                    let already = !self.held.insert(*key);
                    if already && !*repeat {
                        return false;
                    }
                } else {
                    self.held.remove(key);
                }
            }
            true
        });
        if !raw.focused {
            self.held.clear();
            self.recording = false;
            self.candidate = None;
            self.barrier = !self.held.is_empty() || self.modifiers_down;
        }
        if ime {
            if self.open && self.recording {
                self.candidate = None;
                self.barrier = true;
            }
            return; // IME owns its control keys; only the post-composition sequence is fenced.
        }
        if self.barrier {
            raw.events
                .retain(|e| !matches!(e, egui::Event::Key { pressed: true, .. }));
            if self.held.is_empty() && raw.modifiers.is_none() {
                self.barrier = false;
            }
            return;
        }
        if !self.open || !self.recording {
            self.presses = raw
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect();
            return;
        }
        let mut pass_tab = false;
        for event in &raw.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            {
                let plain_tab = *key == egui::Key::Tab
                    && !modifiers.ctrl
                    && !modifiers.mac_cmd
                    && !modifiers.command
                    && !modifiers.alt;
                if *key == egui::Key::Escape || plain_tab {
                    pass_tab = plain_tab;
                    self.recording = false;
                    self.candidate = None;
                    break;
                }
                if *key == egui::Key::Enter && modifiers.is_none() {
                    continue;
                }
                if let Some(key) = key_from_egui(*key) {
                    self.candidate =
                        Some(Shortcut::new(logical(*modifiers, Platform::current()), key));
                    self.recording = false;
                    break;
                }
            }
        }
        raw.events.retain(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::Tab,
                    ..
                }
            ) && pass_tab
                || !matches!(
                    e,
                    egui::Event::Key { pressed: true, .. } | egui::Event::Text(_)
                )
        });
        if !self.recording {
            self.barrier = !self.held.is_empty() || self.modifiers_down;
        }
    }
    pub fn window(&mut self, ctx: &egui::Context, ime: bool, event_text_focus: bool) {
        if !self.open {
            return;
        }
        let platform = Platform::current();
        let response = egui::Modal::new(egui::Id::new("shortcut-settings")).show(ctx, |ui| {
            ui.set_width(crate::ui::tokens::modal_width(ctx, 760., 220.));
            ui.heading("设置 · 快捷键");
            egui::ScrollArea::vertical().id_salt("shortcut-settings-body").max_height((ctx.content_rect().height()-150.).clamp(120., 580.)).auto_shrink([false, false]).show(ui, |ui| {
            ui.label("独立用户配置自动保存；不改变工程、制造内容或撤销历史。");
            ui.label("功能键可能被系统占用；可选择其他绑定，RCam不会更改系统键盘设置。");
            ui.label(if platform == Platform::MacOs { "Primary = Cmd · Secondary = Ctrl · Alt = Option · Shift = Shift" } else { "Primary = Ctrl · Secondary = Win（本版本不可绑定）· Alt = Alt" });
            if let Some(warning) = &self.warning { ui.colored_label(crate::ui::tokens::warning_text(ui.visuals()), warning); }
            if let Some(message) = &self.message { ui.label(message); }
            ui.add_enabled_ui(self.pending.is_none() && !self.recording && !ime, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("导入快捷键文件…").clicked() { match crate::platform::choose_shortcuts(false) { Ok(Some(path)) => self.start(ctx, move || ResultMessage::Import(shortcut_store::import(&path, platform))), Ok(None) => {}, Err(e) => self.message = Some(e) } }
                    if ui.button("导出快捷键文件…").clicked() { match crate::platform::choose_shortcuts(true) { Ok(Some(path)) => { let config = self.current.config.clone(); let current = self.path.clone(); self.start(ctx, move || ResultMessage::Export(shortcut_store::export(&path, config, current.as_deref(), platform))); }, Ok(None) => {}, Err(e) => self.message = Some(e) } }
                    if ui.button(if self.protected { "重新建立默认配置…" } else { "全部恢复默认…" }).clicked() { self.reset_confirm = true; self.preview = None; self.editing = None; }
                });
            });
            if self.pending.is_some() { ui.label("正在处理配置文件…"); ctx.request_repaint(); }
            if self.reset_confirm {
                ui.separator(); ui.label(if self.protected { "原文件将被默认快捷键完整替换；此前已保护原文件。确认重新建立配置？" } else { "将全部41项绑定恢复为当前兼容默认值，并自动保存。" });
                ui.add_enabled_ui(self.pending.is_none() && !ime, |ui| { ui.horizontal(|ui| { if ui.button("确认恢复默认并保存").clicked() { self.save(ctx, Config::defaults(platform), true); }
 if ui.button("取消恢复").clicked() { self.reset_confirm = false; } }); });
            } else if let Some(preview) = &self.preview {
                ui.separator(); ui.strong("导入预览 · 完整替换"); ui.label("确认后，所有当前绑定被此候选替换。显式空列表表示禁用；缺失项补默认，冲突的新项保持未绑定。");
                let changed = preview.candidate.config.bindings.iter().filter(|entry| self.current.config.bindings.iter().find(|e| e.command_id == entry.command_id) != Some(*entry)).count();
                let cleared = preview.candidate.config.bindings.iter().filter(|entry| entry.shortcuts.is_empty() && self.current.config.bindings.iter().any(|old| old.command_id == entry.command_id && !old.shortcuts.is_empty())).count();
                ui.label(format!("变更 {changed} 项 · 清空 {cleared} 项"));
                if preview.candidate.cross_platform { ui.label("逻辑键按当前平台映射：Primary在Mac为Cmd、Windows为Ctrl；Secondary在Mac为Ctrl、Windows为Win（本版本拒绝）。Alt在Mac为Option。物理组合可能不同，请确认。Windows原生未验收。"); }
                if !preview.candidate.default_conflicts.is_empty() { ui.label(format!("默认退让并保持未绑定：{}", preview.candidate.default_conflicts.join(", "))); }
                ui.label(format!("来源 {:?} · 41项命令 · 补缺 {} · 默认退让 {} · 跨平台 {}", preview.candidate.config.source_platform, preview.candidate.missing.len(), preview.candidate.default_conflicts.len(), preview.candidate.cross_platform));
                egui::ScrollArea::vertical().id_salt("shortcut-import").max_height(220.).show(ui, |ui| { for entry in &preview.candidate.config.bindings { if self.current.config.bindings.iter().find(|e| e.command_id == entry.command_id) != Some(entry) { ui.label(format!("{} → {}", entry.command_id, display_keys(&entry.shortcuts, platform))); } } });
                ui.add_enabled_ui(self.pending.is_none() && !self.protected && !ime, |ui| { if ui.button("确认完整替换并保存").clicked() { self.apply_preview(ctx); } });
                if ui.button("取消导入").clicked() { self.preview = None; }
            } else {
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("搜索命令名称 / ID / 当前快捷键"));
                egui::ScrollArea::vertical().id_salt("shortcut-command-list").max_height(260.).show(ui, |ui| {
                    let search = self.search.to_lowercase();
                    for command in shortcut_config::commands() {
                        let keys = &self.current.config.entry(command.id).shortcuts;
                        let display = display_keys(keys, platform);
                        let defaults = Config::defaults(platform);
                        let default_keys = &defaults.entry(command.id).shortcuts;
                        let status = if keys == default_keys { "默认" } else { "已修改" };
                        if !format!("{} {} {} {:?}", command.name, command.id.0, display, command.category).to_lowercase().contains(&search) { continue; }
                        ui.horizontal_wrapped(|ui| { if ui.add_enabled(self.pending.is_none() && !self.recording && !self.protected, egui::Button::new(format!("{} · {}", command.name, display)).selected(self.editing == Some(command.id))).clicked() { self.editing = Some(command.id); self.keys = keys.clone(); self.candidate = None; self.message = None; } ui.small(format!("{} · {:?} · {:?} · {status} · 默认 {}", command.id.0, command.category, command.context, display_keys(default_keys, platform))); });
                    }
                    ui.label("Esc · 固定安全取消键（不可配置）");
                });
                if let Some(id) = self.editing {
                    ui.separator(); ui.strong(format!("编辑 {}", id.0)); ui.label("只保存此命令的完整绑定列表（最多4项）；关闭或切换命令会丢弃候选。");
                    let mut remove = None; for (i, &key) in self.keys.iter().enumerate() { ui.horizontal(|ui| { ui.label(format_shortcut(key, platform)); if ui.add_enabled(!self.recording && self.pending.is_none(), egui::Button::new("移除此项")).clicked() { remove = Some(i); } }); } if let Some(i) = remove { self.keys.remove(i); }
                    if self.recording {
                        ui.label(if ime { "IME正在输入；暂停捕获，组合结束并释放按键后可继续。" } else { "按下组合键… Esc取消录制，Tab结束；不会执行编辑命令。" });
                        if ui.button("取消录制").clicked() { self.recording = false; self.candidate = None; self.barrier = !self.held.is_empty() || self.modifiers_down; }
                    }
                    ui.add_enabled_ui(self.pending.is_none() && !ime, |ui| {
                        if !self.recording {
                            if ui.add_enabled(self.keys.len() < 4, egui::Button::new("录制快捷键")).clicked() { self.recording = true; self.candidate = None; self.barrier = !self.held.is_empty() || self.modifiers_down; ui.memory_mut(|m| { if let Some(id) = m.focused() { m.surrender_focus(id); } }); }
                            if let Some(candidate) = self.candidate { ui.label(format!("录制候选：{}", format_shortcut(candidate, platform))); match shortcut_config::check_key(candidate, platform) { Ok(()) => { if ui.add_enabled(self.keys.len() < 4 && !self.keys.contains(&candidate), egui::Button::new("加入绑定列表")).clicked() { self.keys.push(candidate); self.candidate = None; } }, Err(e) => { ui.colored_label(crate::ui::tokens::warning_text(ui.visuals()), e.to_string()); } } }
                            ui.horizontal(|ui| {
                                if ui.button("确认此命令并自动保存").clicked() { match self.current.config.replace(id, self.keys.clone(), platform) { Ok(v) => self.save(ctx, v.config, false), Err(e) => self.message = Some(e.to_string()) } }
                                if ui.button("清除此命令绑定").clicked() { let next = self.current.config.replace(id, vec![], platform).expect("clearing cannot conflict"); self.save(ctx, next.config, false); }
                                if ui.button("此命令恢复默认").clicked() { match self.current.config.replace(id, Config::defaults(platform).entry(id).shortcuts.clone(), platform) { Ok(v) => self.save(ctx, v.config, false), Err(e) => self.message = Some(e.to_string()) } }
                            });
                        }
                    });
                }
            }
            });
            ui.separator(); if ui.add_enabled(self.pending.is_none() && !self.recording && !ime, egui::Button::new("关闭设置")).clicked() { self.close(); }
        });
        if !self.recording
            && self.pending.is_none()
            && !ime
            && !event_text_focus
            && response.should_close()
        {
            self.close();
        }
    }
}
fn exit_phase(name: &str, state: &Settings) {
    #[cfg(feature = "internal-evidence")]
    if std::env::var("RCAM_SHORTCUT_EXIT_PHASE").ok().as_deref() == Some(name) {
        if let Some(dir) = shortcut_store::native_directory() {
            let marker = serde_json::json!({"schema_version":2,"phase":name,"runtime_generation":state.generation,"runtime_snapshot_sha256":state.current.config.bytes().ok().map(|bytes| editor_core::hash::sha256_hex(&bytes))});
            let _ = std::fs::write(dir.join("exit-observation.json"), marker.to_string());
        }
        std::process::exit(86);
    }
    #[cfg(not(feature = "internal-evidence"))]
    let _ = (name, state);
}
fn catalogue_hash() -> String {
    editor_core::hash::sha256_hex(
        &serde_json::to_vec(&shortcut_config::commands()).expect("serializable catalogue"),
    )
}
fn defaults_hash(platform: Platform) -> String {
    editor_core::hash::sha256_hex(
        &Config::defaults(platform)
            .bytes()
            .expect("bounded defaults"),
    )
}
fn format_shortcut(s: Shortcut, platform: Platform) -> String {
    shortcut_config::format_shortcut(s, platform)
}
pub(crate) fn display_keys(keys: &[Shortcut], platform: Platform) -> String {
    if keys.is_empty() {
        "未绑定".into()
    } else {
        keys.iter()
            .map(|&s| format_shortcut(s, platform))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}
pub(crate) fn logical(m: egui::Modifiers, platform: Platform) -> Modifiers {
    platform.logical(PhysicalModifiers {
        ctrl: m.ctrl,
        alt: m.alt,
        shift: m.shift,
        command: m.mac_cmd || (platform == Platform::Windows && m.command && !m.ctrl),
    })
}
pub(crate) fn key_from_egui(key: egui::Key) -> Option<Key> {
    use egui::Key as E;
    Some(match key {
        E::Delete => Key::Delete,
        E::Backspace => Key::Backspace,
        E::Enter => Key::Enter,
        E::Escape => Key::Escape,
        E::Tab => Key::Tab,
        E::Space => Key::Space,
        E::ArrowUp => Key::ArrowUp,
        E::ArrowDown => Key::ArrowDown,
        E::ArrowLeft => Key::ArrowLeft,
        E::ArrowRight => Key::ArrowRight,
        E::A => Key::Char('a'),
        E::B => Key::Char('b'),
        E::C => Key::Char('c'),
        E::D => Key::Char('d'),
        E::E => Key::Char('e'),
        E::F => Key::Char('f'),
        E::G => Key::Char('g'),
        E::H => Key::Char('h'),
        E::I => Key::Char('i'),
        E::J => Key::Char('j'),
        E::K => Key::Char('k'),
        E::L => Key::Char('l'),
        E::M => Key::Char('m'),
        E::N => Key::Char('n'),
        E::O => Key::Char('o'),
        E::P => Key::Char('p'),
        E::Q => Key::Char('q'),
        E::R => Key::Char('r'),
        E::S => Key::Char('s'),
        E::T => Key::Char('t'),
        E::U => Key::Char('u'),
        E::V => Key::Char('v'),
        E::W => Key::Char('w'),
        E::X => Key::Char('x'),
        E::Y => Key::Char('y'),
        E::Z => Key::Char('z'),
        E::Num0 => Key::Char('0'),
        E::Num1 => Key::Char('1'),
        E::Num2 => Key::Char('2'),
        E::Num3 => Key::Char('3'),
        E::Num4 => Key::Char('4'),
        E::Num5 => Key::Char('5'),
        E::Num6 => Key::Char('6'),
        E::Num7 => Key::Char('7'),
        E::Num8 => Key::Char('8'),
        E::Num9 => Key::Char('9'),
        E::F1 => Key::F(1),
        E::F2 => Key::F(2),
        E::F3 => Key::F(3),
        E::F4 => Key::F(4),
        E::F5 => Key::F(5),
        E::F6 => Key::F(6),
        E::F7 => Key::F(7),
        E::F8 => Key::F(8),
        E::F9 => Key::F(9),
        E::F10 => Key::F(10),
        E::F11 => Key::F(11),
        E::F12 => Key::F(12),
        E::F13 => Key::F(13),
        E::F14 => Key::F(14),
        E::F15 => Key::F(15),
        E::F16 => Key::F(16),
        E::F17 => Key::F(17),
        E::F18 => Key::F(18),
        E::F19 => Key::F(19),
        E::F20 => Key::F(20),
        E::F21 => Key::F(21),
        E::F22 => Key::F(22),
        E::F23 => Key::F(23),
        E::F24 => Key::F(24),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(
        key: egui::Key,
        pressed: bool,
        repeat: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat,
            modifiers,
        }
    }
    fn raw(events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            events,
            modifiers,
            focused: true,
            ..Default::default()
        }
    }
    #[test]
    fn capture_repeat_release_ime_and_barrier_do_not_publish() {
        let mut s = Settings::load(None, Platform::MacOs);
        s.open = true;
        s.recording = true;
        let old = s.current.config.bytes().unwrap();
        let m = egui::Modifiers {
            mac_cmd: true,
            command: true,
            ..Default::default()
        };
        let mut input = raw(vec![event(egui::Key::D, true, true, m)], m);
        s.raw_input(&mut input, false);
        assert!(s.recording);
        assert!(s.candidate.is_none());
        assert!(input.events.is_empty());
        let mut input = raw(vec![event(egui::Key::D, false, false, m)], m);
        s.raw_input(&mut input, false);
        assert!(s.recording);
        let mut input = raw(vec![event(egui::Key::D, true, false, m)], m);
        s.raw_input(&mut input, true);
        assert!(s.recording);
        assert!(s.candidate.is_none());
        assert_eq!(input.events.len(), 1); // IME retains input ownership.
        let mut input = raw(
            vec![event(egui::Key::D, false, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        s.raw_input(&mut input, false);
        assert_eq!(input.events.len(), 1); // release reaches egui so its keys_down cannot become stale.
        let mut input = raw(vec![event(egui::Key::D, true, false, m)], m);
        s.raw_input(&mut input, false);
        assert!(!s.recording);
        assert_eq!(
            s.candidate,
            Some(Shortcut::new(Modifiers::PRIMARY, Key::Char('d')))
        );
        assert!(input.events.is_empty());
        s.close();
        let mut input = raw(vec![event(egui::Key::D, true, true, m)], m);
        s.raw_input(&mut input, false);
        assert!(input.events.is_empty());
        let mut input = raw(
            vec![event(egui::Key::D, false, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        s.raw_input(&mut input, false);
        assert_eq!(input.events.len(), 1);
        let mut input = raw(vec![event(egui::Key::D, true, false, m)], m);
        s.raw_input(&mut input, false);
        assert_eq!(input.events.len(), 1);
        assert_eq!(s.current.config.bytes().unwrap(), old);
        assert_eq!(s.generation, 0);
    }
    #[test]
    fn esc_tab_enter_capture_controls_and_exact_platform_modifiers() {
        for key in [egui::Key::Escape, egui::Key::Tab] {
            let mut s = Settings::load(None, Platform::MacOs);
            s.open = true;
            s.recording = true;
            let mut input = raw(
                vec![event(key, true, false, egui::Modifiers::NONE)],
                egui::Modifiers::NONE,
            );
            s.raw_input(&mut input, false);
            assert!(s.open);
            assert!(!s.recording);
            assert!(s.candidate.is_none());
        }
        let mut s = Settings::load(None, Platform::MacOs);
        s.open = true;
        s.recording = true;
        let mut input = raw(
            vec![event(egui::Key::Enter, true, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        s.raw_input(&mut input, false);
        assert!(s.recording);
        assert!(s.candidate.is_none());
        assert_eq!(
            logical(
                egui::Modifiers {
                    mac_cmd: true,
                    command: true,
                    ctrl: true,
                    alt: true,
                    shift: true
                },
                Platform::MacOs
            ),
            Modifiers {
                primary: true,
                secondary: true,
                alt: true,
                shift: true
            }
        );
        assert_eq!(
            logical(
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
                Platform::Windows
            ),
            Modifiers::PRIMARY
        );
    }
    #[test]
    fn immutable_preview_generation_and_protected_config_cannot_publish() {
        let ctx = egui::Context::default();
        let mut s = Settings::load(None, Platform::MacOs);
        let candidate =
            shortcut_config::validate(Config::defaults(Platform::MacOs), Platform::MacOs).unwrap();
        let hash = editor_core::hash::sha256_hex(&candidate.config.bytes().unwrap());
        s.preview = Some(Preview {
            candidate,
            generation: 0,
            hash,
            catalogue_hash: catalogue_hash(),
            defaults_hash: defaults_hash(Platform::current()),
            platform: Platform::current(),
        });
        s.generation = 1;
        s.apply_preview(&ctx);
        assert!(s.pending.is_none());
        assert!(s.message.as_ref().unwrap().contains("过期"));
        s.save(&ctx, Config::defaults(Platform::MacOs), false);
        assert!(s.pending.is_none());
        assert!(s.protected);
    }
    #[test]
    fn immutable_preview_applies_original_after_source_changes() {
        use editor_core::command::ids;
        let dir = std::env::temp_dir().join(format!("rcam-k1-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shortcuts.json");
        let source = dir.join("import.json");
        let _ = std::fs::remove_file(&path);
        let candidate = Config::defaults(Platform::MacOs)
            .replace(ids::VIEW_FIT, vec![], Platform::MacOs)
            .unwrap();
        std::fs::write(&source, candidate.config.bytes().unwrap()).unwrap();
        let validated = shortcut_store::import(&source, Platform::MacOs).unwrap();
        let mut state = Settings::load(Some(path.clone()), Platform::MacOs);
        state.preview = Some(Preview {
            hash: editor_core::hash::sha256_hex(&validated.config.bytes().unwrap()),
            candidate: validated,
            generation: state.generation,
            catalogue_hash: catalogue_hash(),
            defaults_hash: defaults_hash(Platform::current()),
            platform: Platform::current(),
        });
        std::fs::write(&source, b"not the preview").unwrap();
        state.apply_preview(&egui::Context::default());
        for _ in 0..400 {
            state.poll();
            if state.pending.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(state.generation, 1);
        assert_eq!(state.current.config, candidate.config);
        assert_eq!(
            shortcut_store::load(Some(&path), Platform::MacOs)
                .current
                .config,
            candidate.config
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use crate::EditorApp;
    use editor_core::command::{CommandDispatcher, ids};
    fn run(app: &mut EditorApp, events: Vec<egui::Event>, focused: bool, text: bool, modal: bool) {
        let ctx = egui::Context::default();
        let releases = events
            .iter()
            .filter_map(|event| {
                if let egui::Event::Key {
                    key, pressed: true, ..
                } = event
                {
                    Some(egui::Event::Key {
                        key: *key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                } else {
                    None
                }
            })
            .collect();
        let mut raw = egui::RawInput {
            events,
            focused,
            ..Default::default()
        };
        app.shortcuts
            .raw_input(&mut raw, app.ime_active || app.ime_event);
        let _ = ctx.run(raw, |ctx| {
            app.route_shortcuts(ctx, text, modal);
        });
        let mut raw = egui::RawInput {
            events: releases,
            focused,
            ..Default::default()
        };
        app.shortcuts
            .raw_input(&mut raw, app.ime_active || app.ime_event);
        let _ = ctx.run(raw, |_| {});
    }
    fn key(key: egui::Key, repeat: bool, m: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat,
            modifiers: m,
        }
    }
    #[test]
    fn exact_keys_repeat_focus_ime_modal_and_disabled_gates_use_actual_router() {
        let mut app = crate::modal::tests::app();
        let before = app.object_snap.enabled;
        for (focused, text, modal, repeat, mods) in [
            (false, false, false, false, egui::Modifiers::NONE),
            (true, true, false, false, egui::Modifiers::NONE),
            (true, false, true, false, egui::Modifiers::NONE),
            (true, false, false, true, egui::Modifiers::NONE),
            (true, false, false, false, egui::Modifiers::SHIFT),
        ] {
            run(
                &mut app,
                vec![key(egui::Key::F3, repeat, mods)],
                focused,
                text,
                modal,
            );
            assert_eq!(app.object_snap.enabled, before);
        }
        app.ime_event = true;
        run(
            &mut app,
            vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
            true,
            false,
            false,
        );
        assert_eq!(app.object_snap.enabled, before);
        app.ime_event = false;
        app.busy = true;
        run(
            &mut app,
            vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
            true,
            false,
            false,
        );
        assert_eq!(app.object_snap.enabled, before);
        app.busy = false;
        assert!(!app.dispatch(ids::EDIT_DELETE));
        assert_eq!(app.sequence, 0);
        run(
            &mut app,
            vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
            true,
            false,
            false,
        );
        assert_ne!(app.object_snap.enabled, before);
        assert_eq!(app.sequence, 0);
    }
    #[test]
    fn two_unrelated_presses_are_ordered_and_rebinding_removes_old_key() {
        let mut app = crate::modal::tests::app();
        let before = app.object_snap.enabled;
        let grid = app.grid.visible;
        app.shortcuts.current = app
            .shortcuts
            .current
            .config
            .replace(
                ids::VIEW_GRID_TOGGLE,
                vec![Shortcut::new(Modifiers::NONE, Key::Char('b'))],
                Platform::MacOs,
            )
            .unwrap();
        run(
            &mut app,
            vec![
                key(egui::Key::F3, false, egui::Modifiers::NONE),
                key(egui::Key::B, false, egui::Modifiers::NONE),
            ],
            true,
            false,
            false,
        );
        assert_ne!(app.object_snap.enabled, before);
        assert_ne!(app.grid.visible, grid);
        app.shortcuts.current = app
            .shortcuts
            .current
            .config
            .replace(
                ids::SNAP_TOGGLE,
                vec![Shortcut::new(Modifiers::NONE, Key::F(6))],
                Platform::MacOs,
            )
            .unwrap();
        let before = app.object_snap.enabled;
        run(
            &mut app,
            vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
            true,
            false,
            false,
        );
        assert_eq!(app.object_snap.enabled, before);
        run(
            &mut app,
            vec![key(egui::Key::F6, false, egui::Modifiers::NONE)],
            true,
            false,
            false,
        );
        assert_ne!(app.object_snap.enabled, before);
    }
    #[test]
    fn real_context_repeat_release_and_new_text_focus_preserve_input_ownership() {
        let ctx = egui::Context::default();
        let mut app = crate::modal::tests::app();
        let before = app.object_snap.enabled;
        let mut raw = egui::RawInput {
            events: vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        let _ = ctx.run(raw, |ctx| app.route_shortcuts(ctx, true, false));
        let mut raw = egui::RawInput {
            events: vec![key(egui::Key::F3, true, egui::Modifiers::NONE)],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        let _ = ctx.run(raw, |ctx| app.route_shortcuts(ctx, false, false));
        assert_eq!(app.object_snap.enabled, before);
        app.shortcuts.open = true;
        app.shortcuts.recording = true;
        let mut raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::F3,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        let _ = ctx.run(raw, |ctx| {
            assert!(!ctx.input(|i| i.key_down(egui::Key::F3)))
        });
        app.shortcuts.close();
        let mut raw = egui::RawInput {
            events: vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        let _ = ctx.run(raw, |ctx| app.route_shortcuts(ctx, false, false));
        assert_ne!(app.object_snap.enabled, before);
        // A new text field takes ownership in this frame, before the late command router.
        let mut text = "1234".to_string();
        let id = egui::Id::new("same-frame-text-owner");
        let before = app.object_snap.enabled;
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
                focused: true,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut text).id(id))
                        .request_focus();
                });
                app.route_shortcuts(ctx, ctx.wants_keyboard_input(), false);
            },
        );
        assert_eq!(app.object_snap.enabled, before);
        let mut raw = egui::RawInput {
            events: vec![key(egui::Key::Backspace, false, egui::Modifiers::NONE)],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id));
            });
        });
        let after_first = text.len();
        let mut raw = egui::RawInput {
            events: vec![key(egui::Key::Backspace, true, egui::Modifiers::NONE)],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        assert_eq!(raw.events.len(), 1);
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id));
            });
        });
        assert!(text.len() < after_first);
    }
    #[test]
    fn multipass_and_mixed_backend_repeats_dispatch_one_real_press() {
        let ctx = egui::Context::default();
        let mut app = crate::modal::tests::app();
        let before = app.object_snap.enabled;
        let mut raw = egui::RawInput {
            events: vec![key(egui::Key::F3, false, egui::Modifiers::NONE)],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        let mut passes = 0;
        let _ = ctx.run(raw, |ctx| {
            passes += 1;
            app.route_shortcuts(ctx, false, false);
            if passes == 1 {
                ctx.request_discard("shortcut multi-pass regression");
            }
        });
        assert_eq!(passes, 2);
        assert_ne!(app.object_snap.enabled, before);
        assert!(app.shortcuts.presses.is_empty());
        let mut app = crate::modal::tests::app();
        let before = app.object_snap.enabled;
        let mut raw = egui::RawInput {
            events: vec![
                key(egui::Key::F3, true, egui::Modifiers::NONE),
                egui::Event::Key {
                    key: egui::Key::F3,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                key(egui::Key::F3, false, egui::Modifiers::NONE),
            ],
            focused: true,
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        assert_eq!(app.shortcuts.presses.len(), 1);
        let _ = egui::Context::default().run(raw, |ctx| app.route_shortcuts(ctx, false, false));
        assert_ne!(app.object_snap.enabled, before);
    }
    #[test]
    fn settings_save_does_not_change_real_document_or_view_state() {
        let mut model = crate::state::Model::default();
        model
            .open(
                &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../fixtures/synthetic/s4c2/grips.gbr"),
            )
            .unwrap();
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        let doc = app.view.info.as_ref().unwrap().document_id.clone();
        let before = serde_json::to_vec(&model.service.render_snapshot(&doc).unwrap()).unwrap();
        let info = serde_json::to_vec(&app.view.info).unwrap();
        let camera = app.camera;
        let prefs = serde_json::to_vec(&app.prefs).unwrap();
        let selected = app.view.selected.ids();
        let dir = std::env::temp_dir().join(format!("rcam-k1-invariants-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shortcuts.json");
        let _ = std::fs::remove_file(&path);
        app.shortcuts = Settings::load(Some(path), Platform::MacOs);
        app.shortcuts.open = true;
        let candidate = app
            .shortcuts
            .current
            .config
            .replace(
                ids::SNAP_TOGGLE,
                vec![Shortcut::new(Modifiers::NONE, Key::F(6))],
                Platform::MacOs,
            )
            .unwrap();
        app.shortcuts
            .save(&egui::Context::default(), candidate.config, false);
        for _ in 0..200 {
            app.shortcuts.poll();
            if app.shortcuts.pending.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.shortcuts.generation, 1);
        assert_eq!(app.sequence, 0);
        assert!(!app.busy);
        assert_eq!(serde_json::to_vec(&app.view.info).unwrap(), info);
        assert_eq!(
            serde_json::to_vec(&model.service.render_snapshot(&doc).unwrap()).unwrap(),
            before
        );
        assert_eq!(serde_json::to_vec(&app.prefs).unwrap(), prefs);
        assert_eq!(app.camera.center, camera.center);
        assert_eq!(app.camera.scale, camera.scale);
        assert_eq!(app.view.selected.ids(), selected);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn duplicated_physical_press_is_filtered_but_new_press_after_release_is_kept() {
        let mut s = Settings::load(None, Platform::MacOs);
        let e = key(egui::Key::F3, false, egui::Modifiers::NONE);
        let mut raw = egui::RawInput {
            events: vec![
                e.clone(),
                e.clone(),
                key(egui::Key::B, false, egui::Modifiers::NONE),
            ],
            focused: true,
            ..Default::default()
        };
        s.raw_input(&mut raw, false);
        assert_eq!(raw.events.len(), 2);
        let mut raw = egui::RawInput {
            events: vec![
                egui::Event::Key {
                    key: egui::Key::F3,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                e,
            ],
            focused: true,
            ..Default::default()
        };
        s.raw_input(&mut raw, false);
        assert_eq!(raw.events.len(), 2);
    }
}
