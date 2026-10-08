//! S5-K1: bounded complete shortcut snapshots, independent of manufacturing.
use editor_core::command::{
    self, Binding, CommandId, Key, Keymap, Modifiers, Platform, Shortcut, ShortcutContext, ids,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const MAX_BYTES: usize = 64 * 1024;
pub(crate) const MAX_KEYS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourcePlatform {
    Macos,
    Windows,
}
impl SourcePlatform {
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::MacOs => Self::Macos,
            Platform::Windows => Self::Windows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub command_id: String,
    pub shortcuts: Vec<Shortcut>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub format: String,
    pub schema_version: u32,
    pub modifier_encoding: String,
    pub source_platform: SourcePlatform,
    pub bindings: Vec<Entry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Error {
    pub code: &'static str,
    pub message: String,
}
impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
#[derive(Debug, Clone)]
pub(crate) struct Validated {
    pub config: Config,
    pub keymap: Keymap,
    pub missing: Vec<String>,
    pub default_conflicts: Vec<String>,
    pub cross_platform: bool,
}

/// The six registry-only hotkeys were never active in the published GUI.
const INACTIVE_DEFAULTS: [CommandId; 6] = [
    ids::VIEW_FIT_ACTIVE_LAYER,
    ids::VIEW_GRID_TOGGLE,
    ids::LAYER_CREATE,
    ids::TOOL_SELECT,
    ids::TOOL_MEASURE,
    ids::TOOL_TEXT,
];
pub(crate) fn commands() -> Vec<command::CommandDescriptor> {
    command::standard_commands()
        .into_iter()
        .filter(|c| c.id != ids::GRIP_CANCEL)
        .collect()
}
fn control_all(platform: Platform) -> Shortcut {
    Shortcut::new(
        if platform == Platform::MacOs {
            Modifiers {
                secondary: true,
                ..Modifiers::NONE
            }
        } else {
            Modifiers::PRIMARY
        },
        Key::Char('a'),
    )
}
impl Config {
    pub fn defaults(platform: Platform) -> Self {
        let bindings = commands()
            .into_iter()
            .map(|c| {
                let mut shortcuts = if INACTIVE_DEFAULTS.contains(&c.id) {
                    vec![]
                } else {
                    c.default_shortcut.into_iter().collect()
                };
                // Physical Control was explicitly requested on both platforms.
                if c.id == ids::EDIT_SELECT_ALL && platform == Platform::MacOs {
                    shortcuts = vec![Shortcut::new(
                        Modifiers {
                            secondary: true,
                            ..Modifiers::NONE
                        },
                        Key::Char('a'),
                    )];
                }
                if c.id == ids::EDIT_DELETE {
                    shortcuts.push(Shortcut::new(Modifiers::NONE, Key::Backspace));
                }
                if c.id == ids::EDIT_REDO {
                    shortcuts.push(Shortcut::new(Modifiers::PRIMARY, Key::Char('y')));
                }
                Entry {
                    command_id: c.id.0.into(),
                    shortcuts,
                }
            })
            .collect();
        Self {
            format: "rcam-shortcuts".into(),
            schema_version: 1,
            modifier_encoding: "logical-v1".into(),
            source_platform: SourcePlatform::for_platform(platform),
            bindings,
        }
    }
    pub fn entry(&self, id: CommandId) -> &Entry {
        self.bindings
            .iter()
            .find(|e| e.command_id == id.0)
            .expect("validated complete command snapshot")
    }
    pub fn source(&self) -> Platform {
        match self.source_platform {
            SourcePlatform::Macos => Platform::MacOs,
            SourcePlatform::Windows => Platform::Windows,
        }
    }
    /// Config identity stays unchanged; only this exact physical-Control
    /// default transfers to the actual target keymap and its displayed hints.
    pub fn effective_shortcuts(&self, entry: &Entry, platform: Platform) -> Vec<Shortcut> {
        if entry.command_id == ids::EDIT_SELECT_ALL.0
            && entry.shortcuts == [control_all(self.source())]
        {
            vec![control_all(platform)]
        } else {
            entry.shortcuts.clone()
        }
    }
    pub fn default_shortcuts(&self, id: CommandId, platform: Platform) -> Vec<Shortcut> {
        if id == ids::EDIT_SELECT_ALL {
            vec![control_all(self.source())]
        } else {
            Config::defaults(platform).entry(id).shortcuts.clone()
        }
    }
    pub fn replace(
        &self,
        id: CommandId,
        shortcuts: Vec<Shortcut>,
        platform: Platform,
    ) -> Result<Validated, Error> {
        let mut next = self.clone();
        let entry = next
            .bindings
            .iter_mut()
            .find(|e| e.command_id == id.0)
            .ok_or_else(|| Error::new("UnknownCommand", id.0))?;
        entry.shortcuts = shortcuts;
        validate(next, platform)
    }
    pub fn bytes(&self) -> Result<Vec<u8>, Error> {
        let mut config = self.clone();
        config
            .bindings
            .sort_by(|a, b| a.command_id.cmp(&b.command_id));
        let mut bytes = serde_json::to_vec_pretty(&config)
            .map_err(|_| Error::new("InvalidFormat", "无法编码快捷键"))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_BYTES {
            return Err(Error::new("ResourceLimit", "快捷键文件超过64KiB"));
        }
        Ok(bytes)
    }
}

pub(crate) fn decode(bytes: &[u8], platform: Platform) -> Result<Validated, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::new("ResourceLimit", "快捷键文件超过64KiB"));
    }
    // Bound nesting before serde allocates nested objects. Braces inside strings are data.
    let mut depth = 0u8;
    let mut string = false;
    let mut escaped = false;
    for &byte in bytes {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth = depth.saturating_add(1);
                    if depth > 8 {
                        return Err(Error::new("ResourceLimit", "JSON深度超过8"));
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    // serde structs also accept positional arrays; the portable JSON contract requires objects.
    let shape: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| Error::new("InvalidFormat", "JSON内容无效"))?;
    let object = shape
        .as_object()
        .ok_or_else(|| Error::new("InvalidFormat", "配置必须是JSON对象"))?;
    if let Some(entries) = object.get("bindings").and_then(|v| v.as_array()) {
        for entry in entries {
            let entry = entry
                .as_object()
                .ok_or_else(|| Error::new("InvalidFormat", "命令条目必须是对象"))?;
            if let Some(keys) = entry.get("shortcuts").and_then(|v| v.as_array()) {
                for key in keys {
                    let key = key
                        .as_object()
                        .ok_or_else(|| Error::new("InvalidFormat", "快捷键必须是对象"))?;
                    if !key.get("modifiers").is_some_and(|v| v.is_object()) {
                        return Err(Error::new("InvalidFormat", "修饰键必须是对象"));
                    }
                    if key.get("key").is_some_and(|v| v.to_string().len() > 32) {
                        return Err(Error::new("ResourceLimit", "key token超过32字节"));
                    }
                }
            }
        }
    }
    let config = serde_json::from_slice(bytes)
        .map_err(|_| Error::new("InvalidFormat", "JSON字段、类型、重复字段或内容无效"))?;
    validate(config, platform)
}

/// Contexts below input/modal can overlap. Input and modal exclusively own keys.
pub(crate) fn overlap(a: ShortcutContext, b: ShortcutContext) -> bool {
    a == b
        || (!matches!(a, ShortcutContext::TextInput | ShortcutContext::Modal)
            && !matches!(b, ShortcutContext::TextInput | ShortcutContext::Modal))
}
fn conflicts(a: &Entry, b: &Entry, catalogue: &[command::CommandDescriptor]) -> bool {
    let ca = catalogue
        .iter()
        .find(|c| c.id.0 == a.command_id)
        .expect("known command");
    let cb = catalogue
        .iter()
        .find(|c| c.id.0 == b.command_id)
        .expect("known command");
    overlap(ca.context, cb.context) && a.shortcuts.iter().any(|s| b.shortcuts.contains(s))
}

pub(crate) fn validate(mut config: Config, platform: Platform) -> Result<Validated, Error> {
    if config.schema_version != 1 {
        return Err(Error::new(
            "UnsupportedSchema",
            "仅支持schema_version=1；现有配置不变",
        ));
    }
    if config.format != "rcam-shortcuts" || config.modifier_encoding != "logical-v1" {
        return Err(Error::new(
            "InvalidFormat",
            "格式必须是rcam-shortcuts / logical-v1",
        ));
    }
    let catalogue = commands();
    let cross_platform = config.source_platform != SourcePlatform::for_platform(platform);
    if config.bindings.len() > catalogue.len() {
        return Err(Error::new("ResourceLimit", "命令条目超过目录数量"));
    }
    let mut seen = BTreeSet::new();
    for e in &config.bindings {
        if e.command_id.len() > 96 {
            return Err(Error::new("ResourceLimit", "command_id超过96字节"));
        }
        if e.command_id == ids::GRIP_CANCEL.0 {
            return Err(Error::new("FixedCommand", "Esc是固定安全取消键"));
        }
        if !catalogue.iter().any(|c| c.id.0 == e.command_id) {
            return Err(Error::new("UnknownCommand", "文件包含未知命令；整份拒绝"));
        }
        if !seen.insert(e.command_id.clone()) {
            return Err(Error::new(
                "DuplicateEntry",
                format!("重复命令 {}", e.command_id),
            ));
        }
        if e.shortcuts.len() > MAX_KEYS {
            return Err(Error::new(
                "ResourceLimit",
                format!("{}最多4个快捷键", e.command_id),
            ));
        }
        for (i, &s) in e.shortcuts.iter().enumerate() {
            let portable_default = e.command_id == ids::EDIT_SELECT_ALL.0
                && e.shortcuts == [control_all(config.source())];
            check_key(
                s,
                if portable_default {
                    config.source()
                } else {
                    platform
                },
            )?;
            if e.shortcuts[..i].contains(&s) {
                return Err(Error::new(
                    "DuplicateEntry",
                    format!(
                        "{}重复快捷键 {}",
                        e.command_id,
                        format_shortcut(s, platform)
                    ),
                ));
            }
        }
    }
    // Check explicit entries BEFORE completion/compilation: never last-wins.
    for (i, a) in config.bindings.iter().enumerate() {
        for b in &config.bindings[i + 1..] {
            if conflicts(a, b, &catalogue) {
                return Err(Error::new("Conflict", {
                    let ca = catalogue.iter().find(|c| c.id.0 == a.command_id).unwrap();
                    let cb = catalogue.iter().find(|c| c.id.0 == b.command_id).unwrap();
                    let key = a
                        .shortcuts
                        .iter()
                        .find(|key| b.shortcuts.contains(key))
                        .unwrap();
                    format!(
                        "{}（{} / {:?}）与 {}（{} / {:?}）冲突：{}",
                        ca.name,
                        ca.id.0,
                        ca.context,
                        cb.name,
                        cb.id.0,
                        cb.context,
                        format_shortcut(*key, platform)
                    )
                }));
            }
        }
    }
    let defaults = Config::defaults(platform);
    let mut missing = vec![];
    let mut default_conflicts = vec![];
    for mut entry in defaults.bindings {
        if entry.command_id == ids::EDIT_SELECT_ALL.0 {
            entry.shortcuts = vec![control_all(config.source())];
        }
        if seen.contains(&entry.command_id) {
            continue;
        }
        missing.push(entry.command_id.clone());
        if config.bindings.iter().any(|other| {
            conflicts(&entry, other, &catalogue)
                || conflicts(
                    &Entry {
                        command_id: entry.command_id.clone(),
                        shortcuts: config.effective_shortcuts(&entry, platform),
                    },
                    &Entry {
                        command_id: other.command_id.clone(),
                        shortcuts: config.effective_shortcuts(other, platform),
                    },
                    &catalogue,
                )
        }) {
            default_conflicts.push(entry.command_id.clone());
            entry.shortcuts.clear();
        }
        config.bindings.push(entry);
    }
    config
        .bindings
        .sort_by(|a, b| a.command_id.cmp(&b.command_id));
    // Reject both the original explicit conflicts (above) and any collision
    // introduced when compiling a target-platform physical default.
    let effective: Vec<_> = config
        .bindings
        .iter()
        .map(|e| Entry {
            command_id: e.command_id.clone(),
            shortcuts: config.effective_shortcuts(e, platform),
        })
        .collect();
    for (index, entry) in effective.iter().enumerate() {
        for other in &effective[index + 1..] {
            if conflicts(entry, other, &catalogue) {
                return Err(Error::new("Conflict", "跨平台实际快捷键冲突；现有配置不变"));
            }
        }
    }
    let mut bindings = vec![Binding {
        shortcut: Shortcut::new(Modifiers::NONE, Key::Escape),
        context: ShortcutContext::ObjectEdit,
        command: Some(ids::GRIP_CANCEL),
    }];
    for e in &effective {
        let c = catalogue
            .iter()
            .find(|c| c.id.0 == e.command_id)
            .expect("validated");
        for &shortcut in &e.shortcuts {
            bindings.push(Binding {
                shortcut,
                context: c.context,
                command: Some(c.id),
            });
        }
    }
    Ok(Validated {
        config,
        keymap: Keymap::from_bindings(bindings),
        missing,
        default_conflicts,
        cross_platform,
    })
}

pub(crate) fn check_key(s: Shortcut, platform: Platform) -> Result<(), Error> {
    let m = s.modifiers;
    let valid = match s.key {
        Key::Char(c) => c.is_ascii_lowercase() || c.is_ascii_digit(),
        Key::F(n) => (1..=24).contains(&n),
        _ => true,
    };
    if !valid {
        return Err(Error::new(
            "UnsupportedKey",
            "主键仅支持小写ASCII字母/数字、F1–F24与目录特殊键",
        ));
    }
    if platform == Platform::Windows && m.secondary {
        return Err(Error::new(
            "ReservedShortcut",
            "Windows Secondary为系统键，本版本不可绑定",
        ));
    }
    let fixed = s.key == Key::Escape
        || (m == Modifiers::NONE
            && matches!(
                s.key,
                Key::Enter
                    | Key::Tab
                    | Key::Space
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight
            ));
    let os = match platform {
        Platform::MacOs => {
            (m.primary && matches!(s.key, Key::Char('q' | 'h' | 'm') | Key::Space | Key::Tab))
                || (m.secondary
                    && matches!(
                        s.key,
                        Key::Space
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::F(2..=8)
                    ))
                || (m.primary && m.shift && matches!(s.key, Key::Char('3' | '4' | '5')))
                || (m.primary && m.secondary && s.key == Key::Char('f'))
        }
        Platform::Windows => {
            (m.alt && matches!(s.key, Key::F(4) | Key::Tab | Key::Space))
                || (m.primary && m.alt && s.key == Key::Delete)
        }
    };
    if fixed || os {
        return Err(Error::new(
            "ReservedShortcut",
            format!("{} 为系统/输入/取消保留键", format_shortcut(s, platform)),
        ));
    }
    Ok(())
}
pub(crate) fn format_shortcut(s: Shortcut, platform: Platform) -> String {
    let mut parts = vec![];
    if s.modifiers.primary {
        parts.push(
            if platform == Platform::MacOs {
                "Cmd"
            } else {
                "Ctrl"
            }
            .into(),
        );
    }
    if s.modifiers.secondary {
        parts.push(
            if platform == Platform::MacOs {
                "Ctrl"
            } else {
                "Win"
            }
            .into(),
        );
    }
    if s.modifiers.alt {
        parts.push(
            if platform == Platform::MacOs {
                "Option"
            } else {
                "Alt"
            }
            .into(),
        );
    }
    if s.modifiers.shift {
        parts.push("Shift".into());
    }
    parts.push(match s.key {
        Key::Char(c) => c.to_ascii_uppercase().to_string(),
        Key::F(n) => format!("F{n}"),
        key => format!("{key:?}"),
    });
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use command::{Resolution, ShortcutResolver};
    fn resolve(v: &Validated, s: Shortcut) -> Resolution {
        ShortcutResolver::resolve(
            &v.keymap,
            &[ShortcutContext::Canvas, ShortcutContext::Global],
            s,
        )
    }
    fn key(c: char) -> Shortcut {
        Shortcut::new(Modifiers::PRIMARY, Key::Char(c))
    }
    #[test]
    fn compatible_default_snapshot_and_aliases() {
        let v = validate(Config::defaults(Platform::MacOs), Platform::MacOs).unwrap();
        assert_eq!(v.config.bindings.len(), commands().len());
        for id in INACTIVE_DEFAULTS {
            assert!(v.config.entry(id).shortcuts.is_empty());
        }
        for s in [
            Shortcut::new(Modifiers::NONE, Key::Delete),
            Shortcut::new(Modifiers::NONE, Key::Backspace),
        ] {
            assert_eq!(resolve(&v, s), Resolution::Command(ids::EDIT_DELETE));
        }
        for s in [
            key('y'),
            Shortcut::new(Modifiers::PRIMARY_SHIFT, Key::Char('z')),
        ] {
            assert_eq!(resolve(&v, s), Resolution::Command(ids::EDIT_REDO));
        }
        for s in ['v', 'm', 't', 'g'].map(|c| Shortcut::new(Modifiers::NONE, Key::Char(c))) {
            assert_eq!(resolve(&v, s), Resolution::Unbound);
        }
        assert!(v.keymap.conflicts().is_empty());
    }
    #[test]
    fn replacement_clear_restore_and_conflict_preserve_original() {
        let old = Config::defaults(Platform::MacOs);
        let bytes = old.bytes().unwrap();
        let v = old
            .replace(ids::EDIT_DUPLICATE, vec![key('b')], Platform::MacOs)
            .unwrap();
        assert_eq!(resolve(&v, key('d')), Resolution::Unbound);
        assert_eq!(
            resolve(&v, key('b')),
            Resolution::Command(ids::EDIT_DUPLICATE)
        );
        let cleared = v
            .config
            .replace(ids::EDIT_DUPLICATE, vec![], Platform::MacOs)
            .unwrap();
        assert_eq!(resolve(&cleared, key('b')), Resolution::Unbound);
        assert_eq!(
            old.replace(ids::EDIT_DUPLICATE, vec![key('z')], Platform::MacOs)
                .unwrap_err()
                .code,
            "Conflict"
        );
        assert_eq!(old.bytes().unwrap(), bytes);
        let rest = cleared
            .config
            .replace(
                ids::EDIT_DUPLICATE,
                old.entry(ids::EDIT_DUPLICATE).shortcuts.clone(),
                Platform::MacOs,
            )
            .unwrap();
        assert_eq!(
            resolve(&rest, key('d')),
            Resolution::Command(ids::EDIT_DUPLICATE)
        );
    }
    #[test]
    fn missing_defaults_retreat_but_explicit_conflicts_reject() {
        let mut config = Config::defaults(Platform::MacOs);
        config.bindings.clear();
        config.bindings.push(Entry {
            command_id: ids::VIEW_FIT.0.into(),
            shortcuts: vec![key('d')],
        });
        let v = validate(config.clone(), Platform::MacOs).unwrap();
        assert!(
            v.default_conflicts
                .contains(&ids::EDIT_DUPLICATE.0.to_string())
        );
        assert!(v.config.entry(ids::EDIT_DUPLICATE).shortcuts.is_empty());
        config.bindings.push(Entry {
            command_id: ids::EDIT_DUPLICATE.0.into(),
            shortcuts: vec![key('d')],
        });
        assert_eq!(
            validate(config, Platform::MacOs).unwrap_err().code,
            "Conflict"
        );
    }
    #[test]
    fn strict_json_all_layers_and_versions() {
        let base = Config::defaults(Platform::MacOs).bytes().unwrap();
        let text = String::from_utf8(base.clone()).unwrap();
        for version in ["true", "1.0", "\"1\"", "null", "-1", "0", "2"] {
            assert!(
                decode(
                    text.replace(
                        "\"schema_version\": 1",
                        &format!("\"schema_version\": {version}")
                    )
                    .as_bytes(),
                    Platform::MacOs
                )
                .is_err(),
                "{version}"
            );
        }
        for field in [
            "format",
            "schema_version",
            "modifier_encoding",
            "source_platform",
            "bindings",
        ] {
            let mut data: serde_json::Value = serde_json::from_slice(&base).unwrap();
            data.as_object_mut().unwrap().remove(field);
            assert!(decode(&serde_json::to_vec(&data).unwrap(), Platform::MacOs).is_err());
        }
        for needle in ["\"format\":", "\"command_id\":", "\"primary\":", "\"key\":"] {
            let dup = text.replacen(needle, &format!("\"unknown\": 0, {needle}"), 1);
            assert!(decode(dup.as_bytes(), Platform::MacOs).is_err());
        }
        let dup = text.replacen(
            "\"primary\": true",
            "\"primary\": true, \"primary\": false",
            1,
        );
        assert!(decode(dup.as_bytes(), Platform::MacOs).is_err());
        let dup = text.replacen(
            "\"schema_version\": 1",
            "\"schema_version\": 1, \"schema_version\": 1",
            1,
        );
        assert!(decode(dup.as_bytes(), Platform::MacOs).is_err());
        for bytes in [
            b"null".to_vec(),
            vec![255],
            b"{truncated".to_vec(),
            [base.clone(), b"{}".to_vec()].concat(),
            vec![b' '; MAX_BYTES + 1],
            b"[[[[[[[[[0]]]]]]]]]".to_vec(),
        ] {
            assert!(decode(&bytes, Platform::MacOs).is_err());
        }
    }
    #[test]
    fn duplicate_unknown_fixed_and_resource_boundaries() {
        let base = Config::defaults(Platform::MacOs);
        let mut cfg = base.clone();
        cfg.bindings[0].command_id = "unknown".into();
        assert_eq!(
            validate(cfg, Platform::MacOs).unwrap_err().code,
            "UnknownCommand"
        );
        let mut cfg = base.clone();
        cfg.bindings[0].command_id = ids::GRIP_CANCEL.0.into();
        assert_eq!(
            validate(cfg, Platform::MacOs).unwrap_err().code,
            "FixedCommand"
        );
        let mut cfg = base.clone();
        cfg.bindings[0].command_id = cfg.bindings[1].command_id.clone();
        assert_eq!(
            validate(cfg, Platform::MacOs).unwrap_err().code,
            "DuplicateEntry"
        );
        assert_eq!(
            base.replace(ids::VIEW_FIT, vec![key('b'); 2], Platform::MacOs)
                .unwrap_err()
                .code,
            "DuplicateEntry"
        );
        assert_eq!(
            base.replace(ids::VIEW_FIT, vec![key('b'); 5], Platform::MacOs)
                .unwrap_err()
                .code,
            "ResourceLimit"
        );
        let mut cfg = base.clone();
        cfg.bindings.push(cfg.bindings[0].clone());
        assert_eq!(
            validate(cfg, Platform::MacOs).unwrap_err().code,
            "ResourceLimit"
        );
        let mut cfg = base;
        cfg.bindings[0].command_id = "x".repeat(97);
        assert_eq!(
            validate(cfg, Platform::MacOs).unwrap_err().code,
            "ResourceLimit"
        );
    }
    #[test]
    fn platform_reserved_and_key_ranges() {
        for key in [Key::Char('A'), Key::Char('中'), Key::F(0), Key::F(25)] {
            assert_eq!(
                check_key(Shortcut::new(Modifiers::NONE, key), Platform::MacOs)
                    .unwrap_err()
                    .code,
                "UnsupportedKey"
            );
        }
        for key in [Key::Escape, Key::Enter, Key::Tab, Key::Space, Key::ArrowUp] {
            assert!(check_key(Shortcut::new(Modifiers::NONE, key), Platform::MacOs).is_err());
        }
        for key in [
            Key::Char('q'),
            Key::Char('h'),
            Key::Char('m'),
            Key::Space,
            Key::Tab,
        ] {
            assert!(check_key(Shortcut::new(Modifiers::PRIMARY, key), Platform::MacOs).is_err());
        }
        for key in [Key::F(4), Key::Tab, Key::Space] {
            assert!(
                check_key(
                    Shortcut::new(
                        Modifiers {
                            alt: true,
                            ..Modifiers::NONE
                        },
                        key
                    ),
                    Platform::Windows
                )
                .is_err()
            );
        }
        let secondary = Shortcut::new(
            Modifiers {
                secondary: true,
                ..Modifiers::NONE
            },
            Key::Char('b'),
        );
        assert!(check_key(secondary, Platform::MacOs).is_ok());
        assert!(check_key(secondary, Platform::Windows).is_err());
        let cfg = Config::defaults(Platform::MacOs);
        assert!(
            validate(cfg.clone(), Platform::Windows)
                .unwrap()
                .cross_platform
        );
        for platform in [Platform::MacOs, Platform::Windows] {
            let text = format_shortcut(key('s'), platform);
            assert_eq!(
                text,
                if platform == Platform::MacOs {
                    "Cmd+S"
                } else {
                    "Ctrl+S"
                }
            );
        }
    }
    #[test]
    fn canonical_complete_export_and_exclusive_contexts() {
        let cfg = validate(Config::defaults(Platform::MacOs), Platform::MacOs)
            .unwrap()
            .config;
        let bytes = cfg.bytes().unwrap();
        let mut legacy = cfg.clone();
        legacy
            .bindings
            .retain(|e| e.command_id != ids::EDIT_SELECT_ALL.0);
        assert_eq!(
            legacy.bytes().unwrap(),
            include_bytes!("../../../fixtures/synthetic/s5k1/default-shortcuts.json")
        );
        let migrated = decode(&legacy.bytes().unwrap(), Platform::MacOs).unwrap();
        assert_eq!(migrated.missing, [ids::EDIT_SELECT_ALL.0]);
        assert_eq!(
            migrated.config.entry(ids::EDIT_SELECT_ALL),
            cfg.entry(ids::EDIT_SELECT_ALL)
        );
        let read = decode(&bytes, Platform::MacOs).unwrap();
        assert_eq!(read.config.bytes().unwrap(), bytes);
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(
            cfg.bindings
                .iter()
                .map(|e| &e.command_id)
                .collect::<BTreeSet<_>>()
                .len(),
            commands().len()
        );
        assert!(overlap(ShortcutContext::Global, ShortcutContext::Canvas));
        assert!(!overlap(
            ShortcutContext::TextInput,
            ShortcutContext::Canvas
        ));
        let v = validate(cfg, Platform::MacOs).unwrap();
        for x in [ShortcutContext::TextInput, ShortcutContext::Modal] {
            assert_eq!(
                ShortcutResolver::resolve(&v.keymap, &[ShortcutContext::Global, x], key('z')),
                Resolution::Blocked
            );
        }
    }
    #[test]
    fn occupied_default_restore_is_rejected_without_clearing_another_command() {
        let cfg = Config::defaults(Platform::MacOs)
            .replace(ids::EDIT_DUPLICATE, vec![], Platform::MacOs)
            .unwrap()
            .config
            .replace(ids::VIEW_FIT, vec![key('d')], Platform::MacOs)
            .unwrap()
            .config;
        let before = cfg.bytes().unwrap();
        let error = cfg
            .replace(ids::EDIT_DUPLICATE, vec![key('d')], Platform::MacOs)
            .unwrap_err();
        assert_eq!(error.code, "Conflict");
        assert!(error.to_string().contains(ids::VIEW_FIT.0));
        assert_eq!(cfg.bytes().unwrap(), before);
        assert!(cfg.entry(ids::EDIT_DUPLICATE).shortcuts.is_empty());
    }
}
