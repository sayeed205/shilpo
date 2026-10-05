// <FILE>crates/mcu-dynamiccolor/src/tone_delta_pair.rs</FILE> - <DESC>Tone delta pair for dynamic color tuning</DESC>
// <VERS>VERSION: 2.0.0</VERS>
// <WCTX>Refactor ToneDeltaPair to use DynamicColor references instead of String names</WCTX>
// <CLOG>Change role_a and role_b from String to DynamicColor to match TypeScript semantics; update constructor and tests accordingly</CLOG>

use super::dynamic_color::DynamicColor;

/// Describes the different in tone between colors.
///
/// Represents how tone should relate between two dynamic colors.
/// `nearer` and `farther` are deprecated in favor of using `DeltaConstraint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TonePolarity {
    /// Tone should be darker.
    Darker,
    /// Tone should be lighter.
    Lighter,
    /// Tone should be nearer (deprecated, use DeltaConstraint instead).
    Nearer,
    /// Tone should be farther (deprecated, use DeltaConstraint instead).
    Farther,
    /// Tone should be relatively darker compared to the surface color trend.
    /// In light mode: darker; in dark mode: darker (relative to black).
    RelativeDarker,
    /// Tone should be relatively lighter compared to the surface color trend.
    /// In light mode: lighter; in dark mode: lighter (relative to black).
    RelativeLighter,
}

impl std::fmt::Display for TonePolarity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TonePolarity::Darker => write!(f, "darker"),
            TonePolarity::Lighter => write!(f, "lighter"),
            TonePolarity::Nearer => write!(f, "nearer"),
            TonePolarity::Farther => write!(f, "farther"),
            TonePolarity::RelativeDarker => write!(f, "relative_darker"),
            TonePolarity::RelativeLighter => write!(f, "relative_lighter"),
        }
    }
}

/// Describes how to fulfill a tone delta pair constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DeltaConstraint {
    /// The constraint must be satisfied exactly.
    #[default]
    Exact,
    /// The constraint should be satisfied as closely as possible (nearer to the target).
    Nearer,
    /// The constraint should be satisfied at least (farther from the reference).
    Farther,
}

impl std::fmt::Display for DeltaConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeltaConstraint::Exact => write!(f, "exact"),
            DeltaConstraint::Nearer => write!(f, "nearer"),
            DeltaConstraint::Farther => write!(f, "farther"),
        }
    }
}

/// Documents a constraint between two DynamicColors, in which their tones must
/// have a certain distance from each other.
///
/// Prefer a DynamicColor with a background; this is for special cases when
/// designers want tonal distance (literally contrast) between two colors that
/// don't have a background/foreground relationship or a contrast guarantee.
///
/// # Semantics
///
/// The polarity is an adjective that describes roleA compared to roleB.
///
/// For example, `ToneDeltaPair` with:
/// - roleA: "primary"
/// - roleB: "secondary"
/// - delta: 15.0
/// - polarity: `Darker`
/// - stayTogether: true
/// - constraint: `Exact`
///
/// States that roleA's tone should be exactly 15 darker than roleB's tone.
///
/// `RelativeDarker` and `RelativeLighter` describe the tone adjustment relative to the
/// surface color trend (white in light mode; black in dark mode). For instance, with:
/// - polarity: `RelativeLighter`
/// - constraint: `Farther`
///
/// States that roleA should be at least 10 lighter than roleB in light mode,
/// and at least 10 darker than roleB in dark mode.
///
/// DynamicColor uses Arc internally for closures, so cloning ToneDeltaPair is cheap
/// (just Arc ref count bumps).
#[derive(Clone)]
pub struct ToneDeltaPair {
    /// The first dynamic color in the pair.
    pub role_a: DynamicColor,
    /// The second dynamic color in the pair.
    pub role_b: DynamicColor,
    /// Required difference between tones. Absolute value; negative values have undefined behavior.
    pub delta: f64,
    /// The relative relation between tones of roleA and roleB, as described above.
    pub polarity: TonePolarity,
    /// Whether these two roles should stay on the same side of the "awkward zone" (T50-59).
    /// This is necessary for certain cases where one role has two backgrounds.
    pub stay_together: bool,
    /// How to fulfill the tone delta pair constraint. Defaults to `Exact`.
    pub constraint: DeltaConstraint,
}

impl ToneDeltaPair {
    /// Creates a new `ToneDeltaPair` with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `role_a` - The first dynamic color in the pair
    /// * `role_b` - The second dynamic color in the pair
    /// * `delta` - Required difference between tones (absolute value)
    /// * `polarity` - The relative relation between tones
    /// * `stay_together` - Whether roles should stay on same side of awkward zone
    /// * `constraint` - How to fulfill the constraint
    pub fn new(
        role_a: DynamicColor,
        role_b: DynamicColor,
        delta: f64,
        polarity: TonePolarity,
        stay_together: bool,
        constraint: DeltaConstraint,
    ) -> Self {
        ToneDeltaPair {
            role_a,
            role_b,
            delta,
            polarity,
            stay_together,
            constraint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_color::dynamiccolor::{ContrastCurve, DynamicScheme};
    use crate::material_color::palettes::TonalPalette;

    /// Creates a simple test DynamicColor for testing purposes.
    fn make_test_color(name: &str) -> DynamicColor {
        DynamicColor::from_palette(
            name,
            |_scheme: &DynamicScheme| TonalPalette::from_hue_and_chroma(0.0, 0.0),
            Some(|_scheme: &DynamicScheme| 50.0),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    #[test]
    fn test_tone_polarity_variants() {
        assert_eq!(TonePolarity::Darker, TonePolarity::Darker);
        assert_eq!(TonePolarity::Lighter, TonePolarity::Lighter);
        assert_eq!(TonePolarity::Nearer, TonePolarity::Nearer);
        assert_eq!(TonePolarity::Farther, TonePolarity::Farther);
        assert_eq!(TonePolarity::RelativeDarker, TonePolarity::RelativeDarker);
        assert_eq!(TonePolarity::RelativeLighter, TonePolarity::RelativeLighter);
    }

    #[test]
    fn test_tone_polarity_display() {
        assert_eq!(TonePolarity::Darker.to_string(), "darker");
        assert_eq!(TonePolarity::Lighter.to_string(), "lighter");
        assert_eq!(TonePolarity::Nearer.to_string(), "nearer");
        assert_eq!(TonePolarity::Farther.to_string(), "farther");
        assert_eq!(TonePolarity::RelativeDarker.to_string(), "relative_darker");
        assert_eq!(
            TonePolarity::RelativeLighter.to_string(),
            "relative_lighter"
        );
    }

    #[test]
    fn test_tone_polarity_clone_copy() {
        let polarity = TonePolarity::Darker;
        let polarity_copy = polarity;
        assert_eq!(polarity, polarity_copy);
    }

    #[test]
    fn test_delta_constraint_variants() {
        assert_eq!(DeltaConstraint::Exact, DeltaConstraint::Exact);
        assert_eq!(DeltaConstraint::Nearer, DeltaConstraint::Nearer);
        assert_eq!(DeltaConstraint::Farther, DeltaConstraint::Farther);
    }

    #[test]
    fn test_delta_constraint_display() {
        assert_eq!(DeltaConstraint::Exact.to_string(), "exact");
        assert_eq!(DeltaConstraint::Nearer.to_string(), "nearer");
        assert_eq!(DeltaConstraint::Farther.to_string(), "farther");
    }

    #[test]
    fn test_delta_constraint_default() {
        let constraint: DeltaConstraint = Default::default();
        assert_eq!(constraint, DeltaConstraint::Exact);
    }

    #[test]
    fn test_delta_constraint_clone_copy() {
        let constraint = DeltaConstraint::Exact;
        let constraint_copy = constraint;
        assert_eq!(constraint, constraint_copy);
    }

    #[test]
    fn test_tone_delta_pair_new() {
        let role_a = make_test_color("primary");
        let role_b = make_test_color("secondary");

        let pair = ToneDeltaPair::new(
            role_a,
            role_b,
            15.0,
            TonePolarity::Darker,
            true,
            DeltaConstraint::Exact,
        );

        assert_eq!(pair.role_a.name, "primary");
        assert_eq!(pair.role_b.name, "secondary");
        assert_eq!(pair.delta, 15.0);
        assert_eq!(pair.polarity, TonePolarity::Darker);
        assert!(pair.stay_together);
        assert_eq!(pair.constraint, DeltaConstraint::Exact);
    }

    #[test]
    fn test_tone_delta_pair_with_constraint() {
        let role_a = make_test_color("surface");
        let role_b = make_test_color("outline");

        let pair = ToneDeltaPair::new(
            role_a,
            role_b,
            10.0,
            TonePolarity::RelativeLighter,
            false,
            DeltaConstraint::Farther,
        );

        assert_eq!(pair.role_a.name, "surface");
        assert_eq!(pair.role_b.name, "outline");
        assert_eq!(pair.delta, 10.0);
        assert_eq!(pair.polarity, TonePolarity::RelativeLighter);
        assert!(!pair.stay_together);
        assert_eq!(pair.constraint, DeltaConstraint::Farther);
    }

    #[test]
    fn test_tone_delta_pair_clone() {
        let role_a = make_test_color("primary");
        let role_b = make_test_color("secondary");

        let pair = ToneDeltaPair::new(
            role_a,
            role_b,
            15.0,
            TonePolarity::Darker,
            true,
            DeltaConstraint::Nearer,
        );

        let pair_clone = pair.clone();
        assert_eq!(pair.role_a.name, pair_clone.role_a.name);
        assert_eq!(pair.role_b.name, pair_clone.role_b.name);
        assert_eq!(pair.delta, pair_clone.delta);
        assert_eq!(pair.polarity, pair_clone.polarity);
        assert_eq!(pair.stay_together, pair_clone.stay_together);
        assert_eq!(pair.constraint, pair_clone.constraint);
    }

    #[test]
    fn test_tone_polarity_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TonePolarity::Darker);
        set.insert(TonePolarity::Lighter);
        set.insert(TonePolarity::Darker); // duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_delta_constraint_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(DeltaConstraint::Exact);
        set.insert(DeltaConstraint::Nearer);
        set.insert(DeltaConstraint::Exact); // duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_tone_delta_pair_all_polarities() {
        let polarities = vec![
            TonePolarity::Darker,
            TonePolarity::Lighter,
            TonePolarity::Nearer,
            TonePolarity::Farther,
            TonePolarity::RelativeDarker,
            TonePolarity::RelativeLighter,
        ];

        for polarity in polarities {
            let role_a = make_test_color("role_a");
            let role_b = make_test_color("role_b");

            let pair =
                ToneDeltaPair::new(role_a, role_b, 10.0, polarity, true, DeltaConstraint::Exact);
            assert_eq!(pair.polarity, polarity);
        }
    }

    #[test]
    fn test_tone_delta_pair_all_constraints() {
        let constraints = vec![
            DeltaConstraint::Exact,
            DeltaConstraint::Nearer,
            DeltaConstraint::Farther,
        ];

        for constraint in constraints {
            let role_a = make_test_color("role_a");
            let role_b = make_test_color("role_b");

            let pair =
                ToneDeltaPair::new(role_a, role_b, 10.0, TonePolarity::Darker, true, constraint);
            assert_eq!(pair.constraint, constraint);
        }
    }
}

// <FILE>crates/mcu-dynamiccolor/src/tone_delta_pair.rs</FILE> - <DESC>Tone delta pair for dynamic color tuning</DESC>
// <VERS>END OF VERSION: 2.0.0</VERS>
