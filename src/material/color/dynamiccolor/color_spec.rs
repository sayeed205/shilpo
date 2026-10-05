
use super::DynamicColor;

/// A delegate that provides the dynamic color constraints for MaterialDynamicColors.
///
/// This is used to allow for different color constraints for different spec versions.
/// Implementations of this trait define all the color roles in Material Design 3.
pub trait ColorSpecDelegate {
    // ================================================================
    // Main Palettes
    // ================================================================

    /// Primary palette key color
    fn primary_palette_key_color(&self) -> DynamicColor;

    /// Secondary palette key color
    fn secondary_palette_key_color(&self) -> DynamicColor;

    /// Tertiary palette key color
    fn tertiary_palette_key_color(&self) -> DynamicColor;

    /// Neutral palette key color
    fn neutral_palette_key_color(&self) -> DynamicColor;

    /// Neutral variant palette key color
    fn neutral_variant_palette_key_color(&self) -> DynamicColor;

    /// Error palette key color
    fn error_palette_key_color(&self) -> DynamicColor;

    // ================================================================
    // Surfaces [S]
    // ================================================================

    /// Background color
    fn background(&self) -> DynamicColor;

    /// On-background color (text/icons on background)
    fn on_background(&self) -> DynamicColor;

    /// Surface color
    fn surface(&self) -> DynamicColor;

    /// Surface dim color (darker surface)
    fn surface_dim(&self) -> DynamicColor;

    /// Surface bright color (lighter surface)
    fn surface_bright(&self) -> DynamicColor;

    /// Surface container lowest elevation
    fn surface_container_lowest(&self) -> DynamicColor;

    /// Surface container low elevation
    fn surface_container_low(&self) -> DynamicColor;

    /// Surface container medium elevation
    fn surface_container(&self) -> DynamicColor;

    /// Surface container high elevation
    fn surface_container_high(&self) -> DynamicColor;

    /// Surface container highest elevation
    fn surface_container_highest(&self) -> DynamicColor;

    /// On-surface color (text/icons on surface)
    fn on_surface(&self) -> DynamicColor;

    /// Surface variant color
    fn surface_variant(&self) -> DynamicColor;

    /// On-surface variant color (text/icons on surface variant)
    fn on_surface_variant(&self) -> DynamicColor;

    /// Inverse surface color
    fn inverse_surface(&self) -> DynamicColor;

    /// Inverse on-surface color
    fn inverse_on_surface(&self) -> DynamicColor;

    /// Outline color
    fn outline(&self) -> DynamicColor;

    /// Outline variant color
    fn outline_variant(&self) -> DynamicColor;

    /// Shadow color
    fn shadow(&self) -> DynamicColor;

    /// Scrim color
    fn scrim(&self) -> DynamicColor;

    /// Surface tint color
    fn surface_tint(&self) -> DynamicColor;

    // ================================================================
    // Primaries [P]
    // ================================================================

    /// Primary color
    fn primary(&self) -> DynamicColor;

    /// Primary dim color (optional, only in 2025 spec)
    fn primary_dim(&self) -> Option<DynamicColor> {
        None
    }

    /// On-primary color (text/icons on primary)
    fn on_primary(&self) -> DynamicColor;

    /// Primary container color
    fn primary_container(&self) -> DynamicColor;

    /// On-primary container color (text/icons on primary container)
    fn on_primary_container(&self) -> DynamicColor;

    /// Inverse primary color
    fn inverse_primary(&self) -> DynamicColor;

    // ================================================================
    // Secondaries [Q]
    // ================================================================

    /// Secondary color
    fn secondary(&self) -> DynamicColor;

    /// Secondary dim color (optional, only in 2025 spec)
    fn secondary_dim(&self) -> Option<DynamicColor> {
        None
    }

    /// On-secondary color (text/icons on secondary)
    fn on_secondary(&self) -> DynamicColor;

    /// Secondary container color
    fn secondary_container(&self) -> DynamicColor;

    /// On-secondary container color (text/icons on secondary container)
    fn on_secondary_container(&self) -> DynamicColor;

    // ================================================================
    // Tertiaries [T]
    // ================================================================

    /// Tertiary color
    fn tertiary(&self) -> DynamicColor;

    /// Tertiary dim color (optional, only in 2025 spec)
    fn tertiary_dim(&self) -> Option<DynamicColor> {
        None
    }

    /// On-tertiary color (text/icons on tertiary)
    fn on_tertiary(&self) -> DynamicColor;

    /// Tertiary container color
    fn tertiary_container(&self) -> DynamicColor;

    /// On-tertiary container color (text/icons on tertiary container)
    fn on_tertiary_container(&self) -> DynamicColor;

    // ================================================================
    // Errors [E]
    // ================================================================

    /// Error color
    fn error(&self) -> DynamicColor;

    /// Error dim color (optional, only in 2025 spec)
    fn error_dim(&self) -> Option<DynamicColor> {
        None
    }

    /// On-error color (text/icons on error)
    fn on_error(&self) -> DynamicColor;

    /// Error container color
    fn error_container(&self) -> DynamicColor;

    /// On-error container color (text/icons on error container)
    fn on_error_container(&self) -> DynamicColor;

    // ================================================================
    // Primary Fixed Colors [PF]
    // ================================================================

    /// Primary fixed color
    fn primary_fixed(&self) -> DynamicColor;

    /// Primary fixed dim color
    fn primary_fixed_dim(&self) -> DynamicColor;

    /// On-primary fixed color (text/icons on primary fixed)
    fn on_primary_fixed(&self) -> DynamicColor;

    /// On-primary fixed variant color
    fn on_primary_fixed_variant(&self) -> DynamicColor;

    // ================================================================
    // Secondary Fixed Colors [QF]
    // ================================================================

    /// Secondary fixed color
    fn secondary_fixed(&self) -> DynamicColor;

    /// Secondary fixed dim color
    fn secondary_fixed_dim(&self) -> DynamicColor;

    /// On-secondary fixed color (text/icons on secondary fixed)
    fn on_secondary_fixed(&self) -> DynamicColor;

    /// On-secondary fixed variant color
    fn on_secondary_fixed_variant(&self) -> DynamicColor;

    // ================================================================
    // Tertiary Fixed Colors [TF]
    // ================================================================

    /// Tertiary fixed color
    fn tertiary_fixed(&self) -> DynamicColor;

    /// Tertiary fixed dim color
    fn tertiary_fixed_dim(&self) -> DynamicColor;

    /// On-tertiary fixed color (text/icons on tertiary fixed)
    fn on_tertiary_fixed(&self) -> DynamicColor;

    /// On-tertiary fixed variant color
    fn on_tertiary_fixed_variant(&self) -> DynamicColor;
}
