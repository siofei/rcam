#[path = "../build_identity.rs"]
mod identity;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const COMMIT: &str = "8512a7c3d1d9000f7374f1d1484d413bbb9b41cb";
struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rcam-build-id-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn write(&self, n: &str, d: impl AsRef<[u8]>) {
        let p = self.0.join(n);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, d).unwrap();
    }
    fn git(&self, args: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap().trim().into()
    }
    fn digest(&self, n: &str) -> String {
        editor_core::hash::sha256_hex(&fs::read(self.0.join(n)).unwrap())
    }
    fn seal_package(&self) {
        let i: serde_json::Value =
            serde_json::from_slice(&fs::read(self.0.join("PACKAGE_INFO.json")).unwrap()).unwrap();
        let mut p: Vec<String> = i["included_paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().into())
            .collect();
        p.push("PACKAGE_INFO.json".into());
        p.sort();
        self.write(
            "PACKAGE_MANIFEST.sha256",
            p.iter()
                .map(|n| format!("{}  {n}\n", self.digest(n)))
                .collect::<String>(),
        );
    }
    fn archive() -> Self {
        let t = Self::new();
        let paths = [
            ".gitignore",
            "AGENTS.md",
            "Cargo.lock",
            "Cargo.toml",
            "README.md",
            "RCam_S5M1_P100K_NATIVE_CLOSEOUT_NEXT_TASK.md",
            "THIRD_PARTY_NOTICES.md",
            "crates/demo.rs",
            "rust-toolchain.toml",
        ];
        for p in paths {
            t.write(p, b"fixture\n");
        }
        t.write(
            "MANIFEST.sha256",
            paths
                .iter()
                .map(|n| format!("{}  {n}\n", t.digest(n)))
                .collect::<String>(),
        );
        let mut included: Vec<&str> = paths.into_iter().chain(["MANIFEST.sha256"]).collect();
        included.sort();
        t.write("PACKAGE_INFO.json",serde_json::to_vec(&serde_json::json!({"schema_version":1,"git_commit":COMMIT,"commit":COMMIT,"clean_worktree":true,"supplemental_only":false,"source_manifest_sha256":t.digest("MANIFEST.sha256"),"source_file_count":paths.len(),"manifest_count":included.len(),"included_paths":included})).unwrap());
        t.seal_package();
        t
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn verified_archive_then_source_changed() {
    for path in [
        "crates/demo.rs",
        "RCam_S5M1_P100K_NATIVE_CLOSEOUT_NEXT_TASK.md",
    ] {
        let t = Tree::archive();
        assert_eq!(
            identity::resolve(&t.0).unwrap(),
            (COMMIT.into(), "archive-verified")
        );
        t.write(path, b"changed");
        assert!(identity::resolve(&t.0).is_err(), "{path}");
    }
}
#[test]
fn missing_corrupt_info_and_manifests() {
    for n in [
        "PACKAGE_INFO.json",
        "MANIFEST.sha256",
        "PACKAGE_MANIFEST.sha256",
    ] {
        let t = Tree::archive();
        fs::remove_file(t.0.join(n)).unwrap();
        assert!(identity::resolve(&t.0).is_err());
        t.write(n, b"broken");
        assert!(identity::resolve(&t.0).is_err());
    }
}
#[test]
fn valid_commit_without_binding_rejected() {
    let t = Tree::archive();
    let mut i: serde_json::Value =
        serde_json::from_slice(&fs::read(t.0.join("PACKAGE_INFO.json")).unwrap()).unwrap();
    i["git_commit"] = serde_json::json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    i["commit"] = i["git_commit"].clone();
    t.write("PACKAGE_INFO.json", serde_json::to_vec(&i).unwrap());
    assert!(identity::resolve(&t.0).is_err());
}
#[test]
fn invalid_fields_even_rehashed() {
    for (f, v) in [
        ("clean_worktree", serde_json::json!(false)),
        ("schema_version", serde_json::json!(2)),
        ("source_file_count", serde_json::json!(0)),
        ("manifest_count", serde_json::json!(0)),
        ("commit", serde_json::json!("bogus")),
        ("source_manifest_sha256", serde_json::json!("00")),
    ] {
        let t = Tree::archive();
        let mut i: serde_json::Value =
            serde_json::from_slice(&fs::read(t.0.join("PACKAGE_INFO.json")).unwrap()).unwrap();
        i[f] = v;
        t.write("PACKAGE_INFO.json", serde_json::to_vec(&i).unwrap());
        t.seal_package();
        assert!(identity::resolve(&t.0).is_err(), "{f}");
    }
}
#[test]
fn new_source_rejected() {
    let t = Tree::archive();
    t.write("crates/new.rs", b"new");
    assert!(identity::resolve(&t.0).is_err());
}
#[test]
fn duplicate_paths_rejected() {
    let t = Tree::archive();
    let s = fs::read_to_string(t.0.join("PACKAGE_MANIFEST.sha256")).unwrap();
    t.write("PACKAGE_MANIFEST.sha256", format!("{s}{s}"));
    assert!(identity::resolve(&t.0).is_err());
}
#[test]
fn unrelated_parent_git() {
    let p = Tree::new();
    p.git(&["init", "-q"]);
    p.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "--allow-empty",
        "-qm",
        "parent",
    ]);
    let t = Tree::archive();
    let dest = p.0.join("source");
    fs::rename(&t.0, &dest).unwrap();
    fs::create_dir(&t.0).unwrap();
    assert_eq!(
        identity::resolve(&dest).unwrap(),
        (COMMIT.into(), "archive-verified")
    );
    fs::remove_file(dest.join("PACKAGE_INFO.json")).unwrap();
    assert!(identity::resolve(&dest).is_err());
}
#[test]
fn git_clean_dirty_and_commit() {
    let t = Tree::archive();
    t.git(&["init", "-q"]);
    t.git(&["add", "."]);
    t.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "first",
    ]);
    let a = t.git(&["rev-parse", "HEAD"]);
    assert_eq!(identity::resolve(&t.0).unwrap(), (a.clone(), "git-clean"));
    t.write("crates/demo.rs", b"changed");
    assert_eq!(
        identity::resolve(&t.0).unwrap(),
        (format!("{a}-dirty"), "git-dirty")
    );
    t.git(&["add", "."]);
    t.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "second",
    ]);
    let b = t.git(&["rev-parse", "HEAD"]);
    assert_ne!(a, b);
    assert_eq!(identity::resolve(&t.0).unwrap(), (b, "git-clean"));
}
#[test]
fn broken_own_git_no_fallback() {
    let t = Tree::archive();
    t.write(".git", b"gitdir: missing\n");
    assert!(identity::resolve(&t.0).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn git_marker_symlinks_never_fallback() {
    let t = Tree::archive();
    for target in ["missing-git-dir", "existing-dir"] {
        if target == "existing-dir" {
            fs::create_dir(t.0.join(target)).unwrap();
        }
        std::os::unix::fs::symlink(target, t.0.join(".git")).unwrap();
        assert!(
            identity::resolve(&t.0)
                .unwrap_err()
                .contains("Git marker symlink")
        );
        fs::remove_file(t.0.join(".git")).unwrap();
    }
}
