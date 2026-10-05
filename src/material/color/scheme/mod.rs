// The public seam for generating a full Material dynamic color scheme.

use crate::material::color::dynamiccolor::{DynamicScheme, MaterialDynamicColors};
use crate::material::color::dynamiccolor::SpecVersion;
use crate::material::color::hct::Hct;
use crate::material::color::options::SchemeOptions;
use crate::material::color::palettes::TonalPalette;
use crate::material::color::roles::MaterialRole;

/// A validated source-color request error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemeError {
    /// At least one seed color is required to produce a scheme.
    EmptySourceColors,
    /// CMF palette replacement is all-or-nothing; provide all six palettes.
    PartialCmfPaletteOverrides,
}

impl std::fmt::Display for SchemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySourceColors => f.write_str("source color list cannot be empty"),
            Self::PartialCmfPaletteOverrides => {
                f.write_str("CMF palette overrides must provide all six palettes")
            }
        }
    }
}

impl std::error::Error for SchemeError {}

/// A resolved Material Color scheme.
///
/// The constructor accepts one or more packed ARGB source colors, in order.
/// The first is primary and, for CMF, the second is the tertiary seed. Requests
/// always enter through the 2026 rules; canonical variant fallback is internal.
#[derive(Clone)]
pub struct Scheme {
    inner: DynamicScheme,
}

impl Scheme {
    /// Generate a scheme from one or more ARGB colors.
    pub fn new(source_colors: &[u32], options: SchemeOptions) -> Result<Self, SchemeError> {
        if source_colors.is_empty() {
            return Err(SchemeError::EmptySourceColors);
        }
        if options.variant == crate::material::color::dynamiccolor::Variant::Cmf {
            let overrides = [
                options.primary_palette.is_some(),
                options.secondary_palette.is_some(),
                options.tertiary_palette.is_some(),
                options.neutral_palette.is_some(),
                options.neutral_variant_palette.is_some(),
                options.error_palette.is_some(),
            ];
            if overrides.iter().any(|value| *value) && overrides.iter().any(|value| !*value) {
                return Err(SchemeError::PartialCmfPaletteOverrides);
            }
        }
        let sources = source_colors.iter().copied().map(Hct::from_int).collect();
        Ok(Self {
            inner: DynamicScheme::from_options(sources, options),
        })
    }

    /// Resolve a Material role to its ARGB value. `None` is retained for any
    /// role not defined by the selected specification.
    pub fn role(&self, role: MaterialRole) -> Option<u32> {
        self.resolved_role(role).map(|(argb, _)| argb)
    }

    /// Exact role tone retained for differential tests without expanding the
    /// public role interface beyond ARGB values.
    pub(crate) fn resolved_role(&self, role: MaterialRole) -> Option<(u32, f64)> {
        let color = MaterialDynamicColors::color_for_role(role, &self.inner)?;
        let tone = color.get_tone(&self.inner);
        Some((color.get_argb(&self.inner), tone))
    }

    /// Primary source color after conversion through HCT and back to ARGB.
    pub fn source_color(&self) -> u32 {
        self.inner.source_color_argb
    }

    pub fn variant(&self) -> crate::material::color::dynamiccolor::Variant {
        self.inner.variant
    }

    pub fn platform(&self) -> crate::material::color::dynamiccolor::Platform {
        self.inner.platform
    }

    pub fn is_dark(&self) -> bool {
        self.inner.is_dark
    }

    pub fn contrast_level(&self) -> f64 {
        self.inner.contrast_level
    }

    /// Effective canonical specification after per-variant fallback.
    pub fn effective_spec(&self) -> &'static str {
        match self.inner.spec_version {
            SpecVersion::Spec2021 => "2021",
            SpecVersion::Spec2025 => "2025",
            SpecVersion::Spec2026 => "2026",
        }
    }

    pub fn primary_palette(&self) -> &TonalPalette {
        &self.inner.primary_palette
    }

    pub fn secondary_palette(&self) -> &TonalPalette {
        &self.inner.secondary_palette
    }

    pub fn tertiary_palette(&self) -> &TonalPalette {
        &self.inner.tertiary_palette
    }

    pub fn neutral_palette(&self) -> &TonalPalette {
        &self.inner.neutral_palette
    }

    pub fn neutral_variant_palette(&self) -> &TonalPalette {
        &self.inner.neutral_variant_palette
    }

    pub fn error_palette(&self) -> &TonalPalette {
        &self.inner.error_palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::color::{Platform, Role, Variant};

    const SOURCE: u32 = 0xff6750a4;

    #[test]
    fn empty_source_list_is_rejected() {
        assert!(matches!(
            Scheme::new(&[], SchemeOptions::default()),
            Err(SchemeError::EmptySourceColors)
        ));
    }

    #[test]
    fn options_are_applied_without_contrast_clamping() {
        let scheme = Scheme::new(
            &[SOURCE],
            SchemeOptions::new(Variant::TonalSpot, true)
                .with_platform(Platform::Watch)
                .with_contrast_level(1.25),
        )
        .unwrap();

        assert_eq!(scheme.variant(), Variant::TonalSpot);
        assert_eq!(scheme.platform(), Platform::Watch);
        assert!(scheme.is_dark());
        assert_eq!(scheme.contrast_level(), 1.25);
        assert_eq!(scheme.effective_spec(), "2025");
    }

    #[test]
    fn all_variants_resolve_the_role_catalog_and_spec_fallback() {
        for variant in [
            Variant::Monochrome,
            Variant::Neutral,
            Variant::TonalSpot,
            Variant::Vibrant,
            Variant::Expressive,
            Variant::Fidelity,
            Variant::Content,
            Variant::Rainbow,
            Variant::FruitSalad,
            Variant::Cmf,
        ] {
            let scheme = Scheme::new(&[SOURCE], SchemeOptions::new(variant, false)).unwrap();
            let expected_spec = match variant {
                Variant::Cmf => "2026",
                Variant::Neutral
                | Variant::TonalSpot
                | Variant::Vibrant
                | Variant::Expressive => "2025",
                _ => "2021",
            };
            assert_eq!(scheme.effective_spec(), expected_spec, "{variant:?}");
            for role in Role::all() {
                assert!(scheme.role(*role).is_some(), "{variant:?} / {}", role.name());
            }
        }
    }

    #[test]
    fn cmf_consumes_second_source_as_tertiary_palette_seed() {
        let first = 0xff6750a4;
        let second = 0xff009688;
        let scheme = Scheme::new(&[first, second], SchemeOptions::new(Variant::Cmf, false)).unwrap();
        assert_eq!(scheme.tertiary_palette().hue(), Hct::from_int(second).hue());
        assert_eq!(scheme.source_color(), Hct::from_int(first).to_int());
    }

    #[test]
    fn role_argb_is_observable_for_generated_and_custom_palettes() {
        let custom = TonalPalette::from_hue_and_chroma(280.0, 54.0);
        let scheme = Scheme::new(
            &[SOURCE],
            SchemeOptions::new(Variant::Cmf, true)
                .with_primary_palette(custom.clone())
                .with_secondary_palette(TonalPalette::from_hue_and_chroma(70.0, 40.0))
                .with_tertiary_palette(TonalPalette::from_hue_and_chroma(140.0, 52.0))
                .with_neutral_palette(TonalPalette::from_hue_and_chroma(260.0, 8.0))
                .with_neutral_variant_palette(TonalPalette::from_hue_and_chroma(300.0, 16.0))
                .with_error_palette(TonalPalette::from_hue_and_chroma(10.0, 84.0)),
        )
        .unwrap();
        assert_eq!(scheme.primary_palette().hue(), custom.hue());
        assert!(scheme.role(Role::Primary).is_some());
        assert!(scheme.role(Role::PrimaryDim).is_some());
    }

    #[test]
    fn partial_cmf_palette_override_is_rejected() {
        assert!(matches!(
            Scheme::new(
                &[SOURCE],
                SchemeOptions::new(Variant::Cmf, false)
                    .with_primary_palette(TonalPalette::from_hue_and_chroma(20.0, 64.0)),
            ),
            Err(SchemeError::PartialCmfPaletteOverrides)
        ));
    }

}
