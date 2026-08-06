use shilpo_ext::ExtensionManifest;
use std::path::PathBuf;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates/ext/schema/extension-v1.schema.json"));
    let schema = ExtensionManifest::schema_json().expect("manifest schema should serialize");
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", parent.display()));
    }
    std::fs::write(&output, format!("{schema}\n"))
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}
