use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::collections::HashMap;
use std::fmt;

use mcu_material_color::Hct;

/// Deterministic placeholder timestamp used by `ThemeState::default()` so the pure
/// default carries no hidden clock I/O (ADR-0002). System-boundary callers (e.g.
/// `shilpo-theme-daemon`) must replace it with a real clock time when constructing
/// live state.
pub const DEFAULT_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

const DEFAULT_SOURCE_ARGB: u32 = 0xff006c4c;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, JsonSchema)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
    System,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    pub fn is_system(self) -> bool {
        matches!(self, Self::System)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

impl fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for ThemeMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ThemeMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "light" => Ok(ThemeMode::Light),
            "dark" => Ok(ThemeMode::Dark),
            "system" => Ok(ThemeMode::System),
            _ => Err(de::Error::custom(format!(
                "invalid theme mode '{}': expected 'light', 'dark', or 'system'",
                s
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ColorSource {
    #[default]
    Wallpaper,
    Custom,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum SchemeVariant {
    #[default]
    Auto,
    TonalSpot,
    Content,
    Expressive,
    Fidelity,
    FruitSalad,
    Monochrome,
    Neutral,
    Rainbow,
}

impl SchemeVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::TonalSpot => "tonal-spot",
            Self::Content => "content",
            Self::Expressive => "expressive",
            Self::Fidelity => "fidelity",
            Self::FruitSalad => "fruit-salad",
            Self::Monochrome => "monochrome",
            Self::Neutral => "neutral",
            Self::Rainbow => "rainbow",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::TonalSpot => "Tonal Spot",
            Self::Content => "Content",
            Self::Expressive => "Expressive",
            Self::Fidelity => "Fidelity",
            Self::FruitSalad => "Fruit Salad",
            Self::Monochrome => "Monochrome",
            Self::Neutral => "Neutral",
            Self::Rainbow => "Rainbow",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().replace(' ', "-").as_str() {
            "auto" => Self::Auto,
            "tonal-spot" | "tonal_spot" | "tonal" => Self::TonalSpot,
            "content" => Self::Content,
            "expressive" => Self::Expressive,
            "fidelity" => Self::Fidelity,
            "fruit-salad" | "fruit_salad" => Self::FruitSalad,
            "monochrome" => Self::Monochrome,
            "neutral" => Self::Neutral,
            "rainbow" => Self::Rainbow,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ThemeState {
    pub revision: u64,
    pub selected_mode: ThemeMode,
    pub resolved_mode: ThemeMode,
    pub color_source: ColorSource,
    #[serde(default)]
    pub scheme_variant: SchemeVariant,
    #[serde(default)]
    pub resolved_variant: SchemeVariant,
    pub custom_seed: Option<u32>,
    pub source_argb: u32,
    pub light: HashMap<String, String>,
    pub dark: HashMap<String, String>,
    pub updated_at: String,
    pub palette_generated_at: String,
}

impl ThemeState {
    pub fn new(timestamp: &str) -> Self {
        let resolved_variant = resolve_variant(DEFAULT_SOURCE_ARGB, SchemeVariant::Auto);
        let (light, dark) = generate_m3_palettes(DEFAULT_SOURCE_ARGB, SchemeVariant::Auto);
        Self {
            revision: 1,
            selected_mode: ThemeMode::System,
            resolved_mode: ThemeMode::Light,
            color_source: ColorSource::Wallpaper,
            scheme_variant: SchemeVariant::Auto,
            resolved_variant,
            custom_seed: None,
            source_argb: DEFAULT_SOURCE_ARGB,
            light,
            dark,
            updated_at: timestamp.to_string(),
            palette_generated_at: timestamp.to_string(),
        }
    }

    pub fn palette_algorithm(&self) -> String {
        format!(
            "Material3-{}",
            self.scheme_variant.display_name().replace(' ', "")
        )
    }
}

impl Default for ThemeState {
    fn default() -> Self {
        Self::new(DEFAULT_TIMESTAMP)
    }
}

pub fn argb_to_hex(argb: u32) -> String {
    format!("#{:06X}", argb & 0x00FF_FFFF)
}

/// Resolve a [`SchemeVariant`] against a seed. `Auto` picks a concrete variant
/// from the seed's HCT chroma; any explicit variant passes through unchanged.
/// Pure, so callers (e.g. settings) can show what `Auto` resolves to without
/// regenerating palettes.
pub fn resolve_variant(source_argb: u32, variant: SchemeVariant) -> SchemeVariant {
    match variant {
        SchemeVariant::Auto => {
            let chroma = Hct::from_int(source_argb).chroma();
            if chroma < 6.0 {
                SchemeVariant::Monochrome
            } else if chroma < 20.0 {
                SchemeVariant::Neutral
            } else if chroma >= 70.0 {
                SchemeVariant::Expressive
            } else {
                SchemeVariant::TonalSpot
            }
        }
        explicit => explicit,
    }
}

pub fn generate_m3_palettes(
    source_argb: u32,
    variant: SchemeVariant,
) -> (HashMap<String, String>, HashMap<String, String>) {
    use mcu_material_color::{
        SchemeContent, SchemeExpressive, SchemeFidelity, SchemeFruitSalad, SchemeMonochrome,
        SchemeNeutral, SchemeRainbow, SchemeTonalSpot,
    };

    let hct = Hct::from_int(source_argb);
    let variant = resolve_variant(source_argb, variant);

    fn build_palette(scheme: &mcu_material_color::DynamicScheme) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let tokens: &[(&str, u32)] = &[
            (
                "surface",
                mcu_material_color::MaterialDynamicColors::surface().get_argb(scheme),
            ),
            (
                "on_surface",
                mcu_material_color::MaterialDynamicColors::on_surface().get_argb(scheme),
            ),
            (
                "surface_dim",
                mcu_material_color::MaterialDynamicColors::surface_dim().get_argb(scheme),
            ),
            (
                "surface_bright",
                mcu_material_color::MaterialDynamicColors::surface_bright().get_argb(scheme),
            ),
            (
                "surface_container_lowest",
                mcu_material_color::MaterialDynamicColors::surface_container_lowest()
                    .get_argb(scheme),
            ),
            (
                "surface_container_low",
                mcu_material_color::MaterialDynamicColors::surface_container_low().get_argb(scheme),
            ),
            (
                "surface_container",
                mcu_material_color::MaterialDynamicColors::surface_container().get_argb(scheme),
            ),
            (
                "surface_container_high",
                mcu_material_color::MaterialDynamicColors::surface_container_high()
                    .get_argb(scheme),
            ),
            (
                "surface_container_highest",
                mcu_material_color::MaterialDynamicColors::surface_container_highest()
                    .get_argb(scheme),
            ),
            (
                "surface_variant",
                mcu_material_color::MaterialDynamicColors::surface_variant().get_argb(scheme),
            ),
            (
                "on_surface_variant",
                mcu_material_color::MaterialDynamicColors::on_surface_variant().get_argb(scheme),
            ),
            (
                "inverse_surface",
                mcu_material_color::MaterialDynamicColors::inverse_surface().get_argb(scheme),
            ),
            (
                "inverse_on_surface",
                mcu_material_color::MaterialDynamicColors::inverse_on_surface().get_argb(scheme),
            ),
            (
                "outline",
                mcu_material_color::MaterialDynamicColors::outline().get_argb(scheme),
            ),
            (
                "outline_variant",
                mcu_material_color::MaterialDynamicColors::outline_variant().get_argb(scheme),
            ),
            (
                "shadow",
                mcu_material_color::MaterialDynamicColors::shadow().get_argb(scheme),
            ),
            (
                "scrim",
                mcu_material_color::MaterialDynamicColors::scrim().get_argb(scheme),
            ),
            (
                "surface_tint",
                mcu_material_color::MaterialDynamicColors::surface_tint().get_argb(scheme),
            ),
            (
                "primary",
                mcu_material_color::MaterialDynamicColors::primary().get_argb(scheme),
            ),
            (
                "on_primary",
                mcu_material_color::MaterialDynamicColors::on_primary().get_argb(scheme),
            ),
            (
                "primary_container",
                mcu_material_color::MaterialDynamicColors::primary_container().get_argb(scheme),
            ),
            (
                "on_primary_container",
                mcu_material_color::MaterialDynamicColors::on_primary_container().get_argb(scheme),
            ),
            (
                "inverse_primary",
                mcu_material_color::MaterialDynamicColors::inverse_primary().get_argb(scheme),
            ),
            (
                "primary_fixed",
                mcu_material_color::MaterialDynamicColors::primary_fixed().get_argb(scheme),
            ),
            (
                "primary_fixed_dim",
                mcu_material_color::MaterialDynamicColors::primary_fixed_dim().get_argb(scheme),
            ),
            (
                "on_primary_fixed",
                mcu_material_color::MaterialDynamicColors::on_primary_fixed().get_argb(scheme),
            ),
            (
                "on_primary_fixed_variant",
                mcu_material_color::MaterialDynamicColors::on_primary_fixed_variant()
                    .get_argb(scheme),
            ),
            (
                "secondary",
                mcu_material_color::MaterialDynamicColors::secondary().get_argb(scheme),
            ),
            (
                "on_secondary",
                mcu_material_color::MaterialDynamicColors::on_secondary().get_argb(scheme),
            ),
            (
                "secondary_container",
                mcu_material_color::MaterialDynamicColors::secondary_container().get_argb(scheme),
            ),
            (
                "on_secondary_container",
                mcu_material_color::MaterialDynamicColors::on_secondary_container()
                    .get_argb(scheme),
            ),
            (
                "secondary_fixed",
                mcu_material_color::MaterialDynamicColors::secondary_fixed().get_argb(scheme),
            ),
            (
                "secondary_fixed_dim",
                mcu_material_color::MaterialDynamicColors::secondary_fixed_dim().get_argb(scheme),
            ),
            (
                "on_secondary_fixed",
                mcu_material_color::MaterialDynamicColors::on_secondary_fixed().get_argb(scheme),
            ),
            (
                "on_secondary_fixed_variant",
                mcu_material_color::MaterialDynamicColors::on_secondary_fixed_variant()
                    .get_argb(scheme),
            ),
            (
                "tertiary",
                mcu_material_color::MaterialDynamicColors::tertiary().get_argb(scheme),
            ),
            (
                "on_tertiary",
                mcu_material_color::MaterialDynamicColors::on_tertiary().get_argb(scheme),
            ),
            (
                "tertiary_container",
                mcu_material_color::MaterialDynamicColors::tertiary_container().get_argb(scheme),
            ),
            (
                "on_tertiary_container",
                mcu_material_color::MaterialDynamicColors::on_tertiary_container().get_argb(scheme),
            ),
            (
                "tertiary_fixed",
                mcu_material_color::MaterialDynamicColors::tertiary_fixed().get_argb(scheme),
            ),
            (
                "tertiary_fixed_dim",
                mcu_material_color::MaterialDynamicColors::tertiary_fixed_dim().get_argb(scheme),
            ),
            (
                "on_tertiary_fixed",
                mcu_material_color::MaterialDynamicColors::on_tertiary_fixed().get_argb(scheme),
            ),
            (
                "on_tertiary_fixed_variant",
                mcu_material_color::MaterialDynamicColors::on_tertiary_fixed_variant()
                    .get_argb(scheme),
            ),
            (
                "error",
                mcu_material_color::MaterialDynamicColors::error().get_argb(scheme),
            ),
            (
                "on_error",
                mcu_material_color::MaterialDynamicColors::on_error().get_argb(scheme),
            ),
            (
                "error_container",
                mcu_material_color::MaterialDynamicColors::error_container().get_argb(scheme),
            ),
            (
                "on_error_container",
                mcu_material_color::MaterialDynamicColors::on_error_container().get_argb(scheme),
            ),
        ];
        for (name, argb) in tokens {
            map.insert(name.to_string(), argb_to_hex(*argb));
        }
        map
    }

    macro_rules! gen_pair {
        ($scheme_ty:ident) => {{
            let light = $scheme_ty::new(hct, false, 0.0);
            let dark = $scheme_ty::new(hct, true, 0.0);
            (build_palette(&light), build_palette(&dark))
        }};
    }

    match variant {
        SchemeVariant::TonalSpot => gen_pair!(SchemeTonalSpot),
        SchemeVariant::Content => gen_pair!(SchemeContent),
        SchemeVariant::Expressive => gen_pair!(SchemeExpressive),
        SchemeVariant::Fidelity => gen_pair!(SchemeFidelity),
        SchemeVariant::FruitSalad => gen_pair!(SchemeFruitSalad),
        SchemeVariant::Monochrome => gen_pair!(SchemeMonochrome),
        SchemeVariant::Neutral => gen_pair!(SchemeNeutral),
        SchemeVariant::Rainbow => gen_pair!(SchemeRainbow),
        SchemeVariant::Auto => unreachable!("Auto resolved by resolve_variant"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeCommand {
    SetMode(ThemeMode),
    ToggleMode,
    SetColorSource(ColorSource),
    SetSchemeVariant(SchemeVariant),
    /// Remember the user's custom seed choice and apply it while
    /// [`ColorSource::Custom`] is the active source. The seed is persisted in
    /// `ThemeState.custom_seed`, so switching back to `Custom` re-applies it.
    SetCustomSeed(u32),
    /// Transiently apply the current external source's seed (e.g. from a
    /// wallpaper) while [`ColorSource::Wallpaper`] is the active source. Unlike
    /// [`Self::SetCustomSeed`] this never stores the seed — the daemon owns
    /// remembering it, since it is tied to an on-disk source (ADR-0002). The
    /// core crate never knows where the seed came from; it only applies it for
    /// the source that consumes external seeds, and is a no-op otherwise.
    SetSeed(u32),
}

fn regenerate_palette(state: &mut ThemeState, seed: u32, variant: SchemeVariant, timestamp: &str) {
    apply_palette(state, seed, resolve_variant(seed, variant), timestamp);
}

/// Write the palettes derived from a seed under a concrete variant, and the
/// bookkeeping that accompanies a palette change (source, resolution, and
/// generation timestamp). The caller owns the revision/`updated_at` bump.
fn apply_palette(state: &mut ThemeState, seed: u32, effective: SchemeVariant, timestamp: &str) {
    state.source_argb = seed;
    state.resolved_variant = effective;
    let (light, dark) = generate_m3_palettes(seed, effective);
    state.light = light;
    state.dark = dark;
    state.palette_generated_at = timestamp.to_string();
}

/// Materialize a seed produced by the active external source together with the
/// concrete variant its producer resolved for it. When the stored selection is
/// [`SchemeVariant::Auto`], the passed-in resolution wins; an explicit pin
/// overrides it. The stored selection is left untouched, so the next external
/// seed re-derives its own resolution. If no concrete resolution is supplied
/// (`Auto`), falls back to seed-chroma resolution.
///
/// Core does not know where a seed came from or how its resolution was decided
/// (ADR-0002); the caller owns that. This only materializes both values.
/// Returns whether the state changed.
pub fn materialize_seed_with_variant(
    state: &mut ThemeState,
    seed: u32,
    resolved_variant: SchemeVariant,
    timestamp: &str,
) -> bool {
    if state.color_source != ColorSource::Wallpaper {
        return false;
    }

    let effective = match state.scheme_variant {
        SchemeVariant::Auto if resolved_variant != SchemeVariant::Auto => resolved_variant,
        SchemeVariant::Auto => resolve_variant(seed, SchemeVariant::Auto),
        explicit => explicit,
    };

    if state.source_argb == seed && state.resolved_variant == effective {
        return false;
    }

    apply_palette(state, seed, effective, timestamp);
    state.revision += 1;
    state.updated_at = timestamp.to_string();

    true
}

/// Apply a pure `ThemeCommand` transition, returning whether the state changed.
///
/// The core crate is pure computation with zero I/O (ADR-0002), so `reduce`
/// produces no effects: a `false` return means the command was a no-op and no
/// `revision`/`updated_at` bump was recorded. This is the seam `shilpo-theme-daemon`
/// uses to apply wallpaper seeds and mode toggles without mocking system
/// dependencies.
pub fn reduce(state: &mut ThemeState, command: ThemeCommand, timestamp: &str) -> bool {
    let mut changed = false;

    match command {
        ThemeCommand::SetMode(mode) => {
            if state.selected_mode != mode {
                state.selected_mode = mode;
                changed = true;
            }
            match mode {
                ThemeMode::System => {}
                ThemeMode::Light | ThemeMode::Dark => {
                    if state.resolved_mode != mode {
                        state.resolved_mode = mode;
                        changed = true;
                    }
                }
            }
        }
        ThemeCommand::ToggleMode => {
            let next_mode = if state.resolved_mode == ThemeMode::Dark {
                ThemeMode::Light
            } else {
                ThemeMode::Dark
            };
            state.selected_mode = next_mode;
            state.resolved_mode = next_mode;
            changed = true;
        }
        ThemeCommand::SetColorSource(source) => {
            if state.color_source != source {
                state.color_source = source;
                changed = true;

                if source == ColorSource::Custom
                    && let Some(seed) = state.custom_seed.filter(|&seed| seed != state.source_argb)
                {
                    let variant = state.scheme_variant;
                    regenerate_palette(state, seed, variant, timestamp);
                }
            }
        }
        ThemeCommand::SetSchemeVariant(variant) => {
            if state.scheme_variant != variant {
                let seed = state.source_argb;
                state.scheme_variant = variant;
                changed = true;
                regenerate_palette(state, seed, variant, timestamp);
            }
        }
        ThemeCommand::SetCustomSeed(seed) => {
            if state.custom_seed != Some(seed) {
                state.custom_seed = Some(seed);
                changed = true;
            }
            if state.color_source == ColorSource::Custom && state.source_argb != seed {
                let variant = state.scheme_variant;
                changed = true;
                regenerate_palette(state, seed, variant, timestamp);
            }
        }
        ThemeCommand::SetSeed(seed) => {
            if state.color_source == ColorSource::Wallpaper && state.source_argb != seed {
                let variant = state.scheme_variant;
                changed = true;
                regenerate_palette(state, seed, variant, timestamp);
            }
        }
    }

    if changed {
        state.revision += 1;
        state.updated_at = timestamp.to_string();
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TIMESTAMP: &str = "2026-08-06T20:00:00Z";

    #[test]
    fn test_theme_mode_serde_validation() {
        let json_light = serde_json::to_string(&ThemeMode::Light).unwrap();
        assert_eq!(json_light, "\"light\"");
        let mode: ThemeMode = serde_json::from_str("\"light\"").unwrap();
        assert_eq!(mode, ThemeMode::Light);

        let mode_dark: ThemeMode = serde_json::from_str("\"dark\"").unwrap();
        assert_eq!(mode_dark, ThemeMode::Dark);

        let mode_sys: ThemeMode = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(mode_sys, ThemeMode::System);

        let err = serde_json::from_str::<ThemeMode>("\"auto\"");
        assert!(err.is_err());
    }

    #[test]
    fn test_reducer_system_preservation_and_fixed_mode() {
        let mut state = ThemeState::default();
        assert_eq!(state.selected_mode, ThemeMode::System);
        assert_eq!(state.resolved_mode, ThemeMode::Light);
        assert_eq!(state.updated_at, DEFAULT_TIMESTAMP);

        // Fixed Dark mode command
        let changed = reduce(
            &mut state,
            ThemeCommand::SetMode(ThemeMode::Dark),
            TEST_TIMESTAMP,
        );
        assert!(changed);
        assert_eq!(state.selected_mode, ThemeMode::Dark);
        assert_eq!(state.resolved_mode, ThemeMode::Dark);
        assert_eq!(state.updated_at, TEST_TIMESTAMP);
    }

    #[test]
    fn test_all_scheme_variants_produce_distinct_colors() {
        let seed = 0xffe63946;
        let (light_tonal, _) = generate_m3_palettes(seed, SchemeVariant::TonalSpot);
        let (light_expressive, _) = generate_m3_palettes(seed, SchemeVariant::Expressive);
        let (light_fruit, _) = generate_m3_palettes(seed, SchemeVariant::FruitSalad);
        let (light_mono, _) = generate_m3_palettes(seed, SchemeVariant::Monochrome);

        assert_ne!(light_tonal.get("primary"), light_expressive.get("primary"));
        assert_ne!(
            light_tonal.get("primary_container"),
            light_fruit.get("primary_container")
        );
        assert_ne!(light_tonal.get("secondary"), light_mono.get("secondary"));
    }

    #[test]
    fn test_resolve_variant_matches_generated_auto_palette() {
        assert_eq!(
            resolve_variant(0xff000000, SchemeVariant::Auto),
            SchemeVariant::Monochrome
        );
        assert_eq!(
            resolve_variant(0xffa08f7f, SchemeVariant::Auto),
            SchemeVariant::Neutral
        );
        assert_eq!(
            resolve_variant(0xffe63946, SchemeVariant::Auto),
            SchemeVariant::Expressive
        );
        assert_eq!(
            resolve_variant(0xff006c4c, SchemeVariant::Auto),
            SchemeVariant::TonalSpot
        );
        let explicit = resolve_variant(0xffe63946, SchemeVariant::Expressive);
        assert_eq!(explicit, SchemeVariant::Expressive);
    }

    #[test]
    fn test_toggle_mode() {
        let mut state = ThemeState {
            resolved_mode: ThemeMode::Light,
            ..Default::default()
        };

        let changed = reduce(&mut state, ThemeCommand::ToggleMode, TEST_TIMESTAMP);
        assert!(changed);
        assert_eq!(state.selected_mode, ThemeMode::Dark);
        assert_eq!(state.resolved_mode, ThemeMode::Dark);
        assert_eq!(state.updated_at, TEST_TIMESTAMP);

        let changed = reduce(&mut state, ThemeCommand::ToggleMode, TEST_TIMESTAMP);
        assert!(changed);
        assert_eq!(state.selected_mode, ThemeMode::Light);
        assert_eq!(state.resolved_mode, ThemeMode::Light);
    }

    #[test]
    fn test_color_source_and_seed_changes() {
        let mut state = ThemeState::default();
        let seed = 0xff123456;

        let _ = reduce(
            &mut state,
            ThemeCommand::SetCustomSeed(seed),
            TEST_TIMESTAMP,
        );
        assert_eq!(state.custom_seed, Some(seed));

        let _ = reduce(
            &mut state,
            ThemeCommand::SetColorSource(ColorSource::Custom),
            TEST_TIMESTAMP,
        );
        assert_eq!(state.color_source, ColorSource::Custom);
        assert_eq!(state.source_argb, seed);

        assert!(state.light.contains_key("primary"));
        assert!(state.dark.contains_key("primary"));
        assert_eq!(state.palette_generated_at, TEST_TIMESTAMP);
    }

    #[test]
    fn test_wallpaper_seed_change() {
        let mut state = ThemeState::default();
        assert_eq!(state.color_source, ColorSource::Wallpaper);
        let seed = 0xffab12cd;

        let changed = reduce(
            &mut state,
            ThemeCommand::SetSeed(seed),
            TEST_TIMESTAMP,
        );
        assert!(changed);
        assert_eq!(state.source_argb, seed);
        assert_eq!(state.palette_generated_at, TEST_TIMESTAMP);
    }

    #[test]
    fn test_wallpaper_seed_is_ignored_for_custom_source() {
        let mut state = ThemeState {
            color_source: ColorSource::Custom,
            ..Default::default()
        };
        let revision = state.revision;
        let source_argb = state.source_argb;

        let changed = reduce(
            &mut state,
            ThemeCommand::SetSeed(0xffab12cd),
            TEST_TIMESTAMP,
        );
        assert!(!changed);
        assert_eq!(state.source_argb, source_argb);
        assert_eq!(state.revision, revision);
    }

    #[test]
    fn test_reduce_reports_noop_as_unchanged() {
        let mut state = ThemeState::default();
        let revision = state.revision;

        let changed = reduce(
            &mut state,
            ThemeCommand::SetMode(ThemeMode::System),
            TEST_TIMESTAMP,
        );
        assert!(!changed);
        assert_eq!(state.revision, revision);
    }

    #[test]
    fn test_reduce_is_deterministic_with_timestamp() {
        let mut state = ThemeState::new("2026-01-01T00:00:00Z");
        assert_eq!(state.updated_at, "2026-01-01T00:00:00Z");
        assert_eq!(state.palette_generated_at, "2026-01-01T00:00:00Z");

        let ts = "2026-08-06T12:34:56Z";
        reduce(&mut state, ThemeCommand::SetMode(ThemeMode::Dark), ts);
        assert_eq!(state.updated_at, ts);

        reduce(
            &mut state,
            ThemeCommand::SetSchemeVariant(SchemeVariant::Expressive),
            "2026-08-06T13:00:00Z",
        );
        assert_eq!(state.updated_at, "2026-08-06T13:00:00Z");
        assert_eq!(state.palette_generated_at, "2026-08-06T13:00:00Z");
    }

    #[test]
    fn test_materialize_seed_uses_injected_resolution_when_auto() {
        let mut state = ThemeState::default();
        assert_eq!(state.color_source, ColorSource::Wallpaper);
        assert_eq!(state.scheme_variant, SchemeVariant::Auto);

        let changed = materialize_seed_with_variant(
            &mut state,
            0xffdd11dd,
            SchemeVariant::Expressive,
            TEST_TIMESTAMP,
        );
        assert!(changed);
        assert_eq!(state.scheme_variant, SchemeVariant::Auto);
        assert_eq!(state.source_argb, 0xffdd11dd);
        assert_eq!(state.resolved_variant, SchemeVariant::Expressive);
        assert_eq!(state.palette_generated_at, TEST_TIMESTAMP);
        assert_eq!(state.updated_at, TEST_TIMESTAMP);

        let (expected_light, expected_dark) =
            generate_m3_palettes(0xffdd11dd, SchemeVariant::Expressive);
        assert_eq!(state.light, expected_light);
        assert_eq!(state.dark, expected_dark);
    }

    #[test]
    fn test_materialize_seed_honors_explicit_pin() {
        let mut state = ThemeState {
            scheme_variant: SchemeVariant::TonalSpot,
            ..Default::default()
        };

        let changed = materialize_seed_with_variant(
            &mut state,
            0xffdd11dd,
            SchemeVariant::Expressive,
            TEST_TIMESTAMP,
        );
        assert!(changed);
        assert_eq!(state.scheme_variant, SchemeVariant::TonalSpot);
        assert_eq!(state.resolved_variant, SchemeVariant::TonalSpot);
    }

    #[test]
    fn test_materialize_seed_falls_back_to_chroma_for_auto_resolution() {
        let mut state = ThemeState::default();

        let changed = materialize_seed_with_variant(
            &mut state,
            0xffdd11dd,
            SchemeVariant::Auto,
            TEST_TIMESTAMP,
        );
        assert!(changed);
        assert_eq!(
            state.resolved_variant,
            resolve_variant(0xffdd11dd, SchemeVariant::Auto)
        );
    }

    #[test]
    fn test_materialize_seed_is_noop_for_custom_source() {
        let mut state = ThemeState {
            color_source: ColorSource::Custom,
            ..Default::default()
        };
        let revision = state.revision;

        let changed = materialize_seed_with_variant(
            &mut state,
            0xffdd11dd,
            SchemeVariant::Expressive,
            TEST_TIMESTAMP,
        );
        assert!(!changed);
        assert_eq!(state.revision, revision);
        assert_eq!(state.source_argb, DEFAULT_SOURCE_ARGB);
    }

    #[test]
    fn test_materialize_seed_same_seed_and_resolution_is_noop() {
        let mut state = ThemeState::default();
        let _ = materialize_seed_with_variant(
            &mut state,
            0xffdd11dd,
            SchemeVariant::Expressive,
            TEST_TIMESTAMP,
        );
        let revision = state.revision;

        let changed = materialize_seed_with_variant(
            &mut state,
            0xffdd11dd,
            SchemeVariant::Expressive,
            TEST_TIMESTAMP,
        );
        assert!(!changed);
        assert_eq!(state.revision, revision);
    }
}
