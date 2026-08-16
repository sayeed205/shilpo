use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use shilpo_theme::{SchemeVariant, generate_m3_palettes, resolve_variant};

struct NamedSeed {
    name: &'static str,
    argb: u32,
}

const SEEDS: &[NamedSeed] = &[
    NamedSeed {
        name: "low_chroma",
        argb: 0xFF75_7778,
    },
    NamedSeed {
        name: "medium_chroma",
        argb: 0xFF00_6C4C,
    },
    NamedSeed {
        name: "high_chroma",
        argb: 0xFFFF_0055,
    },
];

const VARIANTS: &[SchemeVariant] = &[
    SchemeVariant::Auto,
    SchemeVariant::TonalSpot,
    SchemeVariant::Content,
    SchemeVariant::Expressive,
    SchemeVariant::Fidelity,
    SchemeVariant::FruitSalad,
    SchemeVariant::Monochrome,
    SchemeVariant::Neutral,
    SchemeVariant::Rainbow,
];

fn bench_resolve_variant(c: &mut Criterion) {
    let mut group = c.benchmark_group("theme/resolve_variant");
    for seed in SEEDS {
        group.bench_with_input(
            BenchmarkId::new("auto", seed.name),
            &seed.argb,
            |b, &argb| {
                b.iter(|| resolve_variant(black_box(argb), black_box(SchemeVariant::Auto)));
            },
        );
    }
    group.finish();
}

fn bench_generate_palettes(c: &mut Criterion) {
    let mut group = c.benchmark_group("theme/generate_palettes");
    group.throughput(Throughput::Elements(2)); // light + dark palette pair

    for seed in SEEDS {
        for &variant in VARIANTS {
            let param = format!("{}/{}", seed.name, variant.as_str());
            group.bench_with_input(
                BenchmarkId::new("m3", param),
                &(seed.argb, variant),
                |b, &(argb, v)| {
                    b.iter(|| generate_m3_palettes(black_box(argb), black_box(v)));
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_resolve_variant, bench_generate_palettes);
criterion_main!(benches);
