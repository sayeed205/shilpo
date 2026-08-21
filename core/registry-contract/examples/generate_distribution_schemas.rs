use std::path::PathBuf;

use shilpo_registry_contract::{PackageSignature, SignedRegistryIndex};

fn main() {
    let output_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("core/registry-contract/schema"));

    let sig_schema = serde_json::to_string_pretty(&schemars::schema_for!(PackageSignature))
        .expect("package signature schema should serialize");
    let reg_schema = serde_json::to_string_pretty(&schemars::schema_for!(SignedRegistryIndex))
        .expect("registry index schema should serialize");

    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

    std::fs::write(
        output_dir.join("package-signature-v1.schema.json"),
        format!("{sig_schema}\n"),
    )
    .unwrap_or_else(|error| panic!("failed to write package signature schema: {error}"));

    std::fs::write(
        output_dir.join("registry-index-v1.schema.json"),
        format!("{reg_schema}\n"),
    )
    .unwrap_or_else(|error| panic!("failed to write registry index schema: {error}"));
}
