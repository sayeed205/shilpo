use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use shilpo_ext_api::{CanonicalId, ContributionId, ExtensionId};

const VALID_EXT_IDS: &[(&str, &str)] = &[
    ("short", "io.github.app"),
    ("medium", "com.example.weather-widget"),
    (
        "near_limit",
        "org.shilpo.long-subdomain-123.extension-package-name.v1-release-candidate",
    ),
];

const INVALID_EXT_IDS: &[(&str, &str)] = &[
    ("missing_segment", "two.segments"),
    ("uppercase", "IO.GITHUB.APP"),
    ("invalid_char", "org.shilpo.invalid_underscore"),
    ("leading_dash", "org.-invalid.app"),
];

const VALID_CONTRIB_IDS: &[(&str, &str)] = &[
    ("short", "clock"),
    ("medium", "weather-widget_v1"),
    (
        "near_limit",
        "a-very-long-contribution-identifier-with-many-words-and-subparts",
    ),
];

const INVALID_CONTRIB_IDS: &[(&str, &str)] = &[
    ("leading_dash", "-leading-dash"),
    ("uppercase", "InvalidCaps"),
    ("invalid_char", "slash/forbidden"),
    ("empty", ""),
];

const VALID_CANONICAL_IDS: &[(&str, &str)] = &[
    ("short", "io.github.app/clock"),
    ("medium", "com.example.weather/widget_v1"),
    (
        "near_limit",
        "org.shilpo.long-subdomain-123.extension-package-name/a-very-long-contribution-identifier",
    ),
];

const INVALID_CANONICAL_IDS: &[(&str, &str)] = &[
    ("no_slash", "io.github.app.clock"),
    ("multiple_slashes", "io.github.app/clock/extra"),
    ("invalid_ext", "invalid/clock"),
    ("invalid_contrib", "io.github.app/-invalid"),
];

fn bench_extension_id(c: &mut Criterion) {
    let mut valid_group = c.benchmark_group("identity/extension_id/valid");
    for &(label, input) in VALID_EXT_IDS {
        valid_group.bench_with_input(BenchmarkId::new("parse", label), &input, |b, &inp| {
            b.iter(|| ExtensionId::new(black_box(inp)));
        });
    }
    valid_group.finish();

    let mut invalid_group = c.benchmark_group("identity/extension_id/invalid");
    for &(label, input) in INVALID_EXT_IDS {
        invalid_group.bench_with_input(BenchmarkId::new("parse", label), &input, |b, &inp| {
            b.iter(|| ExtensionId::new(black_box(inp)));
        });
    }
    invalid_group.finish();
}

fn bench_contribution_id(c: &mut Criterion) {
    let mut valid_group = c.benchmark_group("identity/contribution_id/valid");
    for &(label, input) in VALID_CONTRIB_IDS {
        valid_group.bench_with_input(BenchmarkId::new("parse", label), &input, |b, &inp| {
            b.iter(|| ContributionId::new(black_box(inp)));
        });
    }
    valid_group.finish();

    let mut invalid_group = c.benchmark_group("identity/contribution_id/invalid");
    for &(label, input) in INVALID_CONTRIB_IDS {
        invalid_group.bench_with_input(BenchmarkId::new("parse", label), &input, |b, &inp| {
            b.iter(|| ContributionId::new(black_box(inp)));
        });
    }
    invalid_group.finish();
}

fn bench_canonical_id(c: &mut Criterion) {
    let mut valid_group = c.benchmark_group("identity/canonical_id/parse_valid");
    for &(label, input) in VALID_CANONICAL_IDS {
        valid_group.bench_with_input(BenchmarkId::new("parse", label), &input, |b, &inp| {
            b.iter(|| inp.parse::<CanonicalId>());
        });
    }
    valid_group.finish();

    let mut invalid_group = c.benchmark_group("identity/canonical_id/parse_invalid");
    for &(label, input) in INVALID_CANONICAL_IDS {
        invalid_group.bench_with_input(BenchmarkId::new("parse", label), &input, |b, &inp| {
            b.iter(|| inp.parse::<CanonicalId>());
        });
    }
    invalid_group.finish();

    let mut new_group = c.benchmark_group("identity/canonical_id/new");
    let ext_id = ExtensionId::new("io.github.alice.world-clock").unwrap();
    let contrib_id = ContributionId::new("widget").unwrap();
    new_group.bench_function("composed", |b| {
        b.iter(|| CanonicalId::new(black_box(ext_id.clone()), black_box(contrib_id.clone())));
    });
    new_group.finish();
}

criterion_group!(
    benches,
    bench_extension_id,
    bench_contribution_id,
    bench_canonical_id
);
criterion_main!(benches);
