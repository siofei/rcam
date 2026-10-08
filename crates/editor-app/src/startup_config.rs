//! Opt-in app-state isolation. No project paths or manufacturing settings are changed.
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

const MARKER: &str = ".rcam-config-owner";
const MARKER_BYTES: &[u8] = b"RCam explicit config directory v1\n";
const LOCK: &str = ".rcam-config.lock";
const STATE_OVERRIDES: &[&str] = &[
    "RCAM_SHORTCUT_NATIVE_DIR",
    "RCAM_S5M1_NATIVE_DIR",
    "RCAM_A2_NATIVE_DIR",
    "RCAM_BATCH_DRAG_NATIVE_DIR",
    "RCAM_I2_C_NATIVE_DIR",
    "RCAM_I2_B_NATIVE_DIR",
    "RCAM_I1_NATIVE_DIR",
    "RCAM_I2_C_NATIVE_ROOT",
    "RCAM_I2_B_NATIVE_ROOT",
    "RCAM_PMIX_NATIVE_DIR",
    "RCAM_S4D2_NATIVE_DIR",
    "RCAM_S4D1_NATIVE_DIR",
];

pub(crate) struct Paths {
    pub preferences: PathBuf,
    pub recovery: PathBuf,
    pub logs: PathBuf,
}

struct Exclusive {
    paths: Paths,
    // Static storage deliberately retains the lock through detached worker shutdown.
    // The operating system releases it on process exit, including abnormal exit.
    _lock: File,
}
static EXPLICIT: OnceLock<Exclusive> = OnceLock::new();

pub(crate) fn paths() -> Option<&'static Paths> {
    EXPLICIT.get().map(|config| &config.paths)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn parse(args: impl IntoIterator<Item = OsString>) -> io::Result<Option<PathBuf>> {
    let args: Vec<_> = args.into_iter().collect();
    // Preserve the previous no-option startup behavior, including unrelated arguments.
    if !args
        .iter()
        .any(|arg| arg == "--config-dir" || arg.as_encoded_bytes().starts_with(b"--config-dir="))
    {
        return Ok(None);
    }
    if args.len() != 2 || args[0] != "--config-dir" {
        return Err(invalid(
            "use exactly --config-dir <absolute exclusive directory>",
        ));
    }
    let path = PathBuf::from(&args[1]);
    if !path.is_absolute()
        || path.file_name().is_none()
        || path.components().any(|c| c == Component::ParentDir)
    {
        return Err(invalid(
            "config directory must be an absolute non-root path without '..'",
        ));
    }
    Ok(Some(path))
}

pub(crate) fn initialize() -> io::Result<()> {
    let protected = default_directories(std::env::var_os("HOME"), std::env::var_os("APPDATA"));
    let Some(config) = configure(
        std::env::args_os().skip(1),
        STATE_OVERRIDES
            .iter()
            .any(|key| std::env::var_os(key).is_some()),
        &protected,
    )?
    else {
        return Ok(());
    };
    EXPLICIT
        .set(config)
        .map_err(|_| invalid("config directory already initialized"))
}

fn configure(
    args: impl IntoIterator<Item = OsString>,
    state_override: bool,
    protected: &[PathBuf],
) -> io::Result<Option<Exclusive>> {
    let Some(root) = parse(args)? else {
        return Ok(None);
    };
    if state_override {
        return Err(invalid(
            "--config-dir conflicts with native state directory overrides",
        ));
    }
    acquire(&root, protected).map(Some)
}

#[cfg(test)]
pub(crate) fn install_for_test(root: &Path) {
    let config = configure(
        [OsString::from("--config-dir"), root.as_os_str().to_owned()],
        false,
        &[],
    )
    .unwrap()
    .unwrap();
    assert!(EXPLICIT.set(config).is_ok());
}

fn default_directories(home: Option<OsString>, appdata: Option<OsString>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home {
        let home = PathBuf::from(home);
        paths.extend([
            home.join("Library/Application Support/RCam"),
            home.join("Library/Caches/RCam/recovery"),
            home.join("Library/Logs/RCam"),
        ]);
    }
    if let Some(appdata) = appdata {
        paths.push(PathBuf::from(appdata).join("RCam"));
    }
    paths
}

// Resolve existing ancestors too: macOS /var and /tmp aliases must not bypass overlap checks.
fn resolved(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| invalid("no config parent"))?;
            let name = path.file_name().ok_or_else(|| invalid("no config name"))?;
            Ok(resolved(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}

// Missing suffixes cannot be canonicalized. Conservatively fold ASCII case so an
// uncreated RCam directory on case-insensitive macOS/Windows volumes is protected too.
fn component_prefix(path: &Path, base: &Path) -> bool {
    let mut components = path.components();
    base.components().all(|base| {
        components.next().is_some_and(|component| {
            component
                .as_os_str()
                .as_encoded_bytes()
                .eq_ignore_ascii_case(base.as_os_str().as_encoded_bytes())
        })
    })
}

fn inspect(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(invalid("config directory cannot contain symbolic links"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("config directory cannot contain reparse points"));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.is_file() && metadata.nlink() != 1 {
            return Err(invalid("config directory cannot contain hard-linked files"));
        }
    }
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(invalid(
            "config entries must be regular files or directories",
        ));
    }
    Ok(metadata)
}

fn inspect_optional(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match inspect(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn writable(path: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    if metadata.permissions().readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "config entry is read-only",
        ));
    }
    if metadata.is_file() {
        OpenOptions::new().write(true).open(path)?;
    }
    Ok(())
}

fn inspect_tree(path: &Path, depth: usize, remaining: &mut usize) -> io::Result<()> {
    let metadata = inspect(path)?;
    writable(path, &metadata)?;
    if metadata.is_dir() {
        if depth == 0 {
            return Err(invalid("config directory nesting exceeds limit"));
        }
        for entry in fs::read_dir(path)? {
            *remaining = remaining
                .checked_sub(1)
                .ok_or_else(|| invalid("too many config entries"))?;
            inspect_tree(&entry?.path(), depth - 1, remaining)?;
        }
    }
    Ok(())
}

fn make_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if inspect(path)?.is_dir() {
                Ok(())
            } else {
                Err(invalid("config path must be a directory"))
            }
        }
        Err(error) => Err(error),
    }
}

fn probe_directory(path: &Path) -> io::Result<()> {
    let probe = path.join(format!(".rcam-write-probe-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    let result = file.write_all(b"RCam").and_then(|()| file.sync_all());
    drop(file);
    let cleanup = fs::remove_file(probe);
    result.and(cleanup)
}

fn inspect_root(root: &Path) -> io::Result<bool> {
    let marker_path = root.join(MARKER);
    let owned = if let Some(metadata) = inspect_optional(&marker_path)? {
        if !metadata.is_file() {
            return Err(invalid("invalid config ownership marker"));
        }
        let mut bytes = Vec::new();
        File::open(&marker_path)?
            .take(128)
            .read_to_end(&mut bytes)?;
        if bytes != MARKER_BYTES {
            return Err(invalid("config ownership marker is not recognized"));
        }
        true
    } else {
        false
    };
    let mut remaining: usize = 4096;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let allowed = name == LOCK
            || name == MARKER
            || name == "preferences.json"
            || name == "shortcuts.json"
            || (owned
                && (name == "recovery"
                    || name == "logs"
                    || name == "preferences.json.tmp"
                    || name == "shortcuts.json.lock"
                    || name
                        .to_str()
                        .is_some_and(|s| s.starts_with(".rcam-shortcuts-"))));
        if !allowed {
            return Err(invalid(
                "existing config directory contains unrelated state; use a fresh directory",
            ));
        }
        let path = entry.path();
        let metadata = inspect(&path)?;
        if metadata.is_dir() != (name == "recovery" || name == "logs") {
            return Err(invalid("config entry has the wrong type"));
        }
        remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| invalid("too many config entries"))?;
        inspect_tree(&path, 4, &mut remaining)?;
    }
    Ok(owned)
}

fn acquire(root: &Path, protected: &[PathBuf]) -> io::Result<Exclusive> {
    // All argument, overlap and existing-root checks precede the first write.
    if !root.is_absolute()
        || root.file_name().is_none()
        || root.components().any(|c| c == Component::ParentDir)
    {
        return Err(invalid("invalid config directory"));
    }
    #[cfg(windows)]
    if root.components().any(|component| {
        matches!(component, Component::Normal(name)
        if name.to_string_lossy().ends_with(['.', ' ']))
    }) {
        return Err(invalid(
            "config path components cannot end with a dot or space",
        ));
    }
    // Strip trailing separators and '.' before lstat, which otherwise follows a leaf link.
    let root: PathBuf = root.components().collect();
    if let Some(metadata) = inspect_optional(&root)? {
        if !metadata.is_dir() {
            return Err(invalid("config path must be a directory"));
        }
        writable(&root, &metadata)?;
    }
    let root = resolved(&root)?;
    for path in protected.iter().filter(|p| p.is_absolute()) {
        let path = resolved(path)?;
        if component_prefix(&root, &path) || component_prefix(&path, &root) {
            return Err(invalid("config directory overlaps default RCam state"));
        }
    }
    // Ancestor aliases are allowed; creation is non-recursive and requires an existing parent.
    make_directory(&root)?;
    inspect_root(&root)?;
    let lock_path = root.join(LOCK);
    if inspect_optional(&lock_path)?.is_some_and(|metadata| !metadata.is_file()) {
        return Err(invalid("config lock must be a regular file"));
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    lock.try_lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "config directory is already in use",
        )
    })?;
    // Recheck after locking; another cooperating startup may have initialized the empty root.
    let owned = inspect_root(&root)?;
    let marker_path = root.join(MARKER);
    let recovery = root.join("recovery");
    let logs = root.join("logs");
    // Mark ownership before creating managed subdirectories so a failed preflight can be retried.
    if !owned {
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker_path)?;
        marker.write_all(MARKER_BYTES)?;
        marker.sync_all()?;
    }
    make_directory(&recovery)?;
    make_directory(&logs)?;
    make_directory(&logs.join("crashes"))?;
    for directory in [&root, &recovery, &logs, &logs.join("crashes")] {
        probe_directory(directory)?;
    }
    Ok(Exclusive {
        paths: Paths {
            preferences: root.join("preferences.json"),
            recovery,
            logs,
        },
        _lock: lock,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Temporary(PathBuf);
    impl Temporary {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "rcam-config-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn default_and_invalid_arguments_do_not_create_state() {
        let temp = Temporary::new();
        let root = temp.0.join("unused");
        assert!(
            configure([], true, std::slice::from_ref(&root))
                .unwrap()
                .is_none()
        );
        assert!(parse(args(&["previously-ignored"])).unwrap().is_none());
        for values in [
            vec!["--config-dir"],
            vec!["--config-dir", "relative"],
            vec!["--config-dir", ""],
            vec!["--config-dir", "/"],
            vec!["--config-dir", "/tmp/../escape"],
            vec![
                "--config-dir",
                root.to_str().unwrap(),
                "--config-dir",
                root.to_str().unwrap(),
            ],
            vec!["--config-dir", root.to_str().unwrap(), "--unknown"],
            vec!["--config-dir=/tmp/unsupported"],
        ] {
            assert!(configure(args(&values), false, &[]).is_err(), "{values:?}");
        }
        assert!(configure(args(&["--config-dir", root.to_str().unwrap()]), true, &[]).is_err());
        assert!(!root.exists());
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_explicit_arguments_never_fall_back() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(b"--config-dir=/tmp/nonutf-\xff".to_vec());
        assert!(parse([bad]).is_err());
        let path = OsString::from_vec(b"/tmp/nonutf-\xff".to_vec());
        assert_eq!(
            parse([OsString::from("--config-dir"), path.clone()]).unwrap(),
            Some(PathBuf::from(path))
        );
    }

    #[test]
    fn protects_default_directories_and_their_ancestors_and_descendants() {
        let temp = Temporary::new();
        let home = temp.0.join("home");
        let appdata = temp.0.join("appdata");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&appdata).unwrap();
        let protected = default_directories(
            Some(home.clone().into_os_string()),
            Some(appdata.into_os_string()),
        );
        assert_eq!(protected.len(), 4);
        for path in &protected {
            assert!(acquire(path, &protected).is_err());
            assert!(acquire(&path.join("nested"), &protected).is_err());
            let variant = PathBuf::from(
                path.to_str()
                    .unwrap()
                    .replace("RCam", "rcam")
                    .replace("Library", "library"),
            );
            assert!(
                acquire(&variant, &protected)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("overlaps")
            );
            assert!(
                acquire(&variant.join("nested"), &protected)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("overlaps")
            );
            assert!(
                acquire(variant.parent().unwrap(), &protected)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("overlaps")
            );
        }
        assert!(acquire(&home, &protected).is_err());
        assert_eq!(fs::read_dir(home).unwrap().count(), 0);
        assert!(!temp.0.join("appdata/RCam").exists());
    }

    #[test]
    fn complete_paths_exclusive_hold_release_and_owned_restart() {
        let temp = Temporary::new();
        let root = temp.0.join("独立 config");
        let first = acquire(&root, &[]).unwrap();
        assert_eq!(first.paths.preferences, root.join("preferences.json"));
        assert_eq!(first.paths.recovery, root.join("recovery"));
        assert_eq!(first.paths.logs, root.join("logs"));
        assert_eq!(
            acquire(&root, &[]).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        fs::write(root.join("preferences.json"), b"{}").unwrap();
        fs::write(root.join("shortcuts.json"), b"{}").unwrap();
        fs::write(root.join("recovery/synthetic.tmp"), b"recovery").unwrap();
        fs::write(root.join("logs/rcam.log.1"), b"rotated log").unwrap();
        fs::write(root.join("logs/crashes/synthetic.json"), b"{}").unwrap();
        drop(first);
        let second = acquire(&root, &[]).unwrap();
        assert_eq!(
            fs::read(root.join("recovery/synthetic.tmp")).unwrap(),
            b"recovery"
        );
        drop(second);
        assert!(
            !root
                .join(format!(".rcam-write-probe-{}", std::process::id()))
                .exists()
        );
    }

    #[test]
    fn existing_synthetic_seed_is_preserved_but_unrelated_state_is_rejected() {
        let temp = Temporary::new();
        let seed = temp.0.join("seed");
        fs::create_dir(&seed).unwrap();
        fs::write(seed.join("preferences.json"), b"{\"panel_width\":300}").unwrap();
        let config = acquire(&seed, &[]).unwrap();
        assert_eq!(
            fs::read(&config.paths.preferences).unwrap(),
            b"{\"panel_width\":300}"
        );
        drop(config);
        let unrelated = temp.0.join("unrelated");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("keep.txt"), b"unchanged").unwrap();
        assert!(acquire(&unrelated, &[]).is_err());
        assert_eq!(fs::read(unrelated.join("keep.txt")).unwrap(), b"unchanged");
        assert!(!unrelated.join(MARKER).exists());
        assert!(!unrelated.join(LOCK).exists());
        assert_eq!(fs::read_dir(&unrelated).unwrap().count(), 1);
        assert!(!unrelated.join("logs").exists());
        // Rejection releases its lock too.
        fs::remove_file(unrelated.join("keep.txt")).unwrap();
        drop(acquire(&unrelated, &[]).unwrap());
    }

    #[test]
    fn wrong_types_readonly_missing_parent_and_bad_marker_fail_closed() {
        let temp = Temporary::new();
        let root = temp.0.join("config");
        assert!(acquire(&root.join("missing-parent"), &[]).is_err());
        assert!(!root.exists());
        fs::write(&root, b"file").unwrap();
        assert!(acquire(&root, &[]).is_err());
        fs::remove_file(&root).unwrap();
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("preferences.json")).unwrap();
        assert!(acquire(&root, &[]).is_err());
        fs::remove_dir(root.join("preferences.json")).unwrap();
        fs::write(root.join(MARKER), b"unrecognized").unwrap();
        assert!(acquire(&root, &[]).is_err());
        fs::remove_file(root.join(MARKER)).unwrap();
        fs::write(root.join("preferences.json"), b"{}").unwrap();
        let path = root.join("preferences.json");
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions.clone()).unwrap();
        assert_eq!(
            acquire(&root, &[]).err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions = fs::Permissions::from_mode(0o600);
        }
        #[cfg(windows)]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
        assert!(!root.join("logs").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_and_hard_links_cannot_redirect_writes() {
        use std::os::unix::fs::symlink;
        let temp = Temporary::new();
        let outside = temp.0.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), b"unchanged").unwrap();
        let alias = temp.0.join("alias");
        symlink(&outside, &alias).unwrap();
        assert!(acquire(&alias, &[]).is_err());
        assert!(acquire(&PathBuf::from(format!("{}/", alias.display())), &[]).is_err());
        assert!(acquire(&alias.join("."), &[]).is_err());
        let root = temp.0.join("config");
        drop(acquire(&root, &[]).unwrap());
        for name in [
            "preferences.json",
            "shortcuts.json",
            LOCK,
            "logs/rcam.log",
            "recovery/item.rcam",
        ] {
            let path = root.join(name);
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            symlink(outside.join("keep"), &path).unwrap();
            assert!(acquire(&root, &[]).is_err(), "{name}");
            fs::remove_file(&path).unwrap();
        }
        let dangling = root.join("preferences.json");
        symlink(outside.join("missing"), &dangling).unwrap();
        assert!(acquire(&root, &[]).is_err());
        fs::remove_file(&dangling).unwrap();
        fs::hard_link(outside.join("keep"), root.join("preferences.json")).unwrap();
        assert!(acquire(&root, &[]).is_err());
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"unchanged");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    }

    #[test]
    #[ignore = "child process helper"]
    fn process_lifetime_lock_helper() {
        let root = PathBuf::from(std::env::var_os("RCAM_CONFIG_TEST_DIR").unwrap());
        install_for_test(&root);
        assert_eq!(paths().unwrap().preferences, root.join("preferences.json"));
        println!("RCAM_CONFIG_READY");
        io::stdout().flush().unwrap();
        let mut input = [0u8; 1];
        io::stdin().read_exact(&mut input).unwrap();
    }

    #[test]
    fn process_exit_releases_static_lock_and_second_process_is_excluded() {
        use std::{
            io::BufRead,
            process::{Command, Stdio},
        };
        let temp = Temporary::new();
        let root = temp.0.join("process");
        for terminate in [false, true] {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["process_lifetime_lock_helper", "--ignored", "--nocapture"])
                .env("RCAM_CONFIG_TEST_DIR", &root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut output = io::BufReader::new(child.stdout.take().unwrap());
            let mut line = String::new();
            loop {
                line.clear();
                assert_ne!(
                    output.read_line(&mut line).unwrap(),
                    0,
                    "child exited before lock acquisition"
                );
                if line.trim() == "RCAM_CONFIG_READY" {
                    break;
                }
            }
            assert_eq!(
                acquire(&root, &[]).err().unwrap().kind(),
                io::ErrorKind::WouldBlock
            );
            if terminate {
                child.kill().unwrap();
                assert!(!child.wait().unwrap().success());
            } else {
                child.stdin.take().unwrap().write_all(b"x").unwrap();
                assert!(child.wait().unwrap().success());
            }
            drop(acquire(&root, &[]).unwrap());
        }
    }
}
