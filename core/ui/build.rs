use std::env;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set by cargo");
    let icon_manifest = std::path::Path::new(&manifest_dir).join("icon_names.txt");

    println!(
        "cargo:rustc-env=SHILPO_ICON_MANIFEST={}",
        icon_manifest.display()
    );
    println!("cargo:rerun-if-changed={}", icon_manifest.display());
    println!("cargo:rerun-if-changed=build.rs");
}
