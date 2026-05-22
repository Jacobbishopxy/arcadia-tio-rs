#[test]
fn safe_wrapper_manifest_does_not_depend_on_private_core_crates() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("safe wrapper Cargo.toml is readable");
    let dependency_lines = manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();

    assert!(
        dependency_lines
            .iter()
            .any(|line| line.starts_with("arcadia-tio-sys")),
        "safe wrapper should depend on the public sys crate"
    );
    assert!(
        !dependency_lines
            .iter()
            .any(|line| line.starts_with("arcadia-tio =") || line.starts_with("arcadia-tio-capi")),
        "safe wrapper must not depend on private core/CAPI Rust crates: {dependency_lines:#?}"
    );
}
