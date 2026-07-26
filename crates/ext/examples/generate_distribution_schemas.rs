use shilpo_ext::{PackageSignature, SignedRegistryIndex};
use std::path::PathBuf;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates/ext/schema"));
    std::fs::create_dir_all(&output)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output.display()));
    write_schema::<PackageSignature>(&output.join("package-signature-v1.schema.json"));
    write_schema::<SignedRegistryIndex>(&output.join("registry-index-v1.schema.json"));
}

fn write_schema<T: schemars::JsonSchema>(path: &std::path::Path) {
    let schema = serde_json::to_string_pretty(&schemars::schema_for!(T))
        .expect("distribution schema should serialize");
    std::fs::write(path, format!("{schema}\n"))
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}
