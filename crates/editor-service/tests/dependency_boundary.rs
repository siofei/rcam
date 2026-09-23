use std::{collections::BTreeSet, process::Command};

// Reviewed normal dependencies through S4-A2 (ADR 0025: bounded pure Rust material offset). Adding any package
// requires an explicit boundary review, including native window/dialog crates.
const ALLOWED: &str = "editor-service editor-core editor-text rcam-project clipper2-rust ttf-parser gerber-io gerber-types gerber_parser serde serde_core serde_derive serde_json proc-macro2 quote unicode-ident syn chrono iana-time-zone core-foundation-sys num-traits num-rational num-bigint num-integer strum strum_macros heck thiserror thiserror-impl uuid anyhow lazy-regex lazy-regex-proc_macros regex regex-automata regex-syntax aho-corasick memchr once_cell log itoa zmij windows-core windows-implement windows-interface windows-result windows-strings windows-link";

fn unreviewed(tree: &str) -> BTreeSet<&str> {
    let allowed: BTreeSet<_> = ALLOWED.split_whitespace().collect();
    tree.lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| !allowed.contains(name))
        .collect()
}

#[test]
fn service_normal_dependency_gate() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "-p",
            "editor-service",
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
    assert!(
        tree.starts_with("editor-service "),
        "missing dependency tree"
    );
    assert!(
        unreviewed(&tree).is_empty(),
        "unreviewed service dependencies: {:?}",
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
        "unreviewed-package",
    ] {
        assert_eq!(
            unreviewed(&format!("editor-service v0.1.0\n{forbidden} v1.0")),
            BTreeSet::from([forbidden])
        );
    }
}
