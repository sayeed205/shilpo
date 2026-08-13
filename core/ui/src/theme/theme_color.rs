use gpui::Hsla;
use mcu_material_color::{Hct, MaterialDynamicColors};

/// Material 3 dynamic color roles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeColor {
    pub surface: Hsla,
    pub on_surface: Hsla,
    pub surface_dim: Hsla,
    pub surface_bright: Hsla,
    pub surface_container_lowest: Hsla,
    pub surface_container_low: Hsla,
    pub surface_container: Hsla,
    pub surface_container_high: Hsla,
    pub surface_container_highest: Hsla,
    pub surface_variant: Hsla,
    pub on_surface_variant: Hsla,
    pub inverse_surface: Hsla,
    pub inverse_on_surface: Hsla,
    pub outline: Hsla,
    pub outline_variant: Hsla,
    pub shadow: Hsla,
    pub scrim: Hsla,
    pub surface_tint: Hsla,
    pub primary: Hsla,
    pub on_primary: Hsla,
    pub primary_container: Hsla,
    pub on_primary_container: Hsla,
    pub inverse_primary: Hsla,
    pub primary_fixed: Hsla,
    pub primary_fixed_dim: Hsla,
    pub on_primary_fixed: Hsla,
    pub on_primary_fixed_variant: Hsla,
    pub secondary: Hsla,
    pub on_secondary: Hsla,
    pub secondary_container: Hsla,
    pub on_secondary_container: Hsla,
    pub secondary_fixed: Hsla,
    pub secondary_fixed_dim: Hsla,
    pub on_secondary_fixed: Hsla,
    pub on_secondary_fixed_variant: Hsla,
    pub tertiary: Hsla,
    pub on_tertiary: Hsla,
    pub tertiary_container: Hsla,
    pub on_tertiary_container: Hsla,
    pub tertiary_fixed: Hsla,
    pub tertiary_fixed_dim: Hsla,
    pub on_tertiary_fixed: Hsla,
    pub on_tertiary_fixed_variant: Hsla,
    pub error: Hsla,
    pub on_error: Hsla,
    pub error_container: Hsla,
    pub on_error_container: Hsla,
}

macro_rules! roles {
    ($($field:ident),+ $(,)?) => {
        impl ThemeColor {
            fn from_scheme(scheme: &mcu_material_color::DynamicScheme) -> Self {
                Self { $($field: argb_to_hsla(MaterialDynamicColors::$field().get_argb(scheme)),)+ }
            }

            pub fn interpolate(&self, target: &Self, t: f32) -> Self {
                if t.is_nan() || t <= 0.0 {
                    *self
                } else if t >= 1.0 {
                    *target
                } else {
                    Self {
                        $($field: interpolate_hsla(self.$field, target.$field, t),)+
                    }
                }
            }
        }
    };
}

roles!(
    surface,
    on_surface,
    surface_dim,
    surface_bright,
    surface_container_lowest,
    surface_container_low,
    surface_container,
    surface_container_high,
    surface_container_highest,
    surface_variant,
    on_surface_variant,
    inverse_surface,
    inverse_on_surface,
    outline,
    outline_variant,
    shadow,
    scrim,
    surface_tint,
    primary,
    on_primary,
    primary_container,
    on_primary_container,
    inverse_primary,
    primary_fixed,
    primary_fixed_dim,
    on_primary_fixed,
    on_primary_fixed_variant,
    secondary,
    on_secondary,
    secondary_container,
    on_secondary_container,
    secondary_fixed,
    secondary_fixed_dim,
    on_secondary_fixed,
    on_secondary_fixed_variant,
    tertiary,
    on_tertiary,
    tertiary_container,
    on_tertiary_container,
    tertiary_fixed,
    tertiary_fixed_dim,
    on_tertiary_fixed,
    on_tertiary_fixed_variant,
    error,
    on_error,
    error_container,
    on_error_container,
);

impl Default for ThemeColor {
    fn default() -> Self {
        material_theme(0xff6750a4, false)
    }
}

impl ThemeColor {
    pub fn from_source(source_argb: u32, dark: bool) -> Self {
        material_theme(source_argb, dark)
    }

    /// Converts all M3 color tokens to a map of `"field_name" → "#RRGGBB"` hex strings.
    ///
    /// Used by the theme daemon to write `colors.json` so any external app
    /// or script can consume the generated Material 3 palette.
    pub fn to_hex_map(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        macro_rules! insert {
            ($($field:ident),+ $(,)?) => {
                $( map.insert(stringify!($field).to_string(), hsla_to_hex(self.$field)); )+
            };
        }
        insert!(
            surface,
            on_surface,
            surface_dim,
            surface_bright,
            surface_container_lowest,
            surface_container_low,
            surface_container,
            surface_container_high,
            surface_container_highest,
            surface_variant,
            on_surface_variant,
            inverse_surface,
            inverse_on_surface,
            outline,
            outline_variant,
            shadow,
            scrim,
            surface_tint,
            primary,
            on_primary,
            primary_container,
            on_primary_container,
            inverse_primary,
            primary_fixed,
            primary_fixed_dim,
            on_primary_fixed,
            on_primary_fixed_variant,
            secondary,
            on_secondary,
            secondary_container,
            on_secondary_container,
            secondary_fixed,
            secondary_fixed_dim,
            on_secondary_fixed,
            on_secondary_fixed_variant,
            tertiary,
            on_tertiary,
            tertiary_container,
            on_tertiary_container,
            tertiary_fixed,
            tertiary_fixed_dim,
            on_tertiary_fixed,
            on_tertiary_fixed_variant,
            error,
            on_error,
            error_container,
            on_error_container,
        );
        map
    }
}

#[inline]
pub fn argb_to_hsla(argb: u32) -> Hsla {
    gpui::Rgba {
        r: ((argb >> 16) & 0xff) as f32 / 255.0,
        g: ((argb >> 8) & 0xff) as f32 / 255.0,
        b: (argb & 0xff) as f32 / 255.0,
        a: ((argb >> 24) & 0xff) as f32 / 255.0,
    }
    .into()
}

#[inline]
pub fn hsla_to_argb(hsla: Hsla) -> u32 {
    let rgba = hsla.to_rgb();
    let a = (rgba.a.clamp(0.0, 1.0) * 255.0).round() as u32;
    let r = (rgba.r.clamp(0.0, 1.0) * 255.0).round() as u32;
    let g = (rgba.g.clamp(0.0, 1.0) * 255.0).round() as u32;
    let b = (rgba.b.clamp(0.0, 1.0) * 255.0).round() as u32;
    (a << 24) | (r << 16) | (g << 8) | b
}

#[inline]
pub fn interpolate_hsla(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let from_argb = hsla_to_argb(from);
    let to_argb = hsla_to_argb(to);
    let result_argb = shilpo_theme::interpolate_argb_oklch(from_argb, to_argb, t);
    argb_to_hsla(result_argb)
}

/// Converts an Hsla color to a `#RRGGBB` hex string.
#[inline]
pub fn hsla_to_hex(hsla: Hsla) -> String {
    let rgba = hsla.to_rgb();
    let r = (rgba.r * 255.0).round() as u8;
    let g = (rgba.g * 255.0).round() as u8;
    let b = (rgba.b * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

pub fn material_theme_with_variant(
    source_argb: u32,
    variant: super::SchemeVariant,
    dark: bool,
) -> ThemeColor {
    use mcu_material_color::{
        SchemeContent, SchemeExpressive, SchemeFidelity, SchemeFruitSalad, SchemeMonochrome,
        SchemeNeutral, SchemeRainbow, SchemeTonalSpot,
    };
    let variant = super::resolve_variant(source_argb, variant);
    let hct = Hct::from_int(source_argb);
    match variant {
        super::SchemeVariant::Auto => unreachable!(),
        super::SchemeVariant::TonalSpot => {
            ThemeColor::from_scheme(&SchemeTonalSpot::new(hct, dark, 0.0))
        }
        super::SchemeVariant::Content => {
            ThemeColor::from_scheme(&SchemeContent::new(hct, dark, 0.0))
        }
        super::SchemeVariant::Expressive => {
            ThemeColor::from_scheme(&SchemeExpressive::new(hct, dark, 0.0))
        }
        super::SchemeVariant::Fidelity => {
            ThemeColor::from_scheme(&SchemeFidelity::new(hct, dark, 0.0))
        }
        super::SchemeVariant::FruitSalad => {
            ThemeColor::from_scheme(&SchemeFruitSalad::new(hct, dark, 0.0))
        }
        super::SchemeVariant::Monochrome => {
            ThemeColor::from_scheme(&SchemeMonochrome::new(hct, dark, 0.0))
        }
        super::SchemeVariant::Neutral => {
            ThemeColor::from_scheme(&SchemeNeutral::new(hct, dark, 0.0))
        }
        super::SchemeVariant::Rainbow => {
            ThemeColor::from_scheme(&SchemeRainbow::new(hct, dark, 0.0))
        }
    }
}

pub fn material_theme(source_argb: u32, dark: bool) -> ThemeColor {
    material_theme_with_variant(source_argb, super::SchemeVariant::Auto, dark)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_builds_distinct_schemes() {
        let light = material_theme(0xff6750a4, false);
        let dark = material_theme(0xff6750a4, true);
        assert_ne!(light.surface, dark.surface);
        assert_ne!(light.primary, dark.primary);
    }

    #[test]
    fn argb_converts_channels() {
        let color = argb_to_hsla(0x80402010).to_rgb();
        assert!((color.r - 0x40 as f32 / 255.0).abs() < f32::EPSILON);
        assert!((color.g - 0x20 as f32 / 255.0).abs() < f32::EPSILON);
        assert!((color.b - 0x10 as f32 / 255.0).abs() < f32::EPSILON);
        assert!((color.a - 0x80 as f32 / 255.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_theme_color_interpolation_endpoints_and_midpoint() {
        let light = material_theme(0xff6750a4, false);
        let dark = material_theme(0xff386a20, true);

        // t=0.0 reaches light byte-exact
        assert_eq!(light.interpolate(&dark, 0.0), light);

        // t=1.0 reaches dark byte-exact
        assert_eq!(light.interpolate(&dark, 1.0), dark);

        // t=0.5 produces a valid intermediate palette differing from both endpoints
        let mid = light.interpolate(&dark, 0.5);
        assert_ne!(mid.surface, light.surface);
        assert_ne!(mid.surface, dark.surface);
        assert_ne!(mid.primary, light.primary);
        assert_ne!(mid.primary, dark.primary);
    }
}
