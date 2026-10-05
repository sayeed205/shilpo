//! Checks expected color behavior against the checked-in reference vectors.

use std::collections::HashMap;

use super::contrast::Contrast;
use super::dynamiccolor::cmf_error_hue;
use super::hct::{Cam16, Hct};
use super::palettes::TonalPalette;
use super::{Platform, Role, Scheme, SchemeOptions, Variant};

const CASES: &str = include_str!("fixtures/dynamic_cases_2026.tsv");
const ROLES: &str = include_str!("fixtures/dynamic_roles_2026.tsv");
const EDGES: &str = include_str!("fixtures/dynamic_edges_2026.tsv");
const ROLE_MANIFEST: &str = include_str!("fixtures/dynamic_role_manifest_2026.tsv");
const HCT_VECTORS: &str = include_str!("fixtures/hct_vectors_2026.tsv");
const CAM16_VECTORS: &str = include_str!("fixtures/cam16_vectors_2026.tsv");
const PALETTE_VECTORS: &str = include_str!("fixtures/tonal_palette_vectors_2026.tsv");
const CMF_HUE_BOUNDARIES: &str =
    include_str!("fixtures/cmf_error_hue_boundaries_2026.tsv");
const CMF_VALIDITY: &str = include_str!("fixtures/cmf_validity_2026.tsv");
const ALPHA_CASES: &str = include_str!("fixtures/alpha_cases_2026.tsv");
const ALPHA_ROLES: &str = include_str!("fixtures/alpha_roles_2026.tsv");
const ALPHA_SCIENCE: &str = include_str!("fixtures/alpha_science_2026.tsv");
const NAN_CONTRAST: &str = include_str!("fixtures/nan_contrast_2026.tsv");

fn rows(tsv: &'static str) -> impl Iterator<Item = (usize, Vec<&'static str>)> {
    tsv.lines()
        .enumerate()
        .skip(1)
        .map(|(index, line)| (index + 2, line.split('\t').collect()))
}

fn argb(value: &str) -> u32 {
    u32::from_str_radix(value.trim_start_matches("0x"), 16)
        .unwrap_or_else(|error| panic!("invalid ARGB {value}: {error}"))
}

fn variant(value: &str) -> Variant {
    match value {
        "monochrome" => Variant::Monochrome,
        "neutral" => Variant::Neutral,
        "tonal_spot" => Variant::TonalSpot,
        "vibrant" => Variant::Vibrant,
        "expressive" => Variant::Expressive,
        "fidelity" => Variant::Fidelity,
        "content" => Variant::Content,
        "rainbow" => Variant::Rainbow,
        "fruit_salad" => Variant::FruitSalad,
        "cmf" => Variant::Cmf,
        unknown => panic!("unknown variant {unknown}"),
    }
}

fn role(value: &str) -> Role {
    match value {
        "highestSurface" => Role::HighestSurface,
        "primaryPaletteKeyColor" => Role::PrimaryPaletteKeyColor,
        "secondaryPaletteKeyColor" => Role::SecondaryPaletteKeyColor,
        "tertiaryPaletteKeyColor" => Role::TertiaryPaletteKeyColor,
        "neutralPaletteKeyColor" => Role::NeutralPaletteKeyColor,
        "neutralVariantPaletteKeyColor" => Role::NeutralVariantPaletteKeyColor,
        "errorPaletteKeyColor" => Role::ErrorPaletteKeyColor,
        "background" => Role::Background,
        "onBackground" => Role::OnBackground,
        "surface" => Role::Surface,
        "surfaceDim" => Role::SurfaceDim,
        "surfaceBright" => Role::SurfaceBright,
        "surfaceContainerLowest" => Role::SurfaceContainerLowest,
        "surfaceContainerLow" => Role::SurfaceContainerLow,
        "surfaceContainer" => Role::SurfaceContainer,
        "surfaceContainerHigh" => Role::SurfaceContainerHigh,
        "surfaceContainerHighest" => Role::SurfaceContainerHighest,
        "onSurface" => Role::OnSurface,
        "surfaceVariant" => Role::SurfaceVariant,
        "onSurfaceVariant" => Role::OnSurfaceVariant,
        "outline" => Role::Outline,
        "outlineVariant" => Role::OutlineVariant,
        "inverseSurface" => Role::InverseSurface,
        "inverseOnSurface" => Role::InverseOnSurface,
        "shadow" => Role::Shadow,
        "scrim" => Role::Scrim,
        "surfaceTint" => Role::SurfaceTint,
        "primary" => Role::Primary,
        "primaryDim" => Role::PrimaryDim,
        "onPrimary" => Role::OnPrimary,
        "primaryContainer" => Role::PrimaryContainer,
        "onPrimaryContainer" => Role::OnPrimaryContainer,
        "inversePrimary" => Role::InversePrimary,
        "primaryFixed" => Role::PrimaryFixed,
        "primaryFixedDim" => Role::PrimaryFixedDim,
        "onPrimaryFixed" => Role::OnPrimaryFixed,
        "onPrimaryFixedVariant" => Role::OnPrimaryFixedVariant,
        "secondary" => Role::Secondary,
        "secondaryDim" => Role::SecondaryDim,
        "onSecondary" => Role::OnSecondary,
        "secondaryContainer" => Role::SecondaryContainer,
        "onSecondaryContainer" => Role::OnSecondaryContainer,
        "secondaryFixed" => Role::SecondaryFixed,
        "secondaryFixedDim" => Role::SecondaryFixedDim,
        "onSecondaryFixed" => Role::OnSecondaryFixed,
        "onSecondaryFixedVariant" => Role::OnSecondaryFixedVariant,
        "tertiary" => Role::Tertiary,
        "tertiaryDim" => Role::TertiaryDim,
        "onTertiary" => Role::OnTertiary,
        "tertiaryContainer" => Role::TertiaryContainer,
        "onTertiaryContainer" => Role::OnTertiaryContainer,
        "tertiaryFixed" => Role::TertiaryFixed,
        "tertiaryFixedDim" => Role::TertiaryFixedDim,
        "onTertiaryFixed" => Role::OnTertiaryFixed,
        "onTertiaryFixedVariant" => Role::OnTertiaryFixedVariant,
        "error" => Role::Error,
        "errorDim" => Role::ErrorDim,
        "onError" => Role::OnError,
        "errorContainer" => Role::ErrorContainer,
        "onErrorContainer" => Role::OnErrorContainer,
        unknown => panic!("unknown MaterialDynamicColors role {unknown}"),
    }
}

fn palette(hue: f64, chroma: f64) -> TonalPalette {
    TonalPalette::from_hue_and_chroma(hue, chroma)
}

fn options_from_case(columns: &[&str]) -> (Vec<u32>, SchemeOptions) {
    assert_eq!(columns.len(), 17);
    let variant = variant(columns[2]);
    assert_eq!(columns[3], "2026", "all fixture requests target the latest spec");
    let platform = match columns[5] {
        "phone" => Platform::Phone,
        "watch" => Platform::Watch,
        unknown => panic!("unknown platform {unknown}"),
    };
    let is_dark = match columns[6] {
        "light" => false,
        "dark" => true,
        unknown => panic!("unknown mode {unknown}"),
    };
    let contrast = columns[7].parse::<f64>().expect("contrast is numeric");
    let source_colors = columns[12].split('|').map(argb).collect::<Vec<_>>();
    assert_eq!(source_colors.len(), columns[10].parse::<usize>().unwrap());

    let mut options = SchemeOptions::new(variant, is_dark)
        .with_platform(platform)
        .with_contrast_level(contrast);
    for name in columns[16].split(',').filter(|value| *value != "none") {
        options = match name {
            "primaryPalette" => options.with_primary_palette(palette(20.0, 64.0)),
            "secondaryPalette" => options.with_secondary_palette(palette(70.0, 40.0)),
            "tertiaryPalette" => options.with_tertiary_palette(palette(140.0, 52.0)),
            "neutralPalette" => options.with_neutral_palette(palette(260.0, 8.0)),
            "neutralVariantPalette" => {
                options.with_neutral_variant_palette(palette(300.0, 16.0))
            }
            "errorPalette" => options.with_error_palette(palette(10.0, 84.0)),
            unknown => panic!("unknown palette override {unknown}"),
        };
    }
    (source_colors, options)
}

fn scheme_for_case(columns: &[&str]) -> Scheme {
    let (source_colors, options) = options_from_case(columns);
    let scheme = Scheme::new(&source_colors, options)
        .unwrap_or_else(|error| panic!("{}: {error}", columns[0]));
    assert_eq!(scheme.effective_spec(), columns[4], "{}", columns[0]);
    scheme
}

fn assert_fixture_role(case_id: &str, role: Role, expected_argb: u32, expected_tone: f64) {
    let (_, columns) = rows(CASES)
        .find(|(_, columns)| columns[0] == case_id)
        .unwrap_or_else(|| panic!("unknown fixture case {case_id}"));
    let scheme = scheme_for_case(&columns);
    assert_eq!(
        scheme.resolved_role(role),
        Some((expected_argb, expected_tone)),
        "{case_id} / {}",
        role.name()
    );
}

#[test]
fn targeted_dynamic_color_regressions_match_pinned_reference_roles() {
    // Monochrome primary has the canonical endpoint tones in both light and
    // dark mode (the delegate remains the effective 2021 spec here).
    assert_fixture_role("S1003", Role::Primary, 0xFF000000, 0.0);
    assert_fixture_role("S1008", Role::Primary, 0xFFFFFFFF, 100.0);

    // Neutral dark phone primary's relative tone relationship is not an
    // absolute light/dark polarity.
    assert_fixture_role("S0028", Role::Primary, 0xFFD5C2C6, 80.0);
    assert_fixture_role("S0028", Role::PrimaryContainer, 0xFF514347, 30.0);
    assert_fixture_role("S0028", Role::PrimaryDim, 0xFFC7B4B8, 75.0);

    // Dark expressive yellow exercises the dark-mode primary cap and the
    // minimum-chroma primary-container search.
    assert_fixture_role("S1288", Role::Primary, 0xFFF9F89E, 96.0);
    assert_fixture_role("S1288", Role::PrimaryContainer, 0xFF4C4C01, 31.0);

    // On-background preserves on-surface constraints/chroma while retaining
    // the canonical low-contrast and normal-contrast tones.
    assert_fixture_role(
        "S0001",
        Role::OnBackground,
        0xFF909090,
        59.76366698638103,
    );
    assert_fixture_role("S0003", Role::OnBackground, 0xFF1B1B1B, 10.0);

    // CMF watch background inherits the 2026 surface definition, not the
    // older 2025 yellow-surface tone.
    assert_fixture_role("S1393", Role::Background, 0xFFFEFBD7, 98.0);
    assert_fixture_role("S1393", Role::OnBackground, 0xFF343407, 20.934908167694722);
}

#[test]
fn nan_contrast_2025_roles_match_pinned_reference_vectors() {
    let mut covered_variants = std::collections::HashSet::new();
    let mut covered_branches = std::collections::HashSet::new();
    let mut count = 0;

    for (line, columns) in rows(NAN_CONTRAST) {
        assert_eq!(columns.len(), 14, "nan_contrast_2026.tsv:{line}");
        assert_eq!(columns[2], "2025", "nan_contrast_2026.tsv:{line}");
        assert_eq!(columns[5], "NaN", "contrast must remain explicitly NaN");
        let contrast = columns[5].parse::<f64>().expect("NaN contrast is numeric");
        assert!(contrast.is_nan());

        let source = argb(columns[6]);
        let variant = variant(columns[1]);
        let platform = match columns[3] {
            "phone" => Platform::Phone,
            "watch" => Platform::Watch,
            unknown => panic!("unknown platform {unknown}"),
        };
        let is_dark = match columns[4] {
            "light" => false,
            "dark" => true,
            unknown => panic!("unknown mode {unknown}"),
        };
        let scheme = Scheme::new(
            &[source],
            SchemeOptions::new(variant, is_dark)
                .with_platform(platform)
                .with_contrast_level(contrast),
        )
        .unwrap();
        assert_eq!(scheme.effective_spec(), columns[2], "{}", columns[0]);

        let role_name = columns[7];
        let actual = scheme.resolved_role(role(role_name));
        let expected = (
            argb(columns[13]),
            columns[12].parse::<f64>().expect("canonical tone is numeric"),
        );
        assert_eq!(actual, Some(expected), "{} / {role_name}", columns[0]);

        // The canonical test vectors are selected so the pre-adjustment tone
        // already meets the required curve; NaN still requires recalculation.
        let base_tone = columns[9].parse::<f64>().unwrap();
        let background_tone = columns[10].parse::<f64>().unwrap();
        let required_ratio = columns[11].parse::<f64>().unwrap();
        assert!(
            Contrast::ratio_of_tones(background_tone, base_tone) >= required_ratio,
            "{} did not start with sufficient contrast",
            columns[0]
        );
        assert_ne!(expected.1, base_tone, "{} must exercise recalculation", columns[0]);
        covered_variants.insert(columns[1]);
        covered_branches.insert(columns[8]);
        count += 1;
    }

    assert_eq!(count, 9);
    assert_eq!(covered_variants.len(), 4);
    assert!(covered_branches.contains("single"));
    assert!(covered_branches.contains("tone_delta_pair"));
}

#[test]
fn requested_2026_scheme_roles_match_every_upstream_reference_row() {
    let mut schemes = HashMap::<String, Scheme>::new();
    for (line, columns) in rows(CASES) {
        let scheme = scheme_for_case(&columns);
        let previous = schemes.insert(columns[0].to_owned(), scheme);
        assert!(previous.is_none(), "duplicate case id on line {line}");
    }
    assert_eq!(schemes.len(), 5_280);
    assert_eq!(rows(ROLE_MANIFEST).count(), 60);

    let mut compared_standard = 0usize;
    let mut mismatches = Vec::new();
    for (line, columns) in rows(ROLES) {
        assert_eq!(columns.len(), 5, "dynamic_roles_2026.tsv:{line}");
        let case = schemes
            .get(columns[0])
            .unwrap_or_else(|| panic!("unknown case {} on line {line}", columns[0]));
        let material_role = role(columns[2]);
        let expected_argb = (columns[3] != "null").then(|| argb(columns[3]));
        let expected_tone = (columns[4] != "null")
            .then(|| columns[4].parse::<f64>().expect("tone is numeric"));
        let actual = case.resolved_role(material_role);
        match (actual, expected_argb, expected_tone) {
            (Some((actual_argb, actual_tone)), Some(expected_argb), Some(expected_tone)) => {
                if actual_argb != expected_argb || actual_tone != expected_tone {
                    mismatches.push(format!(
                        "dynamic_roles_2026.tsv:{line} {} / {}: ARGB {actual_argb:#010x} vs {expected_argb:#010x}; tone {actual_tone} vs {expected_tone}",
                        columns[0], columns[2]
                    ));
                }
            }
            (None, None, None) => {}
            (actual, expected_argb, expected_tone) => panic!(
                "dynamic_roles_2026.tsv:{line} {} / {}: actual {actual:?}, expected {expected_argb:?} / {expected_tone:?}",
                columns[0], columns[2]
            ),
        }
        compared_standard += 1;
    }
    assert_eq!(compared_standard, 192_000);

    let mut compared_edges = 0usize;
    for (line, columns) in rows(EDGES) {
        assert_eq!(columns.len(), 5, "dynamic_edges_2026.tsv:{line}");
        let case = schemes
            .get(columns[0])
            .unwrap_or_else(|| panic!("unknown case {} on line {line}", columns[0]));
        let actual = case.resolved_role(role(columns[2]));
        let expected_argb = (columns[3] != "null").then(|| argb(columns[3]));
        let expected_tone = (columns[4] != "null")
            .then(|| columns[4].parse::<f64>().expect("tone is numeric"));
        match (actual, expected_argb, expected_tone) {
            (Some((actual_argb, actual_tone)), Some(expected_argb), Some(expected_tone)) => {
                if actual_argb != expected_argb || actual_tone != expected_tone {
                    mismatches.push(format!(
                        "dynamic_edges_2026.tsv:{line} {} / {}: ARGB {actual_argb:#010x} vs {expected_argb:#010x}; tone {actual_tone} vs {expected_tone}",
                        columns[0], columns[2]
                    ));
                }
            }
            (None, None, None) => {}
            (actual, expected_argb, expected_tone) => panic!(
                "dynamic_edges_2026.tsv:{line} {} / {}: actual {actual:?}, expected {expected_argb:?} / {expected_tone:?}",
                columns[0], columns[2]
            ),
        }
        compared_edges += 1;
    }
    assert_eq!(compared_edges, 124_800);
    assert!(mismatches.is_empty(), "{} parity mismatches:\n{}",
        mismatches.len(), mismatches.iter().take(100).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn alpha_scheme_roles_match_every_upstream_reference_row() {
    let mut schemes = HashMap::<String, Scheme>::new();
    for (line, columns) in rows(ALPHA_CASES) {
        let scheme = scheme_for_case(&columns);
        let previous = schemes.insert(columns[0].to_owned(), scheme);
        assert!(previous.is_none(), "duplicate alpha case id on line {line}");
    }
    assert_eq!(schemes.len(), 1_440);

    let mut compared = 0usize;
    let mut mismatches = Vec::new();
    for (line, columns) in rows(ALPHA_ROLES) {
        assert_eq!(columns.len(), 5, "alpha_roles_2026.tsv:{line}");
        let case = schemes
            .get(columns[0])
            .unwrap_or_else(|| panic!("unknown alpha case {} on line {line}", columns[0]));
        let actual = case.resolved_role(role(columns[2]));
        let expected_argb = (columns[3] != "null").then(|| argb(columns[3]));
        let expected_tone = (columns[4] != "null")
            .then(|| columns[4].parse::<f64>().expect("tone is numeric"));
        match (actual, expected_argb, expected_tone) {
            (Some((actual_argb, actual_tone)), Some(expected_argb), Some(expected_tone)) => {
                if actual_argb != expected_argb || actual_tone != expected_tone {
                    mismatches.push(format!(
                        "alpha_roles_2026.tsv:{line} {} / {}: ARGB {actual_argb:#010x} vs {expected_argb:#010x}; tone {actual_tone} vs {expected_tone}",
                        columns[0], columns[2]
                    ));
                }
            }
            (None, None, None) => {}
            (actual, expected_argb, expected_tone) => panic!(
                "alpha_roles_2026.tsv:{line} {} / {}: actual {actual:?}, expected {expected_argb:?} / {expected_tone:?}",
                columns[0], columns[2]
            ),
        }
        compared += 1;
    }
    assert_eq!(compared, 86_400);
    assert!(mismatches.is_empty(), "{} alpha parity mismatches:\n{}",
        mismatches.len(), mismatches.iter().take(100).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn alpha_science_vectors_match_hct_and_cam16_semantics_exactly() {
    let mut count = 0usize;
    for (line, columns) in rows(ALPHA_SCIENCE) {
        assert_eq!(columns.len(), 24, "alpha_science_2026.tsv:{line}");
        let input = argb(columns[1]);
        assert_eq!(((input >> 24) & 0xff) as u8, columns[2].parse::<u8>().unwrap(), "line {line}");
        assert_eq!(((input >> 16) & 0xff) as u8, columns[3].parse::<u8>().unwrap(), "line {line}");
        assert_eq!(((input >> 8) & 0xff) as u8, columns[4].parse::<u8>().unwrap(), "line {line}");
        assert_eq!((input & 0xff) as u8, columns[5].parse::<u8>().unwrap(), "line {line}");

        let hct = Hct::from_int(input);
        assert_eq!(hct.to_int(), argb(columns[6]), "line {line}");
        assert_eq!(((hct.to_int() >> 24) & 0xff) as u8, columns[7].parse::<u8>().unwrap(), "line {line}");
        assert_eq!(hct.hue(), columns[8].parse::<f64>().unwrap(), "line {line}");
        assert_eq!(hct.chroma(), columns[9].parse::<f64>().unwrap(), "line {line}");
        assert_eq!(hct.tone(), columns[10].parse::<f64>().unwrap(), "line {line}");
        assert_eq!(Hct::from(hct.hue(), hct.chroma(), hct.tone()).to_int(), argb(columns[11]), "line {line}");

        let cam = Cam16::from_int(input);
        let expected = [
            columns[12].parse::<f64>().unwrap(),
            columns[13].parse::<f64>().unwrap(),
            columns[14].parse::<f64>().unwrap(),
            columns[15].parse::<f64>().unwrap(),
            columns[16].parse::<f64>().unwrap(),
            columns[17].parse::<f64>().unwrap(),
            columns[18].parse::<f64>().unwrap(),
            columns[19].parse::<f64>().unwrap(),
            columns[20].parse::<f64>().unwrap(),
        ];
        let actual = [cam.hue, cam.chroma, cam.j, cam.q, cam.m, cam.s, cam.jstar, cam.astar, cam.bstar];
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "alpha_science_2026.tsv:{line}, coordinate {index}");
        }
        let cam_argb = cam.to_int();
        assert_eq!(cam_argb, argb(columns[21]), "line {line}");
        assert_eq!(((cam_argb >> 24) & 0xff) as u8, columns[22].parse::<u8>().unwrap(), "line {line}");
        assert_eq!(format!("{:06X}", cam_argb & 0x00ff_ffff), columns[23], "line {line}");
        count += 1;
    }
    assert_eq!(count, 30);
}

#[test]
fn hct_vectors_match_exactly() {
    let mut count = 0;
    for (line, columns) in rows(HCT_VECTORS) {
        assert_eq!(columns.len(), 6, "hct_vectors_2026.tsv:{line}");
        let hct = Hct::from_int(argb(columns[1]));
        assert_eq!(hct.hue(), columns[2].parse::<f64>().unwrap(), "line {line}");
        assert_eq!(hct.chroma(), columns[3].parse::<f64>().unwrap(), "line {line}");
        assert_eq!(hct.tone(), columns[4].parse::<f64>().unwrap(), "line {line}");
        assert_eq!(hct.to_int(), argb(columns[5]), "line {line}");
        count += 1;
    }
    assert_eq!(count, 8);
}

#[test]
fn cam16_vectors_match_exactly() {
    let mut count = 0;
    for (line, columns) in rows(CAM16_VECTORS) {
        assert_eq!(columns.len(), 16, "cam16_vectors_2026.tsv:{line}");
        let cam = match columns[1] {
            "from_int_default_viewing_conditions" => Cam16::from_int(argb(columns[2])),
            "from_jch_default_viewing_conditions" => Cam16::from_jch(
                columns[3].parse().unwrap(),
                columns[4].parse().unwrap(),
                columns[5].parse().unwrap(),
            ),
            "from_ucs_default_viewing_conditions" => {
                let source = Cam16::from_int(argb(columns[2]));
                Cam16::from_ucs(source.jstar, source.astar, source.bstar)
            }
            unknown => panic!("unknown CAM16 operation {unknown}"),
        };
        let expected = [
            columns[6].parse::<f64>().unwrap(),
            columns[7].parse::<f64>().unwrap(),
            columns[8].parse::<f64>().unwrap(),
            columns[9].parse::<f64>().unwrap(),
            columns[10].parse::<f64>().unwrap(),
            columns[11].parse::<f64>().unwrap(),
            columns[12].parse::<f64>().unwrap(),
            columns[13].parse::<f64>().unwrap(),
            columns[14].parse::<f64>().unwrap(),
        ];
        let actual = [cam.hue, cam.chroma, cam.j, cam.q, cam.m, cam.s, cam.jstar, cam.astar, cam.bstar];
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "cam16_vectors_2026.tsv:{line}, coordinate {index}");
        }
        assert_eq!(cam.to_int(), argb(columns[15]), "line {line}");
        count += 1;
    }
    assert_eq!(count, 12);
}

#[test]
fn tonal_palette_fractional_tones_and_yellow_t99_match_exactly() {
    let mut count = 0;
    for (line, columns) in rows(PALETTE_VECTORS) {
        assert_eq!(columns.len(), 11, "tonal_palette_vectors_2026.tsv:{line}");
        let hue = columns[1].parse::<f64>().unwrap();
        let chroma = columns[2].parse::<f64>().unwrap();
        let requested_tone = columns[3].parse::<f64>().unwrap();
        let is_yellow = columns[4].parse::<bool>().unwrap();
        let tonal_palette = palette(hue, chroma);
        assert_eq!(Hct::is_yellow(hue), is_yellow, "line {line}");
        assert_eq!(tonal_palette.key_color().to_int(), argb(columns[5]), "line {line}");
        assert_eq!(tonal_palette.key_color().tone(), columns[6].parse::<f64>().unwrap(), "line {line}");
        assert_eq!(tonal_palette.tone(requested_tone), argb(columns[7]), "line {line}");
        assert_eq!(Hct::from(hue, chroma, requested_tone).to_int(), argb(columns[8]), "line {line}");
        assert_eq!(tonal_palette.tone(98), argb(columns[9]), "line {line}");
        assert_eq!(tonal_palette.tone(100), argb(columns[10]), "line {line}");
        count += 1;
    }
    assert_eq!(count, 20);
}

#[test]
fn cmf_error_palette_hue_matches_every_inclusive_boundary_vector() {
    let mut count = 0;
    for (line, columns) in rows(CMF_HUE_BOUNDARIES) {
        assert_eq!(columns.len(), 3, "cmf_error_hue_boundaries_2026.tsv:{line}");
        let primary_hue = columns[0].parse::<f64>().unwrap();
        let tertiary_hue = columns[1].parse::<f64>().unwrap();
        let expected = columns[2].parse::<f64>().unwrap();
        assert_eq!(cmf_error_hue(primary_hue, tertiary_hue), expected, "line {line}");
        count += 1;
    }
    assert_eq!(count, 520);
}

#[test]
fn cmf_source_and_override_inputs_follow_the_canonical_constructor_rules() {
    let mut accepted = 0;
    for (line, columns) in rows(CMF_VALIDITY) {
        assert_eq!(columns.len(), 16, "cmf_validity_2026.tsv:{line}");
        match columns[9] {
            "accepted" if columns[0].starts_with("cmf_") => {
                let source_colors = columns[4].split('|').map(argb).collect::<Vec<_>>();
                let options = SchemeOptions::new(Variant::Cmf, columns[7] == "dark")
                    .with_platform(if columns[6] == "watch" { Platform::Watch } else { Platform::Phone })
                    .with_contrast_level(columns[8].parse().unwrap());
                let scheme = Scheme::new(&source_colors, options).unwrap();
                assert_eq!(scheme.effective_spec(), columns[11], "line {line}");
                assert_eq!(scheme.primary_palette().hue(), columns[12].parse::<f64>().unwrap(), "line {line}");
                assert_eq!(scheme.tertiary_palette().hue(), columns[13].parse::<f64>().unwrap(), "line {line}");
                assert_eq!(scheme.error_palette().hue(), columns[14].parse::<f64>().unwrap(), "line {line}");
                accepted += 1;
            }
            "throws" | "accepted" => {
                // Legacy spec selectors, absent generic sources, and partial
                // CMF palettes are intentionally not part of the public API.
                if columns[0] == "cmf_empty_source_list" {
                    assert!(matches!(
                        Scheme::new(&[], SchemeOptions::new(Variant::Cmf, false)),
                        Err(super::SchemeError::EmptySourceColors)
                    ));
                }
                if columns[0] == "generic_cmf_with_partial_palette_override" {
                    assert!(matches!(
                        Scheme::new(
                            &[0xff0000ff],
                            SchemeOptions::new(Variant::Cmf, false)
                                .with_primary_palette(palette(20.0, 64.0)),
                        ),
                        Err(super::SchemeError::PartialCmfPaletteOverrides)
                    ));
                }
            }
            outcome => panic!("unexpected CMF validity outcome {outcome} on line {line}"),
        }
    }
    assert_eq!(accepted, 7);
}
