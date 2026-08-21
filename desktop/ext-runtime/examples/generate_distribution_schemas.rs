use std::path::PathBuf;

use shilpo_ext_runtime::script::ScriptManifest;

fn main() {
    let output_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("desktop/ext-runtime/schema"));

    let script_schema = serde_json::to_string_pretty(&schemars::schema_for!(ScriptManifest))
        .expect("script manifest schema should serialize");

    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));

    std::fs::write(
        output_dir.join("script-manifest-v1.schema.json"),
        format!("{script_schema}\n"),
    )
    .unwrap_or_else(|error| panic!("failed to write script manifest schema: {error}"));
}
