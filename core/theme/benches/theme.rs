//! Benchmarks for the pure Material 3 theme core.
//!
//! Everything measured here is on the theme daemon's hot path: a wallpaper seed
//! or a user command arrives, palettes are regenerated, and the resulting state
//! is serialized to the consumers. All of it is pure computation (ADR-0002), so
//! it is a good fit for CodSpeed's deterministic CPU simulation.

use shilpo_theme::{
    ColorSource, SchemeVariant, ThemeCommand, ThemeMode, ThemeState, generate_m3_palettes,
    interpolate_argb_oklch, materialize_seed_with_variant, reduce, resolve_variant,
};

fn main() {
    divan::main();
}

const TIMESTAMP: &str = "2026-08-06T20:00:00Z";

/// Seeds spanning the chroma buckets `resolve_variant` discriminates on, so the
/// `Auto` path exercises every variant it can pick.
const SEED_NAMES: &[&str] = &["achromatic", "muted", "vivid", "saturated"];

fn seed(name: &str) -> u32 {
    match name {
        "achromatic" => 0xFF7F7F7F,
        "muted" => 0xFF4C6A92,
        "vivid" => 0xFF6750A4,
        "saturated" => 0xFFE5193C,
        other => unreachable!("unknown seed '{other}'"),
    }
}

const VARIANT_NAMES: &[&str] = &[
    "auto",
    "tonal-spot",
    "content",
    "expressive",
    "fidelity",
    "fruit-salad",
    "monochrome",
    "neutral",
    "rainbow",
];

fn variant(name: &str) -> SchemeVariant {
    SchemeVariant::from_str(name)
}

mod palette {
    use super::*;

    /// Full light + dark token generation from a wallpaper seed: the single most
    /// expensive step of a theme transition.
    #[divan::bench(args = SEED_NAMES)]
    fn generate_auto(bencher: divan::Bencher, seed_name: &str) {
        let source = seed(seed_name);
        bencher.bench(|| {
            generate_m3_palettes(
                divan::black_box(source),
                divan::black_box(SchemeVariant::Auto),
            )
        });
    }

    /// Every M3 scheme variant on a fixed seed, to catch a regression isolated
    /// to one scheme.
    #[divan::bench(args = VARIANT_NAMES)]
    fn generate_variant(bencher: divan::Bencher, variant_name: &str) {
        let source = seed("vivid");
        let scheme = variant(variant_name);
        bencher.bench(|| generate_m3_palettes(divan::black_box(source), divan::black_box(scheme)));
    }

    /// The pure `Auto` resolution the settings UI calls without regenerating
    /// palettes (HCT chroma extraction only).
    #[divan::bench(args = SEED_NAMES)]
    fn resolve_auto_variant(bencher: divan::Bencher, seed_name: &str) {
        let source = seed(seed_name);
        bencher.bench(|| {
            resolve_variant(
                divan::black_box(source),
                divan::black_box(SchemeVariant::Auto),
            )
        });
    }
}

mod oklch {
    use super::*;

    /// One interpolated frame between two chromatic colors (shortest hue arc).
    #[divan::bench]
    fn interpolate_frame() -> u32 {
        interpolate_argb_oklch(
            divan::black_box(0xFF6750A4),
            divan::black_box(0xFF1B6C3A),
            divan::black_box(0.37),
        )
    }

    /// The achromatic guard path, which skips hue rotation.
    #[divan::bench]
    fn interpolate_achromatic() -> u32 {
        interpolate_argb_oklch(
            divan::black_box(0xFF7F7F7F),
            divan::black_box(0xFF202020),
            divan::black_box(0.5),
        )
    }

    /// A whole animated theme transition (ADR-0016): every token of a palette
    /// crossfaded over a 60-frame animation.
    #[divan::bench]
    fn interpolate_transition(bencher: divan::Bencher) {
        let (light, dark) = generate_m3_palettes(seed("vivid"), SchemeVariant::TonalSpot);
        let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(light.len());
        let mut tokens: Vec<&String> = light.keys().collect();
        tokens.sort_unstable();
        for token in tokens {
            let from = parse_hex(&light[token]);
            let to = parse_hex(&dark[token]);
            pairs.push((from, to));
        }

        bencher.bench_local(|| {
            let mut acc = 0u32;
            for frame in 0..60u32 {
                let t = frame as f32 / 59.0;
                for (from, to) in &pairs {
                    acc ^= interpolate_argb_oklch(*from, *to, t);
                }
            }
            acc
        });
    }

    fn parse_hex(value: &str) -> u32 {
        let digits = value.trim_start_matches('#');
        0xFF00_0000 | u32::from_str_radix(digits, 16).unwrap_or(0)
    }
}

mod state {
    use super::*;

    /// Cold start: the daemon builds its initial state, palettes included.
    #[divan::bench]
    fn new_default() -> ThemeState {
        ThemeState::new(divan::black_box(TIMESTAMP))
    }

    /// A mode toggle: no palette work, only the state transition bookkeeping.
    #[divan::bench]
    fn reduce_toggle_mode(bencher: divan::Bencher) {
        let state = ThemeState::new(TIMESTAMP);
        bencher
            .with_inputs(|| state.clone())
            .bench_values(|mut state| {
                reduce(
                    &mut state,
                    divan::black_box(ThemeCommand::ToggleMode),
                    TIMESTAMP,
                )
            });
    }

    /// A no-op command, which must not bump the revision: the cheapest path
    /// through `reduce`.
    #[divan::bench]
    fn reduce_noop(bencher: divan::Bencher) {
        let mut state = ThemeState::new(TIMESTAMP);
        reduce(
            &mut state,
            ThemeCommand::SetMode(ThemeMode::Dark),
            TIMESTAMP,
        );
        bencher
            .with_inputs(|| state.clone())
            .bench_values(|mut state| {
                reduce(
                    &mut state,
                    divan::black_box(ThemeCommand::SetMode(ThemeMode::Dark)),
                    TIMESTAMP,
                )
            });
    }

    /// A new wallpaper seed, which regenerates both palettes.
    #[divan::bench]
    fn reduce_set_seed(bencher: divan::Bencher) {
        let state = ThemeState::new(TIMESTAMP);
        bencher
            .with_inputs(|| state.clone())
            .bench_values(|mut state| {
                reduce(
                    &mut state,
                    divan::black_box(ThemeCommand::SetSeed(0xFF2E7D32)),
                    TIMESTAMP,
                )
            });
    }

    /// Pinning an explicit variant, which also regenerates both palettes.
    #[divan::bench]
    fn reduce_set_scheme_variant(bencher: divan::Bencher) {
        let state = ThemeState::new(TIMESTAMP);
        bencher
            .with_inputs(|| state.clone())
            .bench_values(|mut state| {
                reduce(
                    &mut state,
                    divan::black_box(ThemeCommand::SetSchemeVariant(SchemeVariant::Expressive)),
                    TIMESTAMP,
                )
            });
    }

    /// Switching to the remembered custom seed.
    #[divan::bench]
    fn reduce_switch_to_custom_source(bencher: divan::Bencher) {
        let mut state = ThemeState::new(TIMESTAMP);
        reduce(
            &mut state,
            ThemeCommand::SetCustomSeed(0xFF00695C),
            TIMESTAMP,
        );
        bencher
            .with_inputs(|| state.clone())
            .bench_values(|mut state| {
                reduce(
                    &mut state,
                    divan::black_box(ThemeCommand::SetColorSource(ColorSource::Custom)),
                    TIMESTAMP,
                )
            });
    }

    /// The seam the daemon drives when an external producer pushes a seed with
    /// an image-aware variant resolution.
    #[divan::bench]
    fn materialize_seed(bencher: divan::Bencher) {
        let state = ThemeState::new(TIMESTAMP);
        bencher
            .with_inputs(|| state.clone())
            .bench_values(|mut state| {
                materialize_seed_with_variant(
                    &mut state,
                    divan::black_box(0xFF8E24AA),
                    divan::black_box(SchemeVariant::Expressive),
                    TIMESTAMP,
                )
            });
    }

    /// Publishing the state over D-Bus / to disk.
    #[divan::bench]
    fn serialize_json(bencher: divan::Bencher) {
        let state = ThemeState::new(TIMESTAMP);
        bencher.bench(|| serde_json::to_string(divan::black_box(&state)).unwrap());
    }

    /// Reading a persisted state back.
    #[divan::bench]
    fn deserialize_json(bencher: divan::Bencher) {
        let encoded = serde_json::to_string(&ThemeState::new(TIMESTAMP)).unwrap();
        bencher.bench(|| serde_json::from_str::<ThemeState>(divan::black_box(&encoded)).unwrap());
    }
}
