use std::fs;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use shilpo::config::{ConfigResolver, ShellConfig};
use tempfile::TempDir;

const TOML_FIXTURE: &str = include_str!("../fixtures/config/valid_full.toml");

fn setup_layered_fixture_dir() -> TempDir {
    let dir = TempDir::new().expect("create temp dir for layered config fixture");
    let base_path = dir.path().join("config.toml");
    fs::write(&base_path, TOML_FIXTURE).expect("write primary config");

    let conf_d = dir.path().join("conf.d");
    fs::create_dir_all(&conf_d).expect("create conf.d");
    fs::write(
        conf_d.join("10-theme.toml"),
        "version = 1\n[theme]\nfont_family = \"Fira Code\"\n",
    )
    .expect("write 10-theme.toml");
    fs::write(
        conf_d.join("20-outputs.toml"),
        "version = 1\n[outputs.DP-1]\nenabled = true\nscale = 1.25\n",
    )
    .expect("write 20-outputs.toml");

    let overrides_path = dir.path().join("overrides.toml");
    fs::write(&overrides_path, "[theme]\ncorner_radius_scale = 1.2\n")
        .expect("write overrides.toml");

    dir
}

fn bench_config_operations(c: &mut Criterion) {
    // 1. TOML deserialization into the canonical Shell configuration type
    let mut deserialize_group = c.benchmark_group("config/deserialize");
    deserialize_group.bench_function("valid_full", |b| {
        b.iter(|| toml::from_str::<ShellConfig>(black_box(TOML_FIXTURE)));
    });
    deserialize_group.finish();

    // 2. Validation of an already parsed configuration
    let parsed_config: ShellConfig =
        toml::from_str(TOML_FIXTURE).expect("fixture must deserialize into ShellConfig");
    assert!(
        parsed_config.validate().is_ok(),
        "parsed config must be valid"
    );

    let mut validate_group = c.benchmark_group("config/validate");
    validate_group.bench_function("valid_full", |b| {
        b.iter(|| black_box(&parsed_config).validate());
    });
    validate_group.finish();

    // 3. Combined parse plus validation
    let mut parse_validate_group = c.benchmark_group("config/parse_and_validate");
    parse_validate_group.bench_function("valid_full", |b| {
        b.iter(|| {
            let config = toml::from_str::<ShellConfig>(black_box(TOML_FIXTURE));
            match config {
                Ok(cfg) => cfg.validate(),
                Err(err) => panic!("deserialization error in benchmark: {err}"),
            }
        });
    });
    parse_validate_group.finish();

    // 4. Canonical layered ConfigResolver initial resolution
    let layered_dir = setup_layered_fixture_dir();
    let resolver = ConfigResolver::new(layered_dir.path());
    let (initial_snapshot, initial_report) = resolver
        .resolve_initial()
        .expect("initial layered resolution must succeed");
    assert_eq!(initial_snapshot.config.version, 1);
    assert!(!initial_report.sources_loaded.is_empty());

    let mut resolve_group = c.benchmark_group("config/resolve_layered");
    resolve_group.bench_function("initial_resolution", |b| {
        b.iter(|| {
            let resolver = black_box(&resolver);
            resolver.resolve_initial()
        });
    });
    resolve_group.finish();
}

criterion_group!(benches, bench_config_operations);
criterion_main!(benches);
