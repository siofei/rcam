//! Bounded local configuration I/O; rename is the publication commit point.
use crate::shortcut_config::{self, Config, Error, MAX_BYTES, Validated};
use editor_core::{command::Platform, hash::sha256_hex};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Fingerprint {
    Missing,
    Present {
        length: u64,
        modified: Option<std::time::SystemTime>,
        prefix_hash: String,
    },
}
#[derive(Debug)]
pub(crate) struct Startup {
    pub current: Validated,
    pub fingerprint: Option<Fingerprint>,
    pub warning: Option<String>,
    pub protected: bool,
}
#[derive(Debug)]
pub(crate) struct Committed {
    pub current: Validated,
    pub fingerprint: Fingerprint,
    pub durability_warning: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    BeforeTemp,
    #[cfg(any(test, feature = "internal-evidence"))]
    DuringWrite,
    AfterTemp,
    BeforeReplace,
    AfterReplace,
}

#[cfg(feature = "internal-evidence")]
pub(crate) fn native_directory() -> Option<PathBuf> {
    let dir = fs::canonicalize(std::env::var_os("RCAM_SHORTCUT_NATIVE_DIR")?).ok()?;
    (dir.parent() == Some(Path::new("/private/tmp"))
        && dir.file_name()?.to_str()?.starts_with("rcam-k1-native-")
        && dir.is_dir())
    .then_some(dir)
}
pub(crate) fn path() -> Option<PathBuf> {
    crate::preferences::AppPreferences::path()?
        .parent()
        .map(|p| p.join("shortcuts.json"))
}
fn io_error(error: std::io::Error) -> Error {
    Error::new(
        "Io",
        format!(
            "快捷键文件操作失败（{}）；请检查文件权限或重试",
            error.kind()
        ),
    )
}
fn read(path: &Path) -> Result<(Vec<u8>, Fingerprint), Error> {
    match fs::metadata(path) {
        Ok(meta) if !meta.is_file() => {
            return Err(Error::new("InvalidPath", "快捷键路径必须是普通文件"));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((vec![], Fingerprint::Missing));
        }
        Err(e) => return Err(io_error(e)),
    }
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((vec![], Fingerprint::Missing));
        }
        Err(e) => return Err(io_error(e)),
    };
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(Error::new("InvalidPath", "快捷键路径必须是普通文件"));
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    let fingerprint = Fingerprint::Present {
        length: metadata.len(),
        modified: metadata.modified().ok(),
        prefix_hash: sha256_hex(&bytes),
    };
    Ok((bytes, fingerprint))
}
pub(crate) fn import(path: &Path, platform: Platform) -> Result<Validated, Error> {
    let (bytes, fingerprint) = read(path)?;
    if fingerprint == Fingerprint::Missing {
        return Err(Error::new("Io", "导入文件不存在"));
    }
    shortcut_config::decode(&bytes, platform)
}
pub(crate) fn load(path: Option<&Path>, platform: Platform) -> Startup {
    let defaults = || {
        shortcut_config::validate(Config::defaults(platform), platform)
            .expect("validated shipped defaults")
    };
    let Some(path) = path else {
        return Startup {
            current: defaults(),
            fingerprint: None,
            warning: Some("用户配置目录不可用；快捷键使用默认值，暂不能保存".into()),
            protected: true,
        };
    };
    match read(path) {
        Ok((_, Fingerprint::Missing)) => Startup {
            current: defaults(),
            fingerprint: Some(Fingerprint::Missing),
            warning: None,
            protected: false,
        },
        Ok((bytes, fingerprint)) => match shortcut_config::decode(&bytes, platform) {
            Ok(current) => Startup {
                current,
                fingerprint: Some(fingerprint),
                warning: None,
                protected: false,
            },
            Err(error) => Startup {
                current: defaults(),
                fingerprint: Some(fingerprint),
                warning: Some(format!(
                    "{error}。使用默认值；原文件受保护，需明确确认重新建立配置。"
                )),
                protected: true,
            },
        },
        Err(error) => Startup {
            current: defaults(),
            fingerprint: None,
            warning: Some(format!("{error}。使用默认值；原文件受保护。")),
            protected: true,
        },
    }
}
fn lock(path: &Path) -> Result<File, Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("InvalidPath", "没有配置目录"))?;
    fs::create_dir_all(parent).map_err(io_error)?;
    if let Ok(meta) = fs::metadata(path.with_extension("json.lock"))
        && !meta.is_file()
    {
        return Err(Error::new("InvalidPath", "配置锁路径不是普通文件"));
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("json.lock"))
        .map_err(io_error)?;
    file.try_lock().map_err(|_| {
        Error::new(
            "ConfigurationBusy",
            "另一个RCam实例正在保存此快捷键文件；请稍后重试",
        )
    })?;
    Ok(file) // OS advisory lock is released by closing, including process exit.
}
fn phase(phase: Phase, failure: Option<Phase>) -> Result<(), Error> {
    #[cfg(feature = "internal-evidence")]
    {
        let name = match phase {
            Phase::BeforeTemp => "before-temp",
            Phase::DuringWrite => "during-write",
            Phase::AfterTemp => "after-temp",
            Phase::BeforeReplace => "before-replace",
            Phase::AfterReplace => "after-replace",
        };
        if std::env::var("RCAM_SHORTCUT_EXIT_PHASE").ok().as_deref() == Some(name) {
            if let Some(dir) = native_directory() {
                let marker = serde_json::json!({"schema_version":2,"phase":name,"disk_snapshot_sha256":path().and_then(|path| read(&path).ok()).map(|(bytes,_)| sha256_hex(&bytes))});
                let _ = fs::write(dir.join("exit-observation.json"), marker.to_string());
            }
            std::process::exit(86);
        }
    }
    if failure == Some(phase) {
        Err(Error::new("InjectedIo", "测试注入I/O故障"))
    } else {
        Ok(())
    }
}
fn replace(
    path: &Path,
    bytes: &[u8],
    failure: Option<Phase>,
) -> Result<(Option<String>, Fingerprint), Error> {
    phase(Phase::BeforeTemp, failure)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("InvalidPath", "没有配置目录"))?;
    let temp = parent.join(format!(
        ".rcam-shortcuts-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    publish_temp(path, bytes, failure, &temp)
}
fn publish_temp(
    path: &Path,
    bytes: &[u8],
    failure: Option<Phase>,
    temp: &Path,
) -> Result<(Option<String>, Fingerprint), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("InvalidPath", "没有配置目录"))?;
    let mut created = false;
    let mut before_commit = || -> Result<Fingerprint, Error> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(temp)
            .map_err(io_error)?;
        created = true;
        #[cfg(any(test, feature = "internal-evidence"))]
        {
            let middle = bytes.len() / 2;
            file.write_all(&bytes[..middle]).map_err(io_error)?;
            phase(Phase::DuringWrite, failure)?;
            file.write_all(&bytes[middle..]).map_err(io_error)?;
        }
        #[cfg(not(any(test, feature = "internal-evidence")))]
        file.write_all(bytes).map_err(io_error)?;
        file.flush().map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        phase(Phase::AfterTemp, failure)?;
        phase(Phase::BeforeReplace, failure)?;
        let metadata = file.metadata().map_err(io_error)?;
        let fingerprint = Fingerprint::Present {
            length: bytes.len() as u64,
            modified: metadata.modified().ok(),
            prefix_hash: sha256_hex(bytes),
        };
        fs::rename(temp, path).map_err(io_error)?;
        Ok(fingerprint)
    };
    let fingerprint = match before_commit() {
        Ok(fp) => fp,
        Err(error) => {
            if created {
                let _ = fs::remove_file(temp);
            }
            return Err(error);
        }
    };
    // The new file is now committed. Never report a rollback after this point.
    let warning = phase(Phase::AfterReplace, failure)
        .err()
        .map(|e| e.to_string());
    #[cfg(target_os = "macos")]
    let warning = warning.or_else(|| {
        File::open(parent)
            .and_then(|f| f.sync_all())
            .err()
            .map(|e| format!("配置已提交；目录同步失败（{}），耐久性未确认", e.kind()))
    });
    Ok((warning, fingerprint))
}
pub(crate) fn save(
    path: &Path,
    candidate: Config,
    expected: &Fingerprint,
    platform: Platform,
) -> Result<Committed, Error> {
    save_inner(path, candidate, expected, platform, None)
}
fn save_inner(
    path: &Path,
    candidate: Config,
    expected: &Fingerprint,
    platform: Platform,
    failure: Option<Phase>,
) -> Result<Committed, Error> {
    // Prepare both runtime map and exact bytes before taking the publication lock.
    let current = shortcut_config::validate(candidate, platform)?;
    let bytes = current.config.bytes()?;
    let _lock = lock(path)?;
    let (_, actual) = read(path)?;
    if fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err(Error::new("ReadOnly", "快捷键配置文件只读；原配置保持不变"));
    }
    if &actual != expected {
        return Err(Error::new(
            "ExternalChange",
            "磁盘快捷键已被其他实例或编辑器修改；本次未保存，请关闭并重启RCam后重新编辑",
        ));
    }
    let (mut durability_warning, fingerprint) = replace(path, &bytes, failure)?;
    match read(path) {
        Ok((actual, fp)) if actual == bytes && fp == fingerprint => {},
        _ => durability_warning = Some("配置已提交；提交后磁盘观察异常或遭外部改写。运行时已生效，请重启核对；不会将外部内容认领为本次保存基线".into()),
    }
    Ok(Committed {
        current,
        fingerprint,
        durability_warning,
    })
}
fn same_target(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(am), Ok(bm)) = (fs::metadata(a), fs::metadata(b))
            && am.dev() == bm.dev()
            && am.ino() == bm.ino()
        {
            return true;
        }
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            match (
                a.parent().and_then(|p| fs::canonicalize(p).ok()),
                b.parent().and_then(|p| fs::canonicalize(p).ok()),
            ) {
                (Some(ap), Some(bp)) => ap == bp && a.file_name() == b.file_name(),
                _ => false,
            }
        }
    }
}
pub(crate) fn export(
    path: &Path,
    config: Config,
    config_path: Option<&Path>,
    platform: Platform,
) -> Result<Option<String>, Error> {
    if config_path.is_some_and(|current| same_target(path, current)) {
        return Err(Error::new(
            "ConfigurationTarget",
            "导出目标不能是当前自动保存的shortcuts.json；请另选文件",
        ));
    }
    let bytes = shortcut_config::validate(config, platform)?
        .config
        .bytes()?;
    let _lock = lock(path)?;
    replace(path, &bytes, None).map(|(warning, _)| warning)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn folder() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "rcam-k1-store-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn save_restart_external_change_and_invalid_import() {
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let platform = Platform::MacOs;
        let first = load(Some(&path), platform);
        assert!(!first.protected);
        let committed = save(&path, first.current.config, &Fingerprint::Missing, platform).unwrap();
        assert_eq!(
            load(Some(&path), platform).current.config,
            committed.current.config
        );
        let original = fs::read(&path).unwrap();
        fs::write(dir.join("bad.json"), b"{}").unwrap();
        assert!(import(&dir.join("bad.json"), platform).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::write(&path, b"broken").unwrap();
        assert_eq!(
            save(
                &path,
                committed.current.config,
                &committed.fingerprint,
                platform
            )
            .unwrap_err()
            .code,
            "ExternalChange"
        );
        let protected = load(Some(&path), platform);
        assert!(protected.protected);
        assert!(protected.warning.is_some());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn precommit_failures_preserve_old_postcommit_warns_and_os_lock_releases() {
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let platform = Platform::MacOs;
        let cfg = Config::defaults(platform);
        let c = save(&path, cfg.clone(), &Fingerprint::Missing, platform).unwrap();
        let old = fs::read(&path).unwrap();
        for p in [
            Phase::BeforeTemp,
            Phase::DuringWrite,
            Phase::AfterTemp,
            Phase::BeforeReplace,
        ] {
            assert!(save_inner(&path, cfg.clone(), &c.fingerprint, platform, Some(p)).is_err());
            assert_eq!(fs::read(&path).unwrap(), old);
        }
        let result = save_inner(
            &path,
            cfg.clone(),
            &c.fingerprint,
            platform,
            Some(Phase::AfterReplace),
        )
        .unwrap();
        assert!(result.durability_warning.is_some());
        assert_eq!(
            load(Some(&path), platform).current.config,
            result.current.config
        );
        let guard = lock(&path).unwrap();
        assert_eq!(
            save(&path, cfg.clone(), &result.fingerprint, platform)
                .unwrap_err()
                .code,
            "ConfigurationBusy"
        );
        drop(guard);
        save(&path, cfg, &result.fingerprint, platform).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn foreign_preview_commit_runtime_and_restart_have_identical_snapshot_hash() {
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let foreign = Config::defaults(Platform::Windows);
        let preview = shortcut_config::decode(&foreign.bytes().unwrap(), Platform::MacOs).unwrap();
        assert!(preview.cross_platform);
        let expected = sha256_hex(&preview.config.bytes().unwrap());
        let committed = save(
            &path,
            preview.config,
            &Fingerprint::Missing,
            Platform::MacOs,
        )
        .unwrap();
        assert_eq!(sha256_hex(&fs::read(&path).unwrap()), expected);
        assert_eq!(
            sha256_hex(&committed.current.config.bytes().unwrap()),
            expected
        );
        assert_eq!(
            sha256_hex(
                &load(Some(&path), Platform::MacOs)
                    .current
                    .config
                    .bytes()
                    .unwrap()
            ),
            expected
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn existing_temp_is_never_deleted_and_nonregular_input_rejected() {
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let temp = dir.join("collision.tmp");
        fs::write(&temp, b"belongs to someone else").unwrap();
        assert!(publish_temp(&path, b"new", None, &temp).is_err());
        assert_eq!(fs::read(&temp).unwrap(), b"belongs to someone else");
        assert_eq!(
            import(&dir, Platform::MacOs).unwrap_err().code,
            "InvalidPath"
        );
        #[cfg(target_os = "macos")]
        {
            let fifo = dir.join("fifo.json");
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(&fifo)
                    .status()
                    .unwrap()
                    .success()
            );
            assert_eq!(
                import(&fifo, Platform::MacOs).unwrap_err().code,
                "InvalidPath"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bounded_read_export_target_and_complete_roundtrip() {
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let export_path = dir.join("portable.json");
        let cfg = Config::defaults(Platform::MacOs);
        fs::write(&path, vec![b'x'; MAX_BYTES + 1]).unwrap();
        assert!(load(Some(&path), Platform::MacOs).protected);
        assert!(export(&path, cfg.clone(), Some(&path), Platform::MacOs).is_err());
        export(&export_path, cfg.clone(), Some(&path), Platform::MacOs).unwrap();
        assert_eq!(
            import(&export_path, Platform::MacOs)
                .unwrap()
                .config
                .bytes()
                .unwrap(),
            cfg.bytes().unwrap()
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cleared_aliases_unicode_export_restart_and_restore_keep_other_preferences() {
        use editor_core::command::{Key, Modifiers, Shortcut, ids};
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let prefs = dir.join("preferences.json");
        fs::write(&prefs, br#"{"units":"inch","theme":"dark"}"#).unwrap();
        let original_prefs = fs::read(&prefs).unwrap();
        let default = Config::defaults(Platform::MacOs);
        let cfg = default
            .replace(ids::VIEW_FIT, vec![], Platform::MacOs)
            .unwrap()
            .config
            .replace(
                ids::EDIT_DUPLICATE,
                vec![
                    Shortcut::new(Modifiers::PRIMARY, Key::Char('b')),
                    Shortcut::new(Modifiers::PRIMARY, Key::Char('k')),
                ],
                Platform::MacOs,
            )
            .unwrap()
            .config;
        let committed = save(&path, cfg, &Fingerprint::Missing, Platform::MacOs).unwrap();
        let reopened = load(Some(&path), Platform::MacOs);
        assert!(
            reopened
                .current
                .config
                .entry(ids::VIEW_FIT)
                .shortcuts
                .is_empty()
        );
        assert_eq!(
            reopened
                .current
                .config
                .entry(ids::EDIT_DUPLICATE)
                .shortcuts
                .len(),
            2
        );
        let target = dir.join("非ASCII-快捷键.json");
        export(
            &target,
            reopened.current.config.clone(),
            Some(&path),
            Platform::MacOs,
        )
        .unwrap();
        let bytes = fs::read(&target).unwrap();
        export(
            &target,
            reopened.current.config,
            Some(&path),
            Platform::MacOs,
        )
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), bytes);
        assert_eq!(
            import(&target, Platform::MacOs).unwrap().config,
            committed.current.config
        );
        save(
            &path,
            default.clone(),
            &committed.fingerprint,
            Platform::MacOs,
        )
        .unwrap();
        assert_eq!(
            load(Some(&path), Platform::MacOs)
                .current
                .config
                .bytes()
                .unwrap(),
            default.bytes().unwrap()
        );
        assert_eq!(fs::read(&prefs).unwrap(), original_prefs);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn readonly_invalid_path_and_precommit_export_leave_original_files() {
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let cfg = Config::defaults(Platform::MacOs);
        let committed = save(&path, cfg.clone(), &Fingerprint::Missing, Platform::MacOs).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        assert_eq!(
            save(&path, cfg.clone(), &committed.fingerprint, Platform::MacOs)
                .unwrap_err()
                .code,
            "ReadOnly"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let target = dir.join("export.json");
        fs::write(&target, b"previous export").unwrap();
        for phase in [Phase::BeforeTemp, Phase::DuringWrite, Phase::BeforeReplace] {
            assert!(replace(&target, &cfg.bytes().unwrap(), Some(phase)).is_err());
            assert_eq!(fs::read(&target).unwrap(), b"previous export");
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let (warning, _) =
            replace(&target, &cfg.bytes().unwrap(), Some(Phase::AfterReplace)).unwrap();
        assert!(warning.is_some());
        assert_eq!(
            import(&target, Platform::MacOs)
                .unwrap()
                .config
                .bytes()
                .unwrap(),
            cfg.bytes().unwrap()
        );
        fs::write(dir.join("not-directory"), b"x").unwrap();
        assert!(
            save(
                &dir.join("not-directory/shortcuts.json"),
                Config::defaults(Platform::MacOs),
                &Fingerprint::Missing,
                Platform::MacOs
            )
            .is_err()
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cooperating_process_lock_helper() {
        let Some(dir) = std::env::var_os("RCAM_K1_UNIT_LOCK_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let _guard = lock(&dir.join("shortcuts.json")).unwrap();
        fs::write(dir.join("ready"), b"locked").unwrap();
        for _ in 0..2000 {
            if dir.join("release").exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("bounded child lock observation timed out");
    }
    #[test]
    fn cooperating_second_process_rejects_racing_save_and_releases_os_lock() {
        use std::process::{Command, Stdio};
        let dir = folder();
        let path = dir.join("shortcuts.json");
        let first = save(
            &path,
            Config::defaults(Platform::MacOs),
            &Fingerprint::Missing,
            Platform::MacOs,
        )
        .unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "shortcut_store::tests::cooperating_process_lock_helper",
            ])
            .env("RCAM_K1_UNIT_LOCK_DIR", &dir)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let ready = (0..1000).any(|_| {
            if dir.join("ready").exists() {
                true
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
                false
            }
        });
        let outcome = if ready {
            save(
                &path,
                Config::defaults(Platform::MacOs),
                &first.fingerprint,
                Platform::MacOs,
            )
            .err()
            .map(|e| e.code)
        } else {
            None
        };
        fs::write(dir.join("release"), b"release").unwrap();
        assert!(child.wait().unwrap().success());
        assert!(ready);
        assert_eq!(outcome, Some("ConfigurationBusy"));
        save(
            &path,
            Config::defaults(Platform::MacOs),
            &first.fingerprint,
            Platform::MacOs,
        )
        .unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
