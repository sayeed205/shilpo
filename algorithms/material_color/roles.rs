/**
 * @license
 * Copyright 2026 Google LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

// Stable Material dynamic color role identifiers. Optional dim roles remain
// addressable even for fallback specs where the canonical delegate omits them.

/// Palette-key and material color roles available from a dynamic scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialRole {
    HighestSurface,
    PrimaryPaletteKeyColor,
    SecondaryPaletteKeyColor,
    TertiaryPaletteKeyColor,
    NeutralPaletteKeyColor,
    NeutralVariantPaletteKeyColor,
    ErrorPaletteKeyColor,
    Background,
    OnBackground,
    Surface,
    SurfaceDim,
    SurfaceBright,
    SurfaceContainerLowest,
    SurfaceContainerLow,
    SurfaceContainer,
    SurfaceContainerHigh,
    SurfaceContainerHighest,
    OnSurface,
    SurfaceVariant,
    OnSurfaceVariant,
    InverseSurface,
    InverseOnSurface,
    Outline,
    OutlineVariant,
    Shadow,
    Scrim,
    SurfaceTint,
    Primary,
    PrimaryDim,
    OnPrimary,
    PrimaryContainer,
    OnPrimaryContainer,
    PrimaryFixed,
    PrimaryFixedDim,
    OnPrimaryFixed,
    OnPrimaryFixedVariant,
    InversePrimary,
    Secondary,
    SecondaryDim,
    OnSecondary,
    SecondaryContainer,
    OnSecondaryContainer,
    SecondaryFixed,
    SecondaryFixedDim,
    OnSecondaryFixed,
    OnSecondaryFixedVariant,
    Tertiary,
    TertiaryDim,
    OnTertiary,
    TertiaryContainer,
    OnTertiaryContainer,
    TertiaryFixed,
    TertiaryFixedDim,
    OnTertiaryFixed,
    OnTertiaryFixedVariant,
    Error,
    ErrorDim,
    OnError,
    ErrorContainer,
    OnErrorContainer,
}

impl MaterialRole {
    /// Canonical upstream dynamic color identifier.
    pub const fn name(self) -> &'static str {
        match self {
            Self::HighestSurface => "highest_surface",
            Self::PrimaryPaletteKeyColor => "primary_palette_key_color",
            Self::SecondaryPaletteKeyColor => "secondary_palette_key_color",
            Self::TertiaryPaletteKeyColor => "tertiary_palette_key_color",
            Self::NeutralPaletteKeyColor => "neutral_palette_key_color",
            Self::NeutralVariantPaletteKeyColor => "neutral_variant_palette_key_color",
            Self::ErrorPaletteKeyColor => "error_palette_key_color",
            Self::Background => "background",
            Self::OnBackground => "on_background",
            Self::Surface => "surface",
            Self::SurfaceDim => "surface_dim",
            Self::SurfaceBright => "surface_bright",
            Self::SurfaceContainerLowest => "surface_container_lowest",
            Self::SurfaceContainerLow => "surface_container_low",
            Self::SurfaceContainer => "surface_container",
            Self::SurfaceContainerHigh => "surface_container_high",
            Self::SurfaceContainerHighest => "surface_container_highest",
            Self::OnSurface => "on_surface",
            Self::SurfaceVariant => "surface_variant",
            Self::OnSurfaceVariant => "on_surface_variant",
            Self::InverseSurface => "inverse_surface",
            Self::InverseOnSurface => "inverse_on_surface",
            Self::Outline => "outline",
            Self::OutlineVariant => "outline_variant",
            Self::Shadow => "shadow",
            Self::Scrim => "scrim",
            Self::SurfaceTint => "surface_tint",
            Self::Primary => "primary",
            Self::PrimaryDim => "primary_dim",
            Self::OnPrimary => "on_primary",
            Self::PrimaryContainer => "primary_container",
            Self::OnPrimaryContainer => "on_primary_container",
            Self::PrimaryFixed => "primary_fixed",
            Self::PrimaryFixedDim => "primary_fixed_dim",
            Self::OnPrimaryFixed => "on_primary_fixed",
            Self::OnPrimaryFixedVariant => "on_primary_fixed_variant",
            Self::InversePrimary => "inverse_primary",
            Self::Secondary => "secondary",
            Self::SecondaryDim => "secondary_dim",
            Self::OnSecondary => "on_secondary",
            Self::SecondaryContainer => "secondary_container",
            Self::OnSecondaryContainer => "on_secondary_container",
            Self::SecondaryFixed => "secondary_fixed",
            Self::SecondaryFixedDim => "secondary_fixed_dim",
            Self::OnSecondaryFixed => "on_secondary_fixed",
            Self::OnSecondaryFixedVariant => "on_secondary_fixed_variant",
            Self::Tertiary => "tertiary",
            Self::TertiaryDim => "tertiary_dim",
            Self::OnTertiary => "on_tertiary",
            Self::TertiaryContainer => "tertiary_container",
            Self::OnTertiaryContainer => "on_tertiary_container",
            Self::TertiaryFixed => "tertiary_fixed",
            Self::TertiaryFixedDim => "tertiary_fixed_dim",
            Self::OnTertiaryFixed => "on_tertiary_fixed",
            Self::OnTertiaryFixedVariant => "on_tertiary_fixed_variant",
            Self::Error => "error",
            Self::ErrorDim => "error_dim",
            Self::OnError => "on_error",
            Self::ErrorContainer => "error_container",
            Self::OnErrorContainer => "on_error_container",
        }
    }

    pub const fn all() -> &'static [MaterialRole] {
        &ALL_ROLES
    }
}

const ALL_ROLES: [MaterialRole; 60] = [
    MaterialRole::HighestSurface,
    MaterialRole::PrimaryPaletteKeyColor,
    MaterialRole::SecondaryPaletteKeyColor,
    MaterialRole::TertiaryPaletteKeyColor,
    MaterialRole::NeutralPaletteKeyColor,
    MaterialRole::NeutralVariantPaletteKeyColor,
    MaterialRole::ErrorPaletteKeyColor,
    MaterialRole::Background,
    MaterialRole::OnBackground,
    MaterialRole::Surface,
    MaterialRole::SurfaceDim,
    MaterialRole::SurfaceBright,
    MaterialRole::SurfaceContainerLowest,
    MaterialRole::SurfaceContainerLow,
    MaterialRole::SurfaceContainer,
    MaterialRole::SurfaceContainerHigh,
    MaterialRole::SurfaceContainerHighest,
    MaterialRole::OnSurface,
    MaterialRole::SurfaceVariant,
    MaterialRole::OnSurfaceVariant,
    MaterialRole::InverseSurface,
    MaterialRole::InverseOnSurface,
    MaterialRole::Outline,
    MaterialRole::OutlineVariant,
    MaterialRole::Shadow,
    MaterialRole::Scrim,
    MaterialRole::SurfaceTint,
    MaterialRole::Primary,
    MaterialRole::PrimaryDim,
    MaterialRole::OnPrimary,
    MaterialRole::PrimaryContainer,
    MaterialRole::OnPrimaryContainer,
    MaterialRole::PrimaryFixed,
    MaterialRole::PrimaryFixedDim,
    MaterialRole::OnPrimaryFixed,
    MaterialRole::OnPrimaryFixedVariant,
    MaterialRole::InversePrimary,
    MaterialRole::Secondary,
    MaterialRole::SecondaryDim,
    MaterialRole::OnSecondary,
    MaterialRole::SecondaryContainer,
    MaterialRole::OnSecondaryContainer,
    MaterialRole::SecondaryFixed,
    MaterialRole::SecondaryFixedDim,
    MaterialRole::OnSecondaryFixed,
    MaterialRole::OnSecondaryFixedVariant,
    MaterialRole::Tertiary,
    MaterialRole::TertiaryDim,
    MaterialRole::OnTertiary,
    MaterialRole::TertiaryContainer,
    MaterialRole::OnTertiaryContainer,
    MaterialRole::TertiaryFixed,
    MaterialRole::TertiaryFixedDim,
    MaterialRole::OnTertiaryFixed,
    MaterialRole::OnTertiaryFixedVariant,
    MaterialRole::Error,
    MaterialRole::ErrorDim,
    MaterialRole::OnError,
    MaterialRole::ErrorContainer,
    MaterialRole::OnErrorContainer,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_roles_have_unique_canonical_names() {
        let roles = MaterialRole::all();
        assert_eq!(roles.len(), 60);
        for (index, role) in roles.iter().enumerate() {
            assert!(!role.name().is_empty());
            assert!(!roles[..index].iter().any(|other| other.name() == role.name()));
        }
    }

}
