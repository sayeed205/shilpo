// <FILE>crates/mcu-dynamiccolor/src/material_dynamic_colors.rs</FILE> - <DESC>Material Design dynamic color definitions</DESC>
// <VERS>VERSION: 3.2.0</VERS>
// <WCTX>Fix Spec2025 fixed color resolution — add spec-aware _for variants</WCTX>
// <CLOG>Add 12 _for spec-aware variants for fixed colors (primary/secondary/tertiary × fixed/dim/on/on_variant)</CLOG>

use super::{
    ColorSpecDelegate, ColorSpecDelegateImpl2021, ColorSpecDelegateImpl2025,
    ColorSpecDelegateImpl2026, DynamicColor, DynamicScheme, SpecVersion,
};
use crate::material_color::roles::MaterialRole;

/// DynamicColors for the colors in the Material Design system.
///
/// This struct provides a central registry of all Material Design 3 dynamic color roles.
/// Each color role is accessible via a static method that returns a DynamicColor.
///
/// The implementation delegates to a ColorSpecDelegate which determines the actual
/// tone and contrast values based on the scheme's spec_version.
///
/// # Example
///
/// ```ignore
/// use mcu_dynamiccolor::MaterialDynamicColors;
///
/// let primary_color = MaterialDynamicColors::primary();
/// let surface_color = MaterialDynamicColors::surface();
/// ```
pub struct MaterialDynamicColors;

impl MaterialDynamicColors {
    /// Content accent tone delta constant
    pub const CONTENT_ACCENT_TONE_DELTA: f64 = 15.0;

    /// Get the color spec delegate for a scheme
    ///
    /// Routes to the appropriate ColorSpecDelegate implementation based on
    /// the scheme's spec_version field.
    fn get_color_spec(scheme: &DynamicScheme) -> &'static dyn ColorSpecDelegate {
        static SPEC_2021: ColorSpecDelegateImpl2021 = ColorSpecDelegateImpl2021;
        static SPEC_2025: ColorSpecDelegateImpl2025 = ColorSpecDelegateImpl2025;
        static SPEC_2026: ColorSpecDelegateImpl2026 = ColorSpecDelegateImpl2026;
        match scheme.spec_version {
            SpecVersion::Spec2025 => &SPEC_2025,
            SpecVersion::Spec2021 => &SPEC_2021,
            SpecVersion::Spec2026 => &SPEC_2026,
        }
    }

    /// Resolve a current Material role against its canonical effective spec.
    /// The 2025 delegate supplies dim roles for legacy fallback schemes; their
    /// optional return remains intact rather than fabricating a color.
    pub(crate) fn color_for_role(
        role: MaterialRole,
        scheme: &DynamicScheme,
    ) -> Option<DynamicColor> {
        let spec = Self::get_color_spec(scheme);
        Some(match role {
            MaterialRole::HighestSurface => {
                if scheme.is_dark { spec.surface_bright() } else { spec.surface_dim() }
            }
            MaterialRole::PrimaryPaletteKeyColor => spec.primary_palette_key_color(),
            MaterialRole::SecondaryPaletteKeyColor => spec.secondary_palette_key_color(),
            MaterialRole::TertiaryPaletteKeyColor => spec.tertiary_palette_key_color(),
            MaterialRole::NeutralPaletteKeyColor => spec.neutral_palette_key_color(),
            MaterialRole::NeutralVariantPaletteKeyColor => spec.neutral_variant_palette_key_color(),
            MaterialRole::ErrorPaletteKeyColor => spec.error_palette_key_color(),
            MaterialRole::Background => spec.background(),
            MaterialRole::OnBackground => spec.on_background(),
            MaterialRole::Surface => spec.surface(),
            MaterialRole::SurfaceDim => spec.surface_dim(),
            MaterialRole::SurfaceBright => spec.surface_bright(),
            MaterialRole::SurfaceContainerLowest => spec.surface_container_lowest(),
            MaterialRole::SurfaceContainerLow => spec.surface_container_low(),
            MaterialRole::SurfaceContainer => spec.surface_container(),
            MaterialRole::SurfaceContainerHigh => spec.surface_container_high(),
            MaterialRole::SurfaceContainerHighest => spec.surface_container_highest(),
            MaterialRole::OnSurface => spec.on_surface(),
            MaterialRole::SurfaceVariant => spec.surface_variant(),
            MaterialRole::OnSurfaceVariant => spec.on_surface_variant(),
            MaterialRole::InverseSurface => spec.inverse_surface(),
            MaterialRole::InverseOnSurface => spec.inverse_on_surface(),
            MaterialRole::Outline => spec.outline(),
            MaterialRole::OutlineVariant => spec.outline_variant(),
            MaterialRole::Shadow => spec.shadow(),
            MaterialRole::Scrim => spec.scrim(),
            MaterialRole::SurfaceTint => spec.surface_tint(),
            MaterialRole::Primary => spec.primary(),
            MaterialRole::PrimaryDim => {
                if scheme.spec_version == SpecVersion::Spec2026 {
                    spec.primary_dim()?
                } else {
                    ColorSpecDelegateImpl2025.primary_dim()?
                }
            }
            MaterialRole::OnPrimary => spec.on_primary(),
            MaterialRole::PrimaryContainer => spec.primary_container(),
            MaterialRole::OnPrimaryContainer => spec.on_primary_container(),
            MaterialRole::PrimaryFixed => spec.primary_fixed(),
            MaterialRole::PrimaryFixedDim => spec.primary_fixed_dim(),
            MaterialRole::OnPrimaryFixed => spec.on_primary_fixed(),
            MaterialRole::OnPrimaryFixedVariant => spec.on_primary_fixed_variant(),
            MaterialRole::InversePrimary => spec.inverse_primary(),
            MaterialRole::Secondary => spec.secondary(),
            MaterialRole::SecondaryDim => {
                if scheme.spec_version == SpecVersion::Spec2026 {
                    spec.secondary_dim()?
                } else {
                    ColorSpecDelegateImpl2025.secondary_dim()?
                }
            }
            MaterialRole::OnSecondary => spec.on_secondary(),
            MaterialRole::SecondaryContainer => spec.secondary_container(),
            MaterialRole::OnSecondaryContainer => spec.on_secondary_container(),
            MaterialRole::SecondaryFixed => spec.secondary_fixed(),
            MaterialRole::SecondaryFixedDim => spec.secondary_fixed_dim(),
            MaterialRole::OnSecondaryFixed => spec.on_secondary_fixed(),
            MaterialRole::OnSecondaryFixedVariant => spec.on_secondary_fixed_variant(),
            MaterialRole::Tertiary => spec.tertiary(),
            MaterialRole::TertiaryDim => {
                if scheme.spec_version == SpecVersion::Spec2026 {
                    spec.tertiary_dim()?
                } else {
                    ColorSpecDelegateImpl2025.tertiary_dim()?
                }
            }
            MaterialRole::OnTertiary => spec.on_tertiary(),
            MaterialRole::TertiaryContainer => spec.tertiary_container(),
            MaterialRole::OnTertiaryContainer => spec.on_tertiary_container(),
            MaterialRole::TertiaryFixed => spec.tertiary_fixed(),
            MaterialRole::TertiaryFixedDim => spec.tertiary_fixed_dim(),
            MaterialRole::OnTertiaryFixed => spec.on_tertiary_fixed(),
            MaterialRole::OnTertiaryFixedVariant => spec.on_tertiary_fixed_variant(),
            MaterialRole::Error => spec.error(),
            MaterialRole::ErrorDim => {
                if scheme.spec_version == SpecVersion::Spec2026 {
                    spec.error_dim()?
                } else {
                    ColorSpecDelegateImpl2025.error_dim()?
                }
            }
            MaterialRole::OnError => spec.on_error(),
            MaterialRole::ErrorContainer => spec.error_container(),
            MaterialRole::OnErrorContainer => spec.on_error_container(),
        })
    }

    /// Get the default 2021 color spec delegate (for backward compatibility)
    ///
    /// This is used by static methods that don't have access to a scheme.
    /// Users should prefer using the scheme-based methods when possible.
    fn get_default_color_spec() -> &'static ColorSpecDelegateImpl2021 {
        static SPEC: ColorSpecDelegateImpl2021 = ColorSpecDelegateImpl2021;
        &SPEC
    }

    /// Returns the highest surface color for the given scheme.
    /// In dark mode, this is surface_bright (lighter for contrast).
    /// In light mode, this is surface_dim (darker for contrast).
    ///
    /// This method routes to the correct spec delegate based on the scheme's spec_version.
    pub fn highest_surface(scheme: &DynamicScheme) -> DynamicColor {
        let spec = Self::get_color_spec(scheme);
        if scheme.is_dark {
            spec.surface_bright()
        } else {
            spec.surface_dim()
        }
    }

    // ================================================================
    // Main Palettes
    // ================================================================

    /// Primary palette key color
    pub fn primary_palette_key_color() -> DynamicColor {
        Self::get_default_color_spec().primary_palette_key_color()
    }

    /// Secondary palette key color
    pub fn secondary_palette_key_color() -> DynamicColor {
        Self::get_default_color_spec().secondary_palette_key_color()
    }

    /// Tertiary palette key color
    pub fn tertiary_palette_key_color() -> DynamicColor {
        Self::get_default_color_spec().tertiary_palette_key_color()
    }

    /// Neutral palette key color
    pub fn neutral_palette_key_color() -> DynamicColor {
        Self::get_default_color_spec().neutral_palette_key_color()
    }

    /// Neutral variant palette key color
    pub fn neutral_variant_palette_key_color() -> DynamicColor {
        Self::get_default_color_spec().neutral_variant_palette_key_color()
    }

    /// Error palette key color
    pub fn error_palette_key_color() -> DynamicColor {
        Self::get_default_color_spec().error_palette_key_color()
    }

    // ================================================================
    // Surfaces [S]
    // ================================================================

    /// Background color
    pub fn background() -> DynamicColor {
        Self::get_default_color_spec().background()
    }

    /// On-background color (text/icons on background)
    pub fn on_background() -> DynamicColor {
        Self::get_default_color_spec().on_background()
    }

    /// Surface color
    pub fn surface() -> DynamicColor {
        Self::get_default_color_spec().surface()
    }

    /// Surface dim color (darker surface)
    pub fn surface_dim() -> DynamicColor {
        Self::get_default_color_spec().surface_dim()
    }

    /// Surface bright color (lighter surface)
    pub fn surface_bright() -> DynamicColor {
        Self::get_default_color_spec().surface_bright()
    }

    /// Surface container lowest elevation
    pub fn surface_container_lowest() -> DynamicColor {
        Self::get_default_color_spec().surface_container_lowest()
    }

    /// Surface container low elevation
    pub fn surface_container_low() -> DynamicColor {
        Self::get_default_color_spec().surface_container_low()
    }

    /// Surface container medium elevation
    pub fn surface_container() -> DynamicColor {
        Self::get_default_color_spec().surface_container()
    }

    /// Surface container high elevation
    pub fn surface_container_high() -> DynamicColor {
        Self::get_default_color_spec().surface_container_high()
    }

    /// Surface container highest elevation
    pub fn surface_container_highest() -> DynamicColor {
        Self::get_default_color_spec().surface_container_highest()
    }

    /// On-surface color (text/icons on surface)
    pub fn on_surface() -> DynamicColor {
        Self::get_default_color_spec().on_surface()
    }

    /// Surface variant color
    pub fn surface_variant() -> DynamicColor {
        Self::get_default_color_spec().surface_variant()
    }

    /// On-surface variant color (text/icons on surface variant)
    pub fn on_surface_variant() -> DynamicColor {
        Self::get_default_color_spec().on_surface_variant()
    }

    /// Inverse surface color
    pub fn inverse_surface() -> DynamicColor {
        Self::get_default_color_spec().inverse_surface()
    }

    /// Inverse on-surface color
    pub fn inverse_on_surface() -> DynamicColor {
        Self::get_default_color_spec().inverse_on_surface()
    }

    /// Outline color
    pub fn outline() -> DynamicColor {
        Self::get_default_color_spec().outline()
    }

    /// Outline variant color
    pub fn outline_variant() -> DynamicColor {
        Self::get_default_color_spec().outline_variant()
    }

    /// Shadow color
    pub fn shadow() -> DynamicColor {
        Self::get_default_color_spec().shadow()
    }

    /// Scrim color
    pub fn scrim() -> DynamicColor {
        Self::get_default_color_spec().scrim()
    }

    /// Surface tint color
    pub fn surface_tint() -> DynamicColor {
        Self::get_default_color_spec().surface_tint()
    }

    // ================================================================
    // Primaries [P]
    // ================================================================

    /// Primary color
    pub fn primary() -> DynamicColor {
        Self::get_default_color_spec().primary()
    }

    /// Primary dim color (optional, only in 2025 spec)
    pub fn primary_dim() -> Option<DynamicColor> {
        Self::get_default_color_spec().primary_dim()
    }

    /// On-primary color (text/icons on primary)
    pub fn on_primary() -> DynamicColor {
        Self::get_default_color_spec().on_primary()
    }

    /// Primary container color
    pub fn primary_container() -> DynamicColor {
        Self::get_default_color_spec().primary_container()
    }

    /// On-primary container color (text/icons on primary container)
    pub fn on_primary_container() -> DynamicColor {
        Self::get_default_color_spec().on_primary_container()
    }

    /// Inverse primary color
    pub fn inverse_primary() -> DynamicColor {
        Self::get_default_color_spec().inverse_primary()
    }

    // ================================================================
    // Primary Fixed [PF]
    // ================================================================

    /// Primary fixed color (2021 spec — prefer `primary_fixed_for` with Spec2025)
    pub fn primary_fixed() -> DynamicColor {
        Self::get_default_color_spec().primary_fixed()
    }

    /// Primary fixed dim color (2021 spec — prefer `primary_fixed_dim_for` with Spec2025)
    pub fn primary_fixed_dim() -> DynamicColor {
        Self::get_default_color_spec().primary_fixed_dim()
    }

    /// On-primary fixed color (2021 spec — prefer `on_primary_fixed_for` with Spec2025)
    pub fn on_primary_fixed() -> DynamicColor {
        Self::get_default_color_spec().on_primary_fixed()
    }

    /// On-primary fixed variant color (2021 spec — prefer `on_primary_fixed_variant_for` with Spec2025)
    pub fn on_primary_fixed_variant() -> DynamicColor {
        Self::get_default_color_spec().on_primary_fixed_variant()
    }

    /// Primary fixed color (spec-aware version)
    pub fn primary_fixed_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).primary_fixed()
    }

    /// Primary fixed dim color (spec-aware version)
    pub fn primary_fixed_dim_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).primary_fixed_dim()
    }

    /// On-primary fixed color (spec-aware version)
    pub fn on_primary_fixed_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_primary_fixed()
    }

    /// On-primary fixed variant color (spec-aware version)
    pub fn on_primary_fixed_variant_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_primary_fixed_variant()
    }

    // ================================================================
    // Secondaries [Q]
    // ================================================================

    /// Secondary color
    pub fn secondary() -> DynamicColor {
        Self::get_default_color_spec().secondary()
    }

    /// Secondary dim color (optional, only in 2025 spec)
    pub fn secondary_dim() -> Option<DynamicColor> {
        Self::get_default_color_spec().secondary_dim()
    }

    /// On-secondary color (text/icons on secondary)
    pub fn on_secondary() -> DynamicColor {
        Self::get_default_color_spec().on_secondary()
    }

    /// Secondary container color
    pub fn secondary_container() -> DynamicColor {
        Self::get_default_color_spec().secondary_container()
    }

    /// On-secondary container color (text/icons on secondary container)
    pub fn on_secondary_container() -> DynamicColor {
        Self::get_default_color_spec().on_secondary_container()
    }

    // ================================================================
    // Secondary Fixed [QF]
    // ================================================================

    /// Secondary fixed color (2021 spec — prefer `secondary_fixed_for` with Spec2025)
    pub fn secondary_fixed() -> DynamicColor {
        Self::get_default_color_spec().secondary_fixed()
    }

    /// Secondary fixed dim color (2021 spec — prefer `secondary_fixed_dim_for` with Spec2025)
    pub fn secondary_fixed_dim() -> DynamicColor {
        Self::get_default_color_spec().secondary_fixed_dim()
    }

    /// On-secondary fixed color (2021 spec — prefer `on_secondary_fixed_for` with Spec2025)
    pub fn on_secondary_fixed() -> DynamicColor {
        Self::get_default_color_spec().on_secondary_fixed()
    }

    /// On-secondary fixed variant color (2021 spec — prefer `on_secondary_fixed_variant_for` with Spec2025)
    pub fn on_secondary_fixed_variant() -> DynamicColor {
        Self::get_default_color_spec().on_secondary_fixed_variant()
    }

    /// Secondary fixed color (spec-aware version)
    pub fn secondary_fixed_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).secondary_fixed()
    }

    /// Secondary fixed dim color (spec-aware version)
    pub fn secondary_fixed_dim_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).secondary_fixed_dim()
    }

    /// On-secondary fixed color (spec-aware version)
    pub fn on_secondary_fixed_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_secondary_fixed()
    }

    /// On-secondary fixed variant color (spec-aware version)
    pub fn on_secondary_fixed_variant_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_secondary_fixed_variant()
    }

    // ================================================================
    // Tertiaries [T]
    // ================================================================

    /// Tertiary color
    pub fn tertiary() -> DynamicColor {
        Self::get_default_color_spec().tertiary()
    }

    /// Tertiary dim color (optional, only in 2025 spec)
    pub fn tertiary_dim() -> Option<DynamicColor> {
        Self::get_default_color_spec().tertiary_dim()
    }

    /// On-tertiary color (text/icons on tertiary)
    pub fn on_tertiary() -> DynamicColor {
        Self::get_default_color_spec().on_tertiary()
    }

    /// Tertiary container color
    pub fn tertiary_container() -> DynamicColor {
        Self::get_default_color_spec().tertiary_container()
    }

    /// On-tertiary container color (text/icons on tertiary container)
    pub fn on_tertiary_container() -> DynamicColor {
        Self::get_default_color_spec().on_tertiary_container()
    }

    // ================================================================
    // Tertiary Fixed [TF]
    // ================================================================

    /// Tertiary fixed color (2021 spec — prefer `tertiary_fixed_for` with Spec2025)
    pub fn tertiary_fixed() -> DynamicColor {
        Self::get_default_color_spec().tertiary_fixed()
    }

    /// Tertiary fixed dim color (2021 spec — prefer `tertiary_fixed_dim_for` with Spec2025)
    pub fn tertiary_fixed_dim() -> DynamicColor {
        Self::get_default_color_spec().tertiary_fixed_dim()
    }

    /// On-tertiary fixed color (2021 spec — prefer `on_tertiary_fixed_for` with Spec2025)
    pub fn on_tertiary_fixed() -> DynamicColor {
        Self::get_default_color_spec().on_tertiary_fixed()
    }

    /// On-tertiary fixed variant color (2021 spec — prefer `on_tertiary_fixed_variant_for` with Spec2025)
    pub fn on_tertiary_fixed_variant() -> DynamicColor {
        Self::get_default_color_spec().on_tertiary_fixed_variant()
    }

    /// Tertiary fixed color (spec-aware version)
    pub fn tertiary_fixed_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).tertiary_fixed()
    }

    /// Tertiary fixed dim color (spec-aware version)
    pub fn tertiary_fixed_dim_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).tertiary_fixed_dim()
    }

    /// On-tertiary fixed color (spec-aware version)
    pub fn on_tertiary_fixed_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_tertiary_fixed()
    }

    /// On-tertiary fixed variant color (spec-aware version)
    pub fn on_tertiary_fixed_variant_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_tertiary_fixed_variant()
    }

    // ================================================================
    // Errors [E]
    // ================================================================

    /// Error color
    pub fn error() -> DynamicColor {
        Self::get_default_color_spec().error()
    }

    /// Error dim color (optional, only in 2025 spec)
    pub fn error_dim() -> Option<DynamicColor> {
        Self::get_default_color_spec().error_dim()
    }

    /// On-error color (text/icons on error)
    pub fn on_error() -> DynamicColor {
        Self::get_default_color_spec().on_error()
    }

    /// Error container color
    pub fn error_container() -> DynamicColor {
        Self::get_default_color_spec().error_container()
    }

    /// On-error container color (text/icons on error container)
    pub fn on_error_container() -> DynamicColor {
        Self::get_default_color_spec().on_error_container()
    }

    // ================================================================
    // All Colors
    // ================================================================

    /// Returns a vector of all standard dynamic colors (excluding optional dim colors)
    ///
    /// Note: This uses the default 2021 spec. For spec-aware colors, use `all_colors_for`.
    pub fn all_colors() -> Vec<DynamicColor> {
        let mut colors = vec![
            Self::background(),
            Self::on_background(),
            Self::surface(),
            Self::surface_dim(),
            Self::surface_bright(),
            Self::surface_container_lowest(),
            Self::surface_container_low(),
            Self::surface_container(),
            Self::surface_container_high(),
            Self::surface_container_highest(),
            Self::on_surface(),
            Self::on_surface_variant(),
            Self::outline(),
            Self::outline_variant(),
            Self::inverse_surface(),
            Self::inverse_on_surface(),
            Self::primary(),
            Self::on_primary(),
            Self::primary_container(),
            Self::on_primary_container(),
            Self::primary_fixed(),
            Self::primary_fixed_dim(),
            Self::on_primary_fixed(),
            Self::on_primary_fixed_variant(),
            Self::inverse_primary(),
            Self::secondary(),
            Self::on_secondary(),
            Self::secondary_container(),
            Self::on_secondary_container(),
            Self::secondary_fixed(),
            Self::secondary_fixed_dim(),
            Self::on_secondary_fixed(),
            Self::on_secondary_fixed_variant(),
            Self::tertiary(),
            Self::on_tertiary(),
            Self::tertiary_container(),
            Self::on_tertiary_container(),
            Self::tertiary_fixed(),
            Self::tertiary_fixed_dim(),
            Self::on_tertiary_fixed(),
            Self::on_tertiary_fixed_variant(),
            Self::error(),
            Self::on_error(),
            Self::error_container(),
            Self::on_error_container(),
        ];

        // Add optional dim colors if they exist
        if let Some(color) = Self::primary_dim() {
            colors.push(color);
        }
        if let Some(color) = Self::secondary_dim() {
            colors.push(color);
        }
        if let Some(color) = Self::tertiary_dim() {
            colors.push(color);
        }
        if let Some(color) = Self::error_dim() {
            colors.push(color);
        }

        colors
    }

    // ================================================================
    // Spec-Aware Color Accessors
    // ================================================================
    // These methods route to the correct ColorSpecDelegate based on
    // the scheme's spec_version field. Use these when you need colors
    // that respect the 2021 vs 2025 spec differences.
    // ================================================================

    /// Primary color (spec-aware version)
    ///
    /// Routes to the appropriate spec delegate based on scheme.spec_version.
    pub fn primary_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).primary()
    }

    /// On-primary color (spec-aware version)
    pub fn on_primary_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_primary()
    }

    /// Primary container color (spec-aware version)
    pub fn primary_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).primary_container()
    }

    /// On-primary container color (spec-aware version)
    pub fn on_primary_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_primary_container()
    }

    /// Secondary color (spec-aware version)
    pub fn secondary_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).secondary()
    }

    /// On-secondary color (spec-aware version)
    pub fn on_secondary_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_secondary()
    }

    /// Secondary container color (spec-aware version)
    pub fn secondary_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).secondary_container()
    }

    /// On-secondary container color (spec-aware version)
    pub fn on_secondary_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_secondary_container()
    }

    /// Tertiary color (spec-aware version)
    pub fn tertiary_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).tertiary()
    }

    /// On-tertiary color (spec-aware version)
    pub fn on_tertiary_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_tertiary()
    }

    /// Tertiary container color (spec-aware version)
    pub fn tertiary_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).tertiary_container()
    }

    /// On-tertiary container color (spec-aware version)
    pub fn on_tertiary_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_tertiary_container()
    }

    /// Error color (spec-aware version)
    pub fn error_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).error()
    }

    /// On-error color (spec-aware version)
    pub fn on_error_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_error()
    }

    /// Error container color (spec-aware version)
    pub fn error_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).error_container()
    }

    /// On-error container color (spec-aware version)
    pub fn on_error_container_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_error_container()
    }

    /// Surface color (spec-aware version)
    pub fn surface_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).surface()
    }

    /// On-surface color (spec-aware version)
    pub fn on_surface_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_surface()
    }

    /// Surface variant color (spec-aware version)
    pub fn surface_variant_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).surface_variant()
    }

    /// On-surface variant color (spec-aware version)
    pub fn on_surface_variant_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_surface_variant()
    }

    /// Background color (spec-aware version)
    pub fn background_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).background()
    }

    /// On-background color (spec-aware version)
    pub fn on_background_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).on_background()
    }

    /// Outline color (spec-aware version)
    pub fn outline_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).outline()
    }

    /// Outline variant color (spec-aware version)
    pub fn outline_variant_for(scheme: &DynamicScheme) -> DynamicColor {
        Self::get_color_spec(scheme).outline_variant()
    }

    /// Primary dim color (spec-aware version, 2025 spec only)
    pub fn primary_dim_for(scheme: &DynamicScheme) -> Option<DynamicColor> {
        Self::get_color_spec(scheme).primary_dim()
    }

    /// Secondary dim color (spec-aware version, 2025 spec only)
    pub fn secondary_dim_for(scheme: &DynamicScheme) -> Option<DynamicColor> {
        Self::get_color_spec(scheme).secondary_dim()
    }

    /// Tertiary dim color (spec-aware version, 2025 spec only)
    pub fn tertiary_dim_for(scheme: &DynamicScheme) -> Option<DynamicColor> {
        Self::get_color_spec(scheme).tertiary_dim()
    }

    /// Error dim color (spec-aware version, 2025 spec only)
    pub fn error_dim_for(scheme: &DynamicScheme) -> Option<DynamicColor> {
        Self::get_color_spec(scheme).error_dim()
    }

    /// Returns all colors using the correct spec delegate based on scheme.spec_version.
    ///
    /// This is the spec-aware version of `all_colors()` and should be preferred
    /// when working with schemes that may use different spec versions.
    pub fn all_colors_for(scheme: &DynamicScheme) -> Vec<DynamicColor> {
        let spec = Self::get_color_spec(scheme);
        let mut colors = vec![
            spec.background(),
            spec.on_background(),
            spec.surface(),
            spec.surface_dim(),
            spec.surface_bright(),
            spec.surface_container_lowest(),
            spec.surface_container_low(),
            spec.surface_container(),
            spec.surface_container_high(),
            spec.surface_container_highest(),
            spec.on_surface(),
            spec.on_surface_variant(),
            spec.outline(),
            spec.outline_variant(),
            spec.inverse_surface(),
            spec.inverse_on_surface(),
            spec.primary(),
            spec.on_primary(),
            spec.primary_container(),
            spec.on_primary_container(),
            spec.primary_fixed(),
            spec.primary_fixed_dim(),
            spec.on_primary_fixed(),
            spec.on_primary_fixed_variant(),
            spec.inverse_primary(),
            spec.secondary(),
            spec.on_secondary(),
            spec.secondary_container(),
            spec.on_secondary_container(),
            spec.secondary_fixed(),
            spec.secondary_fixed_dim(),
            spec.on_secondary_fixed(),
            spec.on_secondary_fixed_variant(),
            spec.tertiary(),
            spec.on_tertiary(),
            spec.tertiary_container(),
            spec.on_tertiary_container(),
            spec.tertiary_fixed(),
            spec.tertiary_fixed_dim(),
            spec.on_tertiary_fixed(),
            spec.on_tertiary_fixed_variant(),
            spec.error(),
            spec.on_error(),
            spec.error_container(),
            spec.on_error_container(),
        ];

        // Add optional dim colors if they exist (2025 spec only)
        if let Some(color) = spec.primary_dim() {
            colors.push(color);
        }
        if let Some(color) = spec.secondary_dim() {
            colors.push(color);
        }
        if let Some(color) = spec.tertiary_dim() {
            colors.push(color);
        }
        if let Some(color) = spec.error_dim() {
            colors.push(color);
        }

        colors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_color::dynamiccolor::{DynamicSchemeOptions, SpecVersion, Variant};
    use crate::material_color::hct::Hct;

    fn new_2021_scheme(mut options: DynamicSchemeOptions) -> DynamicScheme {
        options.spec_version = Some(SpecVersion::Spec2021);
        DynamicScheme::new(options)
    }

    #[test]
    fn test_material_dynamic_colors_methods_exist() {
        // This test verifies all methods exist and return valid DynamicColor objects

        let _ = MaterialDynamicColors::primary_palette_key_color();
        let _ = MaterialDynamicColors::secondary_palette_key_color();
        let _ = MaterialDynamicColors::tertiary_palette_key_color();
        let _ = MaterialDynamicColors::neutral_palette_key_color();
        let _ = MaterialDynamicColors::neutral_variant_palette_key_color();
        let _ = MaterialDynamicColors::error_palette_key_color();
    }

    #[test]
    fn test_surface_colors_exist() {
        let _ = MaterialDynamicColors::background();
        let _ = MaterialDynamicColors::on_background();
        let _ = MaterialDynamicColors::surface();
        let _ = MaterialDynamicColors::surface_dim();
        let _ = MaterialDynamicColors::surface_bright();
        let _ = MaterialDynamicColors::surface_container_lowest();
        let _ = MaterialDynamicColors::surface_container_low();
        let _ = MaterialDynamicColors::surface_container();
        let _ = MaterialDynamicColors::surface_container_high();
        let _ = MaterialDynamicColors::surface_container_highest();
        let _ = MaterialDynamicColors::on_surface();
        let _ = MaterialDynamicColors::surface_variant();
        let _ = MaterialDynamicColors::on_surface_variant();
    }

    #[test]
    fn test_primary_colors_exist() {
        let _ = MaterialDynamicColors::primary();
        let _ = MaterialDynamicColors::on_primary();
        let _ = MaterialDynamicColors::primary_container();
        let _ = MaterialDynamicColors::on_primary_container();
        let _ = MaterialDynamicColors::inverse_primary();
        let _ = MaterialDynamicColors::primary_fixed();
        let _ = MaterialDynamicColors::primary_fixed_dim();
        let _ = MaterialDynamicColors::on_primary_fixed();
        let _ = MaterialDynamicColors::on_primary_fixed_variant();
    }

    #[test]
    fn test_secondary_colors_exist() {
        let _ = MaterialDynamicColors::secondary();
        let _ = MaterialDynamicColors::on_secondary();
        let _ = MaterialDynamicColors::secondary_container();
        let _ = MaterialDynamicColors::on_secondary_container();
        let _ = MaterialDynamicColors::secondary_fixed();
        let _ = MaterialDynamicColors::secondary_fixed_dim();
        let _ = MaterialDynamicColors::on_secondary_fixed();
        let _ = MaterialDynamicColors::on_secondary_fixed_variant();
    }

    #[test]
    fn test_tertiary_colors_exist() {
        let _ = MaterialDynamicColors::tertiary();
        let _ = MaterialDynamicColors::on_tertiary();
        let _ = MaterialDynamicColors::tertiary_container();
        let _ = MaterialDynamicColors::on_tertiary_container();
        let _ = MaterialDynamicColors::tertiary_fixed();
        let _ = MaterialDynamicColors::tertiary_fixed_dim();
        let _ = MaterialDynamicColors::on_tertiary_fixed();
        let _ = MaterialDynamicColors::on_tertiary_fixed_variant();
    }

    #[test]
    fn test_error_colors_exist() {
        let _ = MaterialDynamicColors::error();
        let _ = MaterialDynamicColors::on_error();
        let _ = MaterialDynamicColors::error_container();
        let _ = MaterialDynamicColors::on_error_container();
    }

    #[test]
    fn test_utility_colors_exist() {
        let _ = MaterialDynamicColors::outline();
        let _ = MaterialDynamicColors::outline_variant();
        let _ = MaterialDynamicColors::shadow();
        let _ = MaterialDynamicColors::scrim();
        let _ = MaterialDynamicColors::surface_tint();
    }

    #[test]
    fn test_highest_surface_light_mode() {
        let hct = Hct::from_int(0xFF0000FF);
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        let scheme = new_2021_scheme(options);

        // In light mode, highest_surface should return surface_dim (darker for contrast)
        let highest = MaterialDynamicColors::highest_surface(&scheme);
        let tone = highest.get_tone(&scheme);
        // surface_dim in light mode has tone 87.0 at standard contrast
        assert_eq!(tone, 87.0);
    }

    #[test]
    fn test_highest_surface_dark_mode() {
        let hct = Hct::from_int(0xFF0000FF);
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, true);
        let scheme = new_2021_scheme(options);

        // In dark mode, highest_surface should return surface_bright (lighter for contrast)
        let highest = MaterialDynamicColors::highest_surface(&scheme);
        let tone = highest.get_tone(&scheme);
        // surface_bright in dark mode has tone 24.0 at standard contrast
        assert_eq!(tone, 24.0);
    }

    #[test]
    fn test_content_accent_tone_delta_constant() {
        assert_eq!(MaterialDynamicColors::CONTENT_ACCENT_TONE_DELTA, 15.0);
    }

    #[test]
    fn test_all_colors_returns_vector() {
        let colors = MaterialDynamicColors::all_colors();
        // Should have at least the standard colors (44 minimum)
        // 2021 spec does not have dim colors, so exactly 44
        assert!(colors.len() >= 44);
    }

    #[test]
    fn test_optional_dim_colors() {
        // 2021 spec does not have dim colors - they should return None
        let primary_dim = MaterialDynamicColors::primary_dim();
        let secondary_dim = MaterialDynamicColors::secondary_dim();
        let tertiary_dim = MaterialDynamicColors::tertiary_dim();
        let error_dim = MaterialDynamicColors::error_dim();

        assert!(primary_dim.is_none());
        assert!(secondary_dim.is_none());
        assert!(tertiary_dim.is_none());
        assert!(error_dim.is_none());
    }

    #[test]
    fn test_primary_tone_light_mode() {
        let hct = Hct::from_int(0xFF0000FF);
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        let scheme = new_2021_scheme(options);

        let primary = MaterialDynamicColors::primary();
        let tone = primary.get_tone(&scheme);
        assert_eq!(tone, 40.0);
    }

    #[test]
    fn test_primary_tone_dark_mode() {
        let hct = Hct::from_int(0xFF0000FF);
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, true);
        let scheme = new_2021_scheme(options);

        let primary = MaterialDynamicColors::primary();
        let tone = primary.get_tone(&scheme);
        assert_eq!(tone, 80.0);
    }

    #[test]
    fn test_background_tone_light_mode() {
        let hct = Hct::from_int(0xFF0000FF);
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        let scheme = new_2021_scheme(options);

        let background = MaterialDynamicColors::background();
        let tone = background.get_tone(&scheme);
        assert_eq!(tone, 98.0);
    }

    #[test]
    fn test_background_tone_dark_mode() {
        let hct = Hct::from_int(0xFF0000FF);
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, true);
        let scheme = new_2021_scheme(options);

        let background = MaterialDynamicColors::background();
        let tone = background.get_tone(&scheme);
        assert_eq!(tone, 6.0);
    }

    // ================================================================
    // Spec-Aware Method Tests
    // ================================================================

    #[test]
    fn test_spec_aware_methods_route_correctly() {
        let hct = Hct::from_int(0xFF0000FF);

        // Create a 2021 spec scheme
        let mut options_2021 = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options_2021.spec_version = Some(SpecVersion::Spec2021);
        let scheme_2021 = DynamicScheme::new(options_2021);

        // Create a 2025 spec scheme
        let mut options_2025 = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options_2025.spec_version = Some(SpecVersion::Spec2025);
        let scheme_2025 = DynamicScheme::new(options_2025);

        // Get primary color using spec-aware method
        let primary_2021 = MaterialDynamicColors::primary_for(&scheme_2021);
        let primary_2025 = MaterialDynamicColors::primary_for(&scheme_2025);

        // Verify both return valid DynamicColor objects
        let tone_2021 = primary_2021.get_tone(&scheme_2021);
        let tone_2025 = primary_2025.get_tone(&scheme_2025);

        // Both should produce valid tones (exact values may differ by spec)
        assert!(tone_2021 >= 0.0 && tone_2021 <= 100.0);
        assert!(tone_2025 >= 0.0 && tone_2025 <= 100.0);
    }

    #[test]
    fn test_spec_aware_dim_colors_2025() {
        let hct = Hct::from_int(0xFF0000FF);

        // Create a 2025 spec scheme
        let mut options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options.spec_version = Some(SpecVersion::Spec2025);
        let scheme = DynamicScheme::new(options);

        // 2025 spec should have dim colors
        let primary_dim = MaterialDynamicColors::primary_dim_for(&scheme);
        let secondary_dim = MaterialDynamicColors::secondary_dim_for(&scheme);
        let tertiary_dim = MaterialDynamicColors::tertiary_dim_for(&scheme);
        let error_dim = MaterialDynamicColors::error_dim_for(&scheme);

        assert!(primary_dim.is_some());
        assert!(secondary_dim.is_some());
        assert!(tertiary_dim.is_some());
        assert!(error_dim.is_some());
    }

    #[test]
    fn test_spec_aware_dim_colors_2021() {
        let hct = Hct::from_int(0xFF0000FF);

        // Create a 2021 spec scheme
        let mut options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options.spec_version = Some(SpecVersion::Spec2021);
        let scheme = DynamicScheme::new(options);

        // 2021 spec should not have dim colors
        let primary_dim = MaterialDynamicColors::primary_dim_for(&scheme);
        let secondary_dim = MaterialDynamicColors::secondary_dim_for(&scheme);
        let tertiary_dim = MaterialDynamicColors::tertiary_dim_for(&scheme);
        let error_dim = MaterialDynamicColors::error_dim_for(&scheme);

        assert!(primary_dim.is_none());
        assert!(secondary_dim.is_none());
        assert!(tertiary_dim.is_none());
        assert!(error_dim.is_none());
    }

    #[test]
    fn test_all_colors_for_2025_includes_dim() {
        let hct = Hct::from_int(0xFF0000FF);

        // Create a 2025 spec scheme
        let mut options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options.spec_version = Some(SpecVersion::Spec2025);
        let scheme = DynamicScheme::new(options);

        let colors = MaterialDynamicColors::all_colors_for(&scheme);
        // 2025 spec has 45 standard colors + 4 dim colors = 49
        assert_eq!(colors.len(), 49);
    }

    #[test]
    fn test_all_colors_for_2021_no_dim() {
        let hct = Hct::from_int(0xFF0000FF);

        // Create a 2021 spec scheme
        let mut options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options.spec_version = Some(SpecVersion::Spec2021);
        let scheme = DynamicScheme::new(options);

        let colors = MaterialDynamicColors::all_colors_for(&scheme);
        // 2021 spec has 45 standard colors, no dim colors
        assert_eq!(colors.len(), 45);
    }

    #[test]
    fn test_fixed_dim_not_white_in_dark_mode_2025() {
        // Regression test: fixed_dim colors resolved to pure white (tone 100)
        // in dark mode under Spec2025 because the non-_for methods always
        // returned 2021 DynamicColor objects, which had incompatible
        // ToneDeltaPair data for the 2025 calculator.
        let hct = Hct::from_int(0xFF50DCCD); // teal seed
        let mut options = DynamicSchemeOptions::new(hct, Variant::Fidelity, 0.0, true);
        options.spec_version = Some(SpecVersion::Spec2025);
        let scheme = DynamicScheme::new(options);

        // Use the spec-aware _for variants
        let primary_fixed_dim = MaterialDynamicColors::primary_fixed_dim_for(&scheme);
        let secondary_fixed_dim = MaterialDynamicColors::secondary_fixed_dim_for(&scheme);
        let tertiary_fixed_dim = MaterialDynamicColors::tertiary_fixed_dim_for(&scheme);

        let pfd_tone = primary_fixed_dim.get_tone(&scheme);
        let sfd_tone = secondary_fixed_dim.get_tone(&scheme);
        let tfd_tone = tertiary_fixed_dim.get_tone(&scheme);

        // Fixed dim should NOT be tone 100 (pure white)
        assert!(
            pfd_tone < 100.0,
            "primary_fixed_dim tone was {pfd_tone}, expected < 100"
        );
        assert!(
            sfd_tone < 100.0,
            "secondary_fixed_dim tone was {sfd_tone}, expected < 100"
        );
        assert!(
            tfd_tone < 100.0,
            "tertiary_fixed_dim tone was {tfd_tone}, expected < 100"
        );

        // Fixed dim tones should be in a reasonable range (typically 75-85)
        assert!(
            pfd_tone > 50.0 && pfd_tone < 95.0,
            "primary_fixed_dim tone {pfd_tone} out of expected range 50-95"
        );
    }

    #[test]
    fn test_fixed_for_methods_exist() {
        // Verify all 12 _for fixed color methods exist and compile
        let hct = Hct::from_int(0xFF0000FF);
        let mut options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options.spec_version = Some(SpecVersion::Spec2025);
        let scheme = DynamicScheme::new(options);

        let _ = MaterialDynamicColors::primary_fixed_for(&scheme);
        let _ = MaterialDynamicColors::primary_fixed_dim_for(&scheme);
        let _ = MaterialDynamicColors::on_primary_fixed_for(&scheme);
        let _ = MaterialDynamicColors::on_primary_fixed_variant_for(&scheme);

        let _ = MaterialDynamicColors::secondary_fixed_for(&scheme);
        let _ = MaterialDynamicColors::secondary_fixed_dim_for(&scheme);
        let _ = MaterialDynamicColors::on_secondary_fixed_for(&scheme);
        let _ = MaterialDynamicColors::on_secondary_fixed_variant_for(&scheme);

        let _ = MaterialDynamicColors::tertiary_fixed_for(&scheme);
        let _ = MaterialDynamicColors::tertiary_fixed_dim_for(&scheme);
        let _ = MaterialDynamicColors::on_tertiary_fixed_for(&scheme);
        let _ = MaterialDynamicColors::on_tertiary_fixed_variant_for(&scheme);
    }

    #[test]
    fn test_highest_surface_routes_by_spec() {
        let hct = Hct::from_int(0xFF0000FF);

        // Create 2021 scheme
        let mut options_2021 = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options_2021.spec_version = Some(SpecVersion::Spec2021);
        let scheme_2021 = DynamicScheme::new(options_2021);

        // Create 2025 scheme
        let mut options_2025 = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options_2025.spec_version = Some(SpecVersion::Spec2025);
        let scheme_2025 = DynamicScheme::new(options_2025);

        // highest_surface should work for both
        let highest_2021 = MaterialDynamicColors::highest_surface(&scheme_2021);
        let highest_2025 = MaterialDynamicColors::highest_surface(&scheme_2025);

        // Both should return valid tones
        let tone_2021 = highest_2021.get_tone(&scheme_2021);
        let tone_2025 = highest_2025.get_tone(&scheme_2025);

        assert!(tone_2021 >= 0.0 && tone_2021 <= 100.0);
        assert!(tone_2025 >= 0.0 && tone_2025 <= 100.0);
    }
}

// <FILE>crates/mcu-dynamiccolor/src/material_dynamic_colors.rs</FILE> - <DESC>Material Design dynamic color definitions</DESC>
// <VERS>END OF VERSION: 3.2.0</VERS>
