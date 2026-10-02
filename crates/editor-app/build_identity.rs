//! Build-time content binding, not a digital signature or a runtime API.
use std::{collections::BTreeMap, fs, path::Path, process::Command};
#[path = "../editor-core/src/hash.rs"]
mod hash;
type Result<T> = std::result::Result<T, String>;
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    // -C does not override GIT_DIR, GIT_WORK_TREE, index/common-dir or injected
    // config. Repository identity must come from this root's own marker.
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    let output = command
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|_| "Git unavailable")?;
    if !output.status.success() {
        return Err("Git identity command failed".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "non-UTF8 Git identity".into())
}
fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read(root: &Path, name: &str) -> Result<Vec<u8>> {
    let mut current = root.to_path_buf();
    for part in Path::new(name).components() {
        current.push(part);
        if fs::symlink_metadata(&current)
            .map_err(|_| format!("missing package path: {name}"))?
            .file_type()
            .is_symlink()
        {
            return Err("package symlink rejected".into());
        }
    }
    fs::read(root.join(name)).map_err(|_| format!("unreadable package path: {name}"))
}
fn manifest(root: &Path, name: &str) -> Result<BTreeMap<String, String>> {
    let bytes = read(root, name)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "non-UTF8 manifest")?;
    let mut entries = BTreeMap::new();
    for line in text.lines() {
        let (digest, path) = line.split_once("  ").ok_or("invalid manifest entry")?;
        if !hex(digest, 64)
            || path.is_empty()
            || path.contains(['\\', '\r', '\n'])
            || path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || Path::new(path).is_absolute()
            || entries.insert(path.to_owned(), digest.to_owned()).is_some()
        {
            return Err("invalid/duplicate manifest path or digest".into());
        }
        if hash::sha256_hex(&read(root, path)?) != digest {
            return Err(format!("package content mismatch: {path}"));
        }
    }
    if entries.is_empty() {
        return Err("empty manifest".into());
    }
    Ok(entries)
}
fn source_paths(root: &Path) -> Result<Vec<String>> {
    // Explicit distributable set in scripts/source_manifest.py.
    const SUFFIXES: &[&str] = &[
        "csv", "tsv", "rs", "wgsl", "toml", "md", "json", "py", "yml", "yaml", "gbr", "sha256",
        "txt", "log", "png", "jpg", "gbx", "rcam",
    ];
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir).map_err(|_| "unreadable source directory")? {
            let entry = entry.map_err(|_| "unreadable source entry")?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|_| "unreadable source type")?;
            if kind.is_symlink() {
                return Err("source symlink rejected".into());
            }
            if entry.file_name() == "__pycache__" {
                continue;
            }
            if kind.is_dir() {
                walk(root, &path, out)?;
            } else if path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| SUFFIXES.contains(&s))
            {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .ok_or("non-UTF8 source path")?
                        .replace('\\', "/"),
                );
            }
        }
        Ok(())
    }
    let mut paths: Vec<String> = [
        ".gitignore",
        "AGENTS.md",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "README.md",
        "THIRD_PARTY_NOTICES.md",
        "RCam_S5M1_P100K_NATIVE_CLOSEOUT_NEXT_TASK.md",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for dir in ["crates", "docs", ".github", "scripts", "fixtures/synthetic"] {
        walk(root, &root.join(dir), &mut paths)?;
    }
    paths.sort();
    Ok(paths)
}
pub fn resolve(root: &Path) -> Result<(String, &'static str)> {
    let root = root
        .canonicalize()
        .map_err(|_| "workspace root unavailable")?;
    let own_git = match fs::symlink_metadata(root.join(".git")) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Git marker symlink rejected".into());
        }
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err("Git marker unavailable".into()),
    };
    if own_git {
        let top = git(&root, &["rev-parse", "--show-toplevel"])?;
        if Path::new(top.trim())
            .canonicalize()
            .map_err(|_| "Git root unavailable")?
            != root
        {
            return Err("Git root differs from source workspace".into());
        }
        let commit = git(&root, &["rev-parse", "HEAD"])?.trim().to_owned();
        if !hex(&commit, 40) {
            return Err("invalid Git commit".into());
        }
        let dirty = !git(
            &root,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .trim()
        .is_empty();
        return Ok(if dirty {
            (format!("{commit}-dirty"), "git-dirty")
        } else {
            (commit, "git-clean")
        });
    }
    let package = manifest(&root, "PACKAGE_MANIFEST.sha256")?;
    let source = manifest(&root, "MANIFEST.sha256")?;
    let info: serde_json::Value = serde_json::from_slice(&read(&root, "PACKAGE_INFO.json")?)
        .map_err(|_| "invalid PACKAGE_INFO JSON")?;
    let commit = info["git_commit"]
        .as_str()
        .ok_or("missing archive commit")?;
    if info["schema_version"] != 1
        || info["clean_worktree"] != true
        || info["supplemental_only"] != false
        || !hex(commit, 40)
        || info["commit"] != commit
        || info["source_manifest_sha256"] != hash::sha256_hex(&read(&root, "MANIFEST.sha256")?)
    {
        return Err("invalid/non-pristine package metadata".into());
    }
    let actual = source_paths(&root)?;
    if source.keys().cloned().collect::<Vec<_>>() != actual
        || info["source_file_count"].as_u64() != Some(actual.len() as u64)
    {
        return Err("source manifest coverage mismatch".into());
    }
    let mut included = actual;
    included.push("MANIFEST.sha256".into());
    included.sort();
    if info["included_paths"] != serde_json::json!(included)
        || info["manifest_count"].as_u64() != Some(included.len() as u64)
    {
        return Err("package path/count mismatch".into());
    }
    included.push("PACKAGE_INFO.json".into());
    included.sort();
    if package.keys().cloned().collect::<Vec<_>>() != included
        || source
            .iter()
            .any(|(path, digest)| package.get(path) != Some(digest))
    {
        return Err("package/source manifest binding mismatch".into());
    }
    Ok((commit.to_owned(), "archive-verified"))
}
