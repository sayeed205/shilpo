use shilpo_config::ShellConfig;
use std::path::PathBuf;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("desktop/config/schema/config-v1.schema.json"));
    std::fs::write(&output, ShellConfig::schema_json())
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}
