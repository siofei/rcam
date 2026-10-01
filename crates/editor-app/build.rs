mod build_identity;

fn main() {
    // This path cannot exist: Cargo.toml must be a file for Cargo to build this
    // package. Unlike a root sentinel, a child of that file cannot disable the
    // locked Cargo toolchain's missing-input refresh on each invocation.
    println!("cargo:rerun-if-changed=Cargo.toml/.rcam-identity-always-recheck");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let (commit, source) = build_identity::resolve(root)
        .unwrap_or_else(|error| panic!("RCam build source identity rejected: {error}"));
    println!("cargo:rustc-env=RCAM_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RCAM_BUILD_SOURCE={source}");
}
