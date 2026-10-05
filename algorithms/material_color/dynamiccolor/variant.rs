// <FILE>crates/mcu-dynamiccolor/src/variant.rs</FILE> - <DESC>Dynamic color variant enumeration</DESC>
// <VERS>VERSION: 1.1.0</VERS>
// <WCTX>Complete implementation of Material Design 3 dynamic color variants</WCTX>
// <CLOG>Add all 9 variants with documentation, derive standard traits, add comprehensive unit tests</CLOG>

/// Enumeration of dynamic color scheme variants for the Material Color 2026 spec.
///
/// Each variant represents a different approach to generating color schemes
/// from a source color, balancing different aesthetic and functional properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    /// Monochrome variant using only lightness and saturation variations.
    ///
    /// Creates a scheme using a single hue, varying only the tone (lightness).
    /// Useful for minimalist, accessibility-focused designs.
    Monochrome,

    /// Neutral variant with subtle color shifts.
    ///
    /// Generates a scheme that is mostly neutral with very subtle chromatic content.
    /// Best for conservative, professional designs.
    Neutral,

    /// Tonal spot variant emphasizing the source color with tonal adjustments.
    ///
    /// Uses the source color as an accent with complementary tones.
    /// This is the most commonly used variant and the default.
    TonalSpot,

    /// Vibrant variant with saturated, bold colors.
    ///
    /// Generates a scheme with maximum saturation while maintaining readability.
    /// Ideal for energetic, modern designs.
    Vibrant,

    /// Expressive variant with creative color manipulation.
    ///
    /// Uses complex color transformations to create unexpected but harmonious combinations.
    /// Best for distinctive, artistic designs.
    Expressive,

    /// Fidelity variant that closely follows the source color's hue and chroma.
    ///
    /// Prioritizes accuracy in reproducing the source color's visual characteristics.
    /// Useful when precise color representation is critical.
    Fidelity,

    /// Content variant that extracts dominant colors from images.
    ///
    /// Designed to work with extracted colors from images or artwork.
    /// Adapts to the dominant colors in visual content.
    Content,

    /// Rainbow variant creating a spectrum-inspired color scheme.
    ///
    /// Generates colors distributed across the full hue range.
    /// Best for playful, colorful designs.
    Rainbow,

    /// Fruit salad variant with warm, saturated colors.
    ///
    /// Creates a scheme with warm hues and high saturation, inspired by fruit colors.
    /// Ideal for fun, appetizing designs.
    FruitSalad,
    /// Content-mediated flavor variant; requires the 2026 color specification.
    Cmf,
}

impl Default for Variant {
    /// The default variant is TonalSpot, the most commonly used variant.
    fn default() -> Self {
        Variant::TonalSpot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_monochrome() {
        let variant = Variant::Monochrome;
        assert_eq!(variant, Variant::Monochrome);
    }

    #[test]
    fn test_variant_neutral() {
        let variant = Variant::Neutral;
        assert_eq!(variant, Variant::Neutral);
    }

    #[test]
    fn test_variant_tonal_spot() {
        let variant = Variant::TonalSpot;
        assert_eq!(variant, Variant::TonalSpot);
    }

    #[test]
    fn test_variant_vibrant() {
        let variant = Variant::Vibrant;
        assert_eq!(variant, Variant::Vibrant);
    }

    #[test]
    fn test_variant_expressive() {
        let variant = Variant::Expressive;
        assert_eq!(variant, Variant::Expressive);
    }

    #[test]
    fn test_variant_fidelity() {
        let variant = Variant::Fidelity;
        assert_eq!(variant, Variant::Fidelity);
    }

    #[test]
    fn test_variant_content() {
        let variant = Variant::Content;
        assert_eq!(variant, Variant::Content);
    }

    #[test]
    fn test_variant_rainbow() {
        let variant = Variant::Rainbow;
        assert_eq!(variant, Variant::Rainbow);
    }

    #[test]
    fn test_variant_fruit_salad() {
        let variant = Variant::FruitSalad;
        assert_eq!(variant, Variant::FruitSalad);
    }

    #[test]
    fn test_variant_default_is_tonal_spot() {
        assert_eq!(Variant::default(), Variant::TonalSpot);
    }

    #[test]
    fn test_variant_clone() {
        let variant = Variant::Vibrant;
        let cloned = variant; // Copy trait, clone is implicit
        assert_eq!(variant, cloned);
    }

    #[test]
    fn test_variant_copy() {
        let variant = Variant::Expressive;
        let copied = variant;
        assert_eq!(variant, copied);
    }

    #[test]
    fn test_variant_debug() {
        let variant = Variant::FruitSalad;
        let debug_str = format!("{:?}", variant);
        assert_eq!(debug_str, "FruitSalad");
    }

    #[test]
    fn test_all_variants_are_distinct() {
        let variants = [
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
        ];

        for (i, &variant_a) in variants.iter().enumerate() {
            for &variant_b in variants.iter().skip(i + 1) {
                assert_ne!(variant_a, variant_b, "Variants should be distinct");
            }
        }
    }

    #[test]
    fn test_variant_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let variant1 = Variant::TonalSpot;
        let variant2 = Variant::TonalSpot;

        let mut hasher1 = DefaultHasher::new();
        variant1.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        variant2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2, "Equal variants should have equal hashes");
    }

    #[test]
    fn test_variant_in_collection() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(Variant::TonalSpot);
        set.insert(Variant::Vibrant);
        set.insert(Variant::TonalSpot); // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&Variant::TonalSpot));
        assert!(set.contains(&Variant::Vibrant));
        assert!(!set.contains(&Variant::Rainbow));
    }
}

// <FILE>crates/mcu-dynamiccolor/src/variant.rs</FILE> - <DESC>Dynamic color variant enumeration</DESC>
// <VERS>END OF VERSION: 1.1.0</VERS>
