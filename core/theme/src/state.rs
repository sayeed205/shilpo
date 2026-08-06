use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

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
    pub custom_seed: Option<u32>,
    pub wallpaper_path: Option<PathBuf>,
    pub wallpaper_seed: Option<u32>,
    pub wallpaper_dir: PathBuf,
    pub source_argb: u32,
    pub light: HashMap<String, String>,
    pub dark: HashMap<String, String>,
    pub updated_at: String,
    pub palette_generated_at: String,
}

impl ThemeState {
    pub fn palette_algorithm(&self) -> String {
        format!(
            "Material3-{}",
            self.scheme_variant.display_name().replace(' ', "")
        )
    }
}

impl Default for ThemeState {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        let (light, dark) = generate_m3_palettes(DEFAULT_SOURCE_ARGB, SchemeVariant::Auto);
        Self {
            revision: 1,
            selected_mode: ThemeMode::System,
            resolved_mode: ThemeMode::Light,
            color_source: ColorSource::Wallpaper,
            scheme_variant: SchemeVariant::Auto,
            custom_seed: None,
            wallpaper_path: None,
            wallpaper_seed: None,
            wallpaper_dir: PathBuf::from("~/Pictures/Wallpapers"),
            source_argb: DEFAULT_SOURCE_ARGB,
            light,
            dark,
            updated_at: now.clone(),
            palette_generated_at: now,
        }
    }
}

pub fn argb_to_hex(argb: u32) -> String {
    format!("#{:06X}", argb & 0x00FF_FFFF)
}

pub fn generate_m3_palettes(
    source_argb: u32,
    variant: SchemeVariant,
) -> (HashMap<String, String>, HashMap<String, String>) {
    use mcu_material_color::{
        Hct, SchemeContent, SchemeExpressive, SchemeFidelity, SchemeFruitSalad, SchemeMonochrome,
        SchemeNeutral, SchemeRainbow, SchemeTonalSpot,
    };

    let hct = Hct::from_int(source_argb);

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
        SchemeVariant::Auto => {
            let chroma = hct.chroma();
            if chroma < 6.0 {
                gen_pair!(SchemeMonochrome)
            } else if chroma < 20.0 {
                gen_pair!(SchemeNeutral)
            } else if chroma >= 70.0 {
                gen_pair!(SchemeExpressive)
            } else {
                gen_pair!(SchemeTonalSpot)
            }
        }
        SchemeVariant::TonalSpot => gen_pair!(SchemeTonalSpot),
        SchemeVariant::Content => gen_pair!(SchemeContent),
        SchemeVariant::Expressive => gen_pair!(SchemeExpressive),
        SchemeVariant::Fidelity => gen_pair!(SchemeFidelity),
        SchemeVariant::FruitSalad => gen_pair!(SchemeFruitSalad),
        SchemeVariant::Monochrome => gen_pair!(SchemeMonochrome),
        SchemeVariant::Neutral => gen_pair!(SchemeNeutral),
        SchemeVariant::Rainbow => gen_pair!(SchemeRainbow),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeCommand {
    SetMode(ThemeMode),
    ToggleMode,
    SetColorSource(ColorSource),
    SetSchemeVariant(SchemeVariant),
    SetCustomSeed(u32),
    SetWallpaperDirectory(PathBuf),
    SetWallpaper { path: PathBuf, seed: u32 },
    PortalAppearanceChanged(Option<ThemeMode>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideEffect {
    DispatchDesktopAdapter(ThemeMode),
}

pub fn reduce(state: &mut ThemeState, command: ThemeCommand) -> Vec<SideEffect> {
    let mut effects = Vec::new();
    let mut changed = false;

    match command {
        ThemeCommand::SetMode(mode) => {
            if state.selected_mode != mode {
                state.selected_mode = mode;
                changed = true;
            }
            match mode {
                ThemeMode::System => {
                    // System performs no desktop adapter dispatch
                }
                ThemeMode::Light | ThemeMode::Dark => {
                    if state.resolved_mode != mode {
                        state.resolved_mode = mode;
                        changed = true;
                    }
                    effects.push(SideEffect::DispatchDesktopAdapter(mode));
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
            effects.push(SideEffect::DispatchDesktopAdapter(next_mode));
        }
        ThemeCommand::SetColorSource(source) => {
            let target_seed = match source {
                ColorSource::Custom => state.custom_seed,
                ColorSource::Wallpaper => state.wallpaper_seed,
            };
            if target_seed.is_some() && state.color_source != source {
                state.color_source = source;
                changed = true;

                if let Some(seed) = target_seed.filter(|&seed| seed != state.source_argb) {
                    state.source_argb = seed;
                    let (light, dark) = generate_m3_palettes(seed, state.scheme_variant);
                    state.light = light;
                    state.dark = dark;
                    state.palette_generated_at = Utc::now().to_rfc3339();
                }
            }
        }
        ThemeCommand::SetSchemeVariant(variant) => {
            if state.scheme_variant != variant {
                state.scheme_variant = variant;
                changed = true;
                let (light, dark) = generate_m3_palettes(state.source_argb, variant);
                state.light = light;
                state.dark = dark;
                state.palette_generated_at = Utc::now().to_rfc3339();
            }
        }
        ThemeCommand::SetCustomSeed(seed) => {
            if state.custom_seed != Some(seed) {
                state.custom_seed = Some(seed);
                changed = true;
            }
            if state.color_source == ColorSource::Custom && state.source_argb != seed {
                state.source_argb = seed;
                let (light, dark) = generate_m3_palettes(seed, state.scheme_variant);
                state.light = light;
                state.dark = dark;
                state.palette_generated_at = Utc::now().to_rfc3339();
                changed = true;
            }
        }
        ThemeCommand::SetWallpaperDirectory(dir) => {
            if state.wallpaper_dir != dir {
                state.wallpaper_dir = dir;
                changed = true;
            }
        }
        ThemeCommand::SetWallpaper { path, seed } => {
            if state.wallpaper_path.as_ref() != Some(&path) {
                state.wallpaper_path = Some(path.clone());
                changed = true;
            }
            if state.wallpaper_seed != Some(seed) {
                state.wallpaper_seed = Some(seed);
                changed = true;
            }
            if state.color_source == ColorSource::Wallpaper && state.source_argb != seed {
                state.source_argb = seed;
                let (light, dark) = generate_m3_palettes(seed, state.scheme_variant);
                state.light = light;
                state.dark = dark;
                state.palette_generated_at = Utc::now().to_rfc3339();
                changed = true;
            }
        }
        ThemeCommand::PortalAppearanceChanged(portal_mode) => {
            if let Some(pm) = portal_mode {
                debug_assert!(pm != ThemeMode::System);
                if state.selected_mode == ThemeMode::System && state.resolved_mode != pm {
                    state.resolved_mode = pm;
                    changed = true;
                }
            }
        }
    }

    if changed {
        state.revision += 1;
        state.updated_at = Utc::now().to_rfc3339();
    }

    effects
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_reducer_system_preservation_and_fixed_mode_immunity() {
        let mut state = ThemeState::default();
        assert_eq!(state.selected_mode, ThemeMode::System);
        assert_eq!(state.resolved_mode, ThemeMode::Light);

        // Portal appearance changes to Dark while in System mode
        let effects = reduce(
            &mut state,
            ThemeCommand::PortalAppearanceChanged(Some(ThemeMode::Dark)),
        );
        assert!(effects.is_empty());
        assert_eq!(state.selected_mode, ThemeMode::System);
        assert_eq!(state.resolved_mode, ThemeMode::Dark);

        // Fixed Dark mode command
        let effects = reduce(&mut state, ThemeCommand::SetMode(ThemeMode::Dark));
        assert_eq!(
            effects,
            vec![SideEffect::DispatchDesktopAdapter(ThemeMode::Dark)]
        );
        assert_eq!(state.selected_mode, ThemeMode::Dark);
        assert_eq!(state.resolved_mode, ThemeMode::Dark);

        // Portal appearance changes to Light while in fixed Dark mode -> ignored!
        let effects = reduce(
            &mut state,
            ThemeCommand::PortalAppearanceChanged(Some(ThemeMode::Light)),
        );
        assert!(effects.is_empty());
        assert_eq!(state.selected_mode, ThemeMode::Dark);
        assert_eq!(state.resolved_mode, ThemeMode::Dark);
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
    fn test_portal_echo_and_no_preference() {
        let mut state = ThemeState::default();
        let initial_rev = state.revision;

        // Portal echo equal to current resolution
        let effects = reduce(
            &mut state,
            ThemeCommand::PortalAppearanceChanged(Some(ThemeMode::Light)),
        );
        assert!(effects.is_empty());
        assert_eq!(state.revision, initial_rev);

        // No preference
        let effects = reduce(&mut state, ThemeCommand::PortalAppearanceChanged(None));
        assert!(effects.is_empty());
        assert_eq!(state.revision, initial_rev);
        assert_eq!(state.resolved_mode, ThemeMode::Light);
    }

    #[test]
    fn test_toggle_mode() {
        let mut state = ThemeState {
            resolved_mode: ThemeMode::Light,
            ..Default::default()
        };

        let effects = reduce(&mut state, ThemeCommand::ToggleMode);
        assert_eq!(
            effects,
            vec![SideEffect::DispatchDesktopAdapter(ThemeMode::Dark)]
        );
        assert_eq!(state.selected_mode, ThemeMode::Dark);
        assert_eq!(state.resolved_mode, ThemeMode::Dark);

        let effects = reduce(&mut state, ThemeCommand::ToggleMode);
        assert_eq!(
            effects,
            vec![SideEffect::DispatchDesktopAdapter(ThemeMode::Light)]
        );
        assert_eq!(state.selected_mode, ThemeMode::Light);
        assert_eq!(state.resolved_mode, ThemeMode::Light);
    }

    #[test]
    fn test_color_source_and_seed_changes() {
        let mut state = ThemeState::default();
        let seed = 0xff123456;

        let _ = reduce(&mut state, ThemeCommand::SetCustomSeed(seed));
        assert_eq!(state.custom_seed, Some(seed));

        let _ = reduce(
            &mut state,
            ThemeCommand::SetColorSource(ColorSource::Custom),
        );
        assert_eq!(state.color_source, ColorSource::Custom);
        assert_eq!(state.source_argb, seed);

        assert!(state.light.contains_key("primary"));
        assert!(state.dark.contains_key("primary"));
    }
}
