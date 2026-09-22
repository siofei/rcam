//! `rcam-project` is a pure model + codec crate (§3 of the S4-B2 brief): no
//! `egui`/`eframe`/`wgpu`/`winit`/file-dialog dependency, mirroring
//! `editor-service`'s own `dependency_boundary.rs`.
use std::{collections::BTreeSet, process::Command};

const ALLOWED: &str = "rcam-project editor-core serde serde_core serde_derive serde_json proc-macro2 quote unicode-ident syn itoa memchr zmij";

fn unreviewed(tree: &str) -> BTreeSet<&str> {
    let allowed: BTreeSet<_> = ALLOWED.split_whitespace().collect();
    tree.lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| !allowed.contains(name))
        .collect()
}

#[test]
fn rcam_project_normal_dependency_gate() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "-p",
            "rcam-project",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .output()
        .expect("cargo tree must execute");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    println!("{tree}");
    assert!(tree.starts_with("rcam-project "), "missing dependency tree");
    assert!(
        unreviewed(&tree).is_empty(),
        "unreviewed rcam-project dependencies: {:?}",
        unreviewed(&tree)
    );
}

#[test]
fn dependency_gate_rejects_window_gpu_dialog_and_unknown_crates() {
    for forbidden in [
        "egui",
        "eframe",
        "egui-wgpu",
        "wgpu",
        "winit",
        "rfd",
        "native-dialog",
        "objc2-app-kit",
        "zip",
        "unreviewed-package",
    ] {
        assert_eq!(
            unreviewed(&format!("rcam-project v0.1.0\n{forbidden} v1.0")),
            BTreeSet::from([forbidden])
        );
    }
}
