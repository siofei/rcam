//! Command / Keymap / Shortcut architecture (S4-B1).
//!
//! Flow: `key event -> Keymap -> CommandId -> CommandDispatcher -> ApplicationService`.
//! Menu, Toolbar, Context Menu and shortcuts all invoke the same `CommandId`.
//! Automation never simulates shortcuts; it calls `ApplicationService`
//! directly. Logical modifiers (`Primary`, `Secondary`) keep macOS Cmd and
//! Windows Ctrl out of the bindings. User overrides belong to AppPreferences,
//! never to a `.rcam` project.

use serde::{Deserialize, Serialize};

/// Stable, string-typed command identity (e.g. `file.import_gerber`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct CommandId(pub &'static str);

/// Canonical command ids known to the registry.
pub mod ids {
    use super::CommandId;
    pub const FILE_IMPORT_GERBER: CommandId = CommandId("file.import_gerber");
    pub const FILE_EXPORT_GERBER: CommandId = CommandId("file.export_gerber");
    pub const FILE_NEW: CommandId = CommandId("file.new");
    pub const FILE_NEW_PROJECT: CommandId = CommandId("file.new_project");
    pub const FILE_OPEN_PROJECT: CommandId = CommandId("file.open_project");
    pub const FILE_SAVE_PROJECT: CommandId = CommandId("file.save_project");
    pub const FILE_SAVE_PROJECT_AS: CommandId = CommandId("file.save_project_as");
    pub const FILE_CLOSE_PROJECT: CommandId = CommandId("file.close_project");
    pub const EDIT_UNDO: CommandId = CommandId("edit.undo");
    pub const EDIT_REDO: CommandId = CommandId("edit.redo");
    pub const EDIT_SELECT_ALL: CommandId = CommandId("edit.select_all");
    pub const EDIT_DELETE: CommandId = CommandId("edit.delete");
    pub const EDIT_DUPLICATE: CommandId = CommandId("edit.duplicate");
    pub const OBJECT_MOVE: CommandId = CommandId("object.move");
    pub const OBJECT_ROTATE: CommandId = CommandId("object.rotate");
    pub const OBJECT_MIRROR: CommandId = CommandId("object.mirror");
    pub const OBJECT_ARRAY_RECTANGULAR: CommandId = CommandId("objects.array_rectangular");
    pub const OBJECT_ALIGN_LEFT: CommandId = CommandId("objects.align_left");
    pub const OBJECT_ALIGN_RIGHT: CommandId = CommandId("objects.align_right");
    pub const OBJECT_ALIGN_TOP: CommandId = CommandId("objects.align_top");
    pub const OBJECT_ALIGN_BOTTOM: CommandId = CommandId("objects.align_bottom");
    pub const OBJECT_ALIGN_HCENTER: CommandId = CommandId("objects.align_hcenter");
    pub const OBJECT_ALIGN_VCENTER: CommandId = CommandId("objects.align_vcenter");
    pub const OBJECT_DISTRIBUTE_HORIZONTAL: CommandId = CommandId("objects.distribute_horizontal");
    pub const OBJECT_DISTRIBUTE_VERTICAL: CommandId = CommandId("objects.distribute_vertical");
    pub const VIEW_FIT: CommandId = CommandId("view.fit");
    pub const VIEW_FIT_ACTIVE_LAYER: CommandId = CommandId("view.fit_active_layer");
    pub const VIEW_GRID_TOGGLE: CommandId = CommandId("view.grid.toggle");
    pub const LAYER_CREATE: CommandId = CommandId("layer.create");
    pub const LAYER_DELETE: CommandId = CommandId("layer.delete");
    pub const LAYER_SOLO: CommandId = CommandId("layer.solo");
    pub const TOOL_SELECT: CommandId = CommandId("tool.select");
    pub const TOOL_MEASURE: CommandId = CommandId("tool.measure");
    pub const TOOL_TEXT: CommandId = CommandId("tool.text");
    pub const BLOCK_CREATE: CommandId = CommandId("block.create_from_selection");
    pub const BLOCK_PLACE: CommandId = CommandId("block.place");
    pub const BLOCK_RENAME: CommandId = CommandId("block.rename");
    pub const BLOCK_EXPLODE: CommandId = CommandId("block.explode");
    pub const BLOCK_DELETE: CommandId = CommandId("block.delete_definition");
    pub const BLOCK_SELECT: CommandId = CommandId("block.select_instances");
    pub const BLOCK_TRANSFORM: CommandId = CommandId("block.transform");
    pub const GRIP_CANCEL: CommandId = CommandId("grip.cancel");
    pub const SNAP_TOGGLE: CommandId = CommandId("snap.toggle");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCategory {
    File,
    Edit,
    Object,
    View,
    Layer,
    Tool,
    Snap,
}

/// Logical modifiers: `Primary` is Cmd on macOS and Ctrl on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifiers {
    pub primary: bool,
    pub secondary: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        primary: false,
        secondary: false,
        shift: false,
        alt: false,
    };
    pub const PRIMARY: Self = Self {
        primary: true,
        ..Self::NONE
    };
    pub const SHIFT: Self = Self {
        shift: true,
        ..Self::NONE
    };
    pub const PRIMARY_SHIFT: Self = Self {
        primary: true,
        shift: true,
        ..Self::NONE
    };
}

/// Modifier state as reported by the windowing layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhysicalModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on macOS; the Windows key elsewhere.
    pub command: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Windows
        }
    }

    pub fn logical(self, physical: PhysicalModifiers) -> Modifiers {
        match self {
            Self::MacOs => Modifiers {
                primary: physical.command,
                secondary: physical.ctrl,
                shift: physical.shift,
                alt: physical.alt,
            },
            Self::Windows => Modifiers {
                primary: physical.ctrl,
                secondary: physical.command,
                shift: physical.shift,
                alt: physical.alt,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Char(char),
    Delete,
    Backspace,
    Enter,
    Escape,
    Tab,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Shortcut {
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShortcutContext {
    Global,
    Canvas,
    Modal,
    TextInput,
    TextPlacement,
    ObjectEdit,
}

impl ShortcutContext {
    /// Larger wins: IME/TextInput > Modal > current Tool > Canvas > Global.
    pub fn priority(self) -> u8 {
        match self {
            Self::TextInput => 5,
            Self::Modal => 4,
            Self::TextPlacement | Self::ObjectEdit => 3,
            Self::Canvas => 2,
            Self::Global => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandDescriptor {
    pub id: CommandId,
    pub name: &'static str,
    pub category: CommandCategory,
    pub default_shortcut: Option<Shortcut>,
    pub context: ShortcutContext,
}

/// All commands, in menu order. Every UI entry point refers to these ids.
pub fn standard_commands() -> Vec<CommandDescriptor> {
    use CommandCategory as C;
    use ShortcutContext as X;
    let ch = |c: char| Key::Char(c);
    let d = |id, name, category, shortcut: Option<Shortcut>, context| CommandDescriptor {
        id,
        name,
        category,
        default_shortcut: shortcut,
        context,
    };
    vec![
        d(
            ids::FILE_NEW_PROJECT,
            "新建工程",
            C::File,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('n'))),
            X::Global,
        ),
        d(
            ids::FILE_OPEN_PROJECT,
            "打开工程…",
            C::File,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('o'))),
            X::Global,
        ),
        d(
            ids::FILE_SAVE_PROJECT,
            "保存工程",
            C::File,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('s'))),
            X::Global,
        ),
        d(
            ids::FILE_SAVE_PROJECT_AS,
            "工程另存为…",
            C::File,
            Some(Shortcut::new(Modifiers::PRIMARY_SHIFT, ch('s'))),
            X::Global,
        ),
        d(
            ids::FILE_CLOSE_PROJECT,
            "关闭工程",
            C::File,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('w'))),
            X::Global,
        ),
        d(ids::FILE_NEW, "新建工作区", C::File, None, X::Global),
        d(
            ids::FILE_IMPORT_GERBER,
            "导入 Gerber…",
            C::File,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('i'))),
            X::Global,
        ),
        d(
            ids::FILE_EXPORT_GERBER,
            "导出 Gerber…",
            C::File,
            Some(Shortcut::new(Modifiers::PRIMARY_SHIFT, ch('e'))),
            X::Global,
        ),
        d(
            ids::EDIT_UNDO,
            "撤销",
            C::Edit,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('z'))),
            X::Global,
        ),
        d(
            ids::EDIT_REDO,
            "重做",
            C::Edit,
            Some(Shortcut::new(Modifiers::PRIMARY_SHIFT, ch('z'))),
            X::Global,
        ),
        d(
            ids::EDIT_SELECT_ALL,
            "全选可选对象",
            C::Edit,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('a'))),
            X::Canvas,
        ),
        d(
            ids::EDIT_DELETE,
            "删除",
            C::Edit,
            Some(Shortcut::new(Modifiers::NONE, Key::Delete)),
            X::Canvas,
        ),
        d(
            ids::EDIT_DUPLICATE,
            "复制对象",
            C::Edit,
            Some(Shortcut::new(Modifiers::PRIMARY, ch('d'))),
            X::Canvas,
        ),
        d(ids::BLOCK_CREATE, "创建 Block…", C::Object, None, X::Canvas),
        d(ids::BLOCK_PLACE, "放置 Block", C::Object, None, X::Canvas),
        d(
            ids::BLOCK_RENAME,
            "重命名 Block…",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::BLOCK_EXPLODE,
            "拆解 Block…",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::BLOCK_DELETE,
            "删除 Block 定义…",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::BLOCK_SELECT,
            "选择 Block 实例",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::BLOCK_TRANSFORM,
            "实例变换…",
            C::Object,
            None,
            X::Canvas,
        ),
        d(ids::OBJECT_MOVE, "移动…", C::Object, None, X::Canvas),
        d(ids::OBJECT_ROTATE, "旋转…", C::Object, None, X::Canvas),
        d(ids::OBJECT_MIRROR, "镜像…", C::Object, None, X::Canvas),
        d(
            ids::OBJECT_ARRAY_RECTANGULAR,
            "矩形阵列…",
            C::Object,
            None,
            X::Canvas,
        ),
        d(ids::OBJECT_ALIGN_LEFT, "左对齐", C::Object, None, X::Canvas),
        d(
            ids::OBJECT_ALIGN_RIGHT,
            "右对齐",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::OBJECT_ALIGN_TOP,
            "顶端对齐",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::OBJECT_ALIGN_BOTTOM,
            "底端对齐",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::OBJECT_ALIGN_HCENTER,
            "水平居中",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::OBJECT_ALIGN_VCENTER,
            "垂直居中",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::OBJECT_DISTRIBUTE_HORIZONTAL,
            "水平等距分布",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::OBJECT_DISTRIBUTE_VERTICAL,
            "垂直等距分布",
            C::Object,
            None,
            X::Canvas,
        ),
        d(
            ids::VIEW_FIT,
            "适应可见图层",
            C::View,
            Some(Shortcut::new(Modifiers::NONE, ch('f'))),
            X::Canvas,
        ),
        d(
            ids::VIEW_FIT_ACTIVE_LAYER,
            "适应当前图层",
            C::View,
            Some(Shortcut::new(Modifiers::SHIFT, ch('f'))),
            X::Canvas,
        ),
        d(
            ids::VIEW_GRID_TOGGLE,
            "显示/隐藏网格",
            C::View,
            Some(Shortcut::new(Modifiers::NONE, ch('g'))),
            X::Canvas,
        ),
        d(
            ids::LAYER_CREATE,
            "新建空白图层…",
            C::Layer,
            Some(Shortcut::new(Modifiers::PRIMARY_SHIFT, ch('n'))),
            X::Global,
        ),
        d(ids::LAYER_DELETE, "删除图层…", C::Layer, None, X::Global),
        d(ids::LAYER_SOLO, "Solo 当前图层", C::Layer, None, X::Global),
        d(
            ids::TOOL_SELECT,
            "选择工具",
            C::Tool,
            Some(Shortcut::new(Modifiers::NONE, ch('v'))),
            X::Canvas,
        ),
        d(
            ids::TOOL_MEASURE,
            "测距工具",
            C::Tool,
            Some(Shortcut::new(Modifiers::NONE, ch('m'))),
            X::Canvas,
        ),
        d(
            ids::TOOL_TEXT,
            "文字工具",
            C::Tool,
            Some(Shortcut::new(Modifiers::NONE, ch('t'))),
            X::Canvas,
        ),
        d(
            ids::GRIP_CANCEL,
            "取消控制点编辑",
            C::Tool,
            Some(Shortcut::new(Modifiers::NONE, Key::Escape)),
            X::ObjectEdit,
        ),
        d(
            ids::SNAP_TOGGLE,
            "开关吸附",
            C::Snap,
            Some(Shortcut::new(Modifiers::NONE, Key::F(3))),
            X::Canvas,
        ),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub shortcut: Shortcut,
    pub context: ShortcutContext,
    /// `None` explicitly unbinds a default in that context.
    pub command: Option<CommandId>,
}

/// Default keymap plus user overrides (AppPreferences data).
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    defaults: Vec<Binding>,
    overrides: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapConflict {
    pub shortcut: Shortcut,
    pub context: ShortcutContext,
    pub commands: Vec<CommandId>,
}

impl Keymap {
    pub fn standard() -> Self {
        let mut defaults = Vec::new();
        for command in standard_commands() {
            if let Some(shortcut) = command.default_shortcut {
                defaults.push(Binding {
                    shortcut,
                    context: command.context,
                    command: Some(command.id),
                });
            }
        }
        // Backspace deletes like Delete on Canvas.
        defaults.push(Binding {
            shortcut: Shortcut::new(Modifiers::NONE, Key::Backspace),
            context: ShortcutContext::Canvas,
            command: Some(ids::EDIT_DELETE),
        });
        Self {
            defaults,
            overrides: Vec::new(),
        }
    }

    /// Install a complete, already validated set; never sequentially overwrite conflicts.
    pub fn from_bindings(bindings: Vec<Binding>) -> Self {
        Self {
            defaults: bindings,
            overrides: Vec::new(),
        }
    }

    pub fn set_override(&mut self, binding: Binding) {
        self.overrides
            .retain(|b| !(b.shortcut == binding.shortcut && b.context == binding.context));
        self.overrides.push(binding);
    }

    pub fn clear_overrides(&mut self) {
        self.overrides.clear();
    }

    fn effective(&self) -> Vec<Binding> {
        let mut result: Vec<Binding> = self
            .defaults
            .iter()
            .filter(|d| {
                !self
                    .overrides
                    .iter()
                    .any(|o| o.shortcut == d.shortcut && o.context == d.context)
            })
            .copied()
            .collect();
        result.extend(self.overrides.iter().copied());
        result
    }

    fn lookup(&self, context: ShortcutContext, shortcut: Shortcut) -> Option<Option<CommandId>> {
        self.overrides
            .iter()
            .rev()
            .chain(self.defaults.iter())
            .find(|b| b.context == context && b.shortcut == shortcut)
            .map(|b| b.command)
    }

    /// Two different commands on one (shortcut, context).
    pub fn conflicts(&self) -> Vec<KeymapConflict> {
        let effective = self.effective();
        let mut result: Vec<KeymapConflict> = Vec::new();
        for (i, a) in effective.iter().enumerate() {
            let Some(command) = a.command else { continue };
            for b in &effective[i + 1..] {
                let Some(other) = b.command else { continue };
                if b.shortcut != a.shortcut || b.context != a.context || other == command {
                    continue;
                }
                let entry = result
                    .iter_mut()
                    .find(|c| c.shortcut == a.shortcut && c.context == a.context);
                match entry {
                    Some(entry) => {
                        for id in [command, other] {
                            if !entry.commands.contains(&id) {
                                entry.commands.push(id);
                            }
                        }
                    }
                    None => result.push(KeymapConflict {
                        shortcut: a.shortcut,
                        context: a.context,
                        commands: vec![command, other],
                    }),
                }
            }
        }
        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Run this command.
    Command(CommandId),
    /// A higher-priority context (IME / Modal) owns the key: do nothing else.
    Blocked,
    /// No binding applies; the caller may route the event elsewhere.
    Unbound,
}

pub struct ShortcutResolver;

impl ShortcutResolver {
    /// Resolve a key press against the active contexts (any order).
    /// While `TextInput` or `Modal` is active nothing below it can fire, so
    /// Delete/Enter/Esc typed into a text field never reaches manufacturing edits.
    pub fn resolve(keymap: &Keymap, active: &[ShortcutContext], shortcut: Shortcut) -> Resolution {
        let mut contexts: Vec<ShortcutContext> = active.to_vec();
        contexts.sort_by_key(|c| std::cmp::Reverse(c.priority()));
        contexts.dedup();
        let mut lower_blocked = false;
        for context in contexts {
            if lower_blocked {
                return Resolution::Blocked;
            }
            match keymap.lookup(context, shortcut) {
                Some(Some(command)) => return Resolution::Command(command),
                Some(None) => return Resolution::Unbound,
                None => {}
            }
            if matches!(context, ShortcutContext::TextInput | ShortcutContext::Modal) {
                lower_blocked = true;
            }
        }
        if lower_blocked {
            Resolution::Blocked
        } else {
            Resolution::Unbound
        }
    }
}

/// Executes a command. GUI implements it on top of `ApplicationService`.
pub trait CommandDispatcher {
    type Outcome;
    fn dispatch(&mut self, command: CommandId) -> Self::Outcome;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_the_required_commands_with_unique_ids() {
        let commands = standard_commands();
        let mut ids: Vec<_> = commands.iter().map(|c| c.id.0).collect();
        for required in [
            "file.import_gerber",
            "file.export_gerber",
            "edit.undo",
            "edit.redo",
            "edit.delete",
            "edit.duplicate",
            "object.move",
            "object.rotate",
            "object.mirror",
            "objects.align_left",
            "objects.align_right",
            "objects.align_top",
            "objects.align_bottom",
            "objects.align_hcenter",
            "objects.align_vcenter",
            "objects.distribute_horizontal",
            "objects.distribute_vertical",
            "view.fit",
            "view.grid.toggle",
            "layer.create",
            "layer.delete",
            "layer.solo",
            "tool.select",
            "tool.measure",
            "tool.text",
            "snap.toggle",
        ] {
            assert!(ids.contains(&required), "missing {required}");
        }
        ids.sort_unstable();
        let unique = ids.len();
        ids.dedup();
        assert_eq!(unique, ids.len(), "duplicate command id");
    }

    #[test]
    fn standard_keymap_has_no_conflicts() {
        assert!(Keymap::standard().conflicts().is_empty());
    }

    #[test]
    fn modifiers_map_logically_per_platform() {
        let physical = PhysicalModifiers {
            command: true,
            ..Default::default()
        };
        assert!(Platform::MacOs.logical(physical).primary);
        assert!(!Platform::Windows.logical(physical).primary);
        let ctrl = PhysicalModifiers {
            ctrl: true,
            ..Default::default()
        };
        assert!(Platform::Windows.logical(ctrl).primary);
        assert!(!Platform::MacOs.logical(ctrl).primary && Platform::MacOs.logical(ctrl).secondary);
    }

    #[test]
    fn context_priority_blocks_manufacturing_shortcuts_during_text_input_and_modals() {
        let keymap = Keymap::standard();
        let delete = Shortcut::new(Modifiers::NONE, Key::Delete);
        assert_eq!(
            ShortcutResolver::resolve(
                &keymap,
                &[ShortcutContext::Global, ShortcutContext::Canvas],
                delete
            ),
            Resolution::Command(ids::EDIT_DELETE)
        );
        for blocking in [ShortcutContext::TextInput, ShortcutContext::Modal] {
            assert_eq!(
                ShortcutResolver::resolve(
                    &keymap,
                    &[ShortcutContext::Global, ShortcutContext::Canvas, blocking],
                    delete
                ),
                Resolution::Blocked,
                "{blocking:?}"
            );
        }
        // A global command is also blocked while a modal is open.
        let undo = Shortcut::new(Modifiers::PRIMARY, Key::Char('z'));
        assert_eq!(
            ShortcutResolver::resolve(
                &keymap,
                &[ShortcutContext::Global, ShortcutContext::Modal],
                undo
            ),
            Resolution::Blocked
        );
        assert_eq!(
            ShortcutResolver::resolve(&keymap, &[ShortcutContext::Global], undo),
            Resolution::Command(ids::EDIT_UNDO)
        );

        let f3 = Shortcut::new(Modifiers::NONE, Key::F(3));
        assert_eq!(
            ShortcutResolver::resolve(&keymap, &[ShortcutContext::Canvas], f3),
            Resolution::Command(ids::SNAP_TOGGLE)
        );
        for blocking in [ShortcutContext::TextInput, ShortcutContext::Modal] {
            assert_eq!(
                ShortcutResolver::resolve(&keymap, &[ShortcutContext::Canvas, blocking], f3),
                Resolution::Blocked,
                "F3 must respect {blocking:?}"
            );
        }
    }

    #[test]
    fn overrides_replace_defaults_and_conflicts_are_detected() {
        let mut keymap = Keymap::standard();
        let delete = Shortcut::new(Modifiers::NONE, Key::Delete);
        keymap.set_override(Binding {
            shortcut: delete,
            context: ShortcutContext::Canvas,
            command: Some(ids::TOOL_MEASURE),
        });
        assert_eq!(
            ShortcutResolver::resolve(&keymap, &[ShortcutContext::Canvas], delete),
            Resolution::Command(ids::TOOL_MEASURE)
        );
        assert!(keymap.conflicts().is_empty());
        // Binding Delete-in-Canvas twice through different commands is a conflict.
        let mut clash = Keymap::default();
        clash.defaults.push(Binding {
            shortcut: delete,
            context: ShortcutContext::Canvas,
            command: Some(ids::EDIT_DELETE),
        });
        clash.defaults.push(Binding {
            shortcut: delete,
            context: ShortcutContext::Canvas,
            command: Some(ids::TOOL_TEXT),
        });
        assert_eq!(clash.conflicts().len(), 1);
        keymap.set_override(Binding {
            shortcut: delete,
            context: ShortcutContext::Canvas,
            command: None,
        });
        assert_eq!(
            ShortcutResolver::resolve(&keymap, &[ShortcutContext::Canvas], delete),
            Resolution::Unbound
        );
        keymap.clear_overrides();
        assert_eq!(
            ShortcutResolver::resolve(&keymap, &[ShortcutContext::Canvas], delete),
            Resolution::Command(ids::EDIT_DELETE)
        );
    }
}
