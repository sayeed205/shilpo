// Motion values are data only: this module does not simulate springs, evaluate
// easing curves, or define runtime interaction policy.

use super::units::Milliseconds;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MotionScheme {
    Standard,
    Expressive,
}

impl MotionScheme {
    pub const ALL: [Self; 2] = [Self::Standard, Self::Expressive];
}

/// Spatial and effects spring roles.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MotionRole {
    DefaultSpatial,
    FastSpatial,
    SlowSpatial,
    DefaultEffects,
    FastEffects,
    SlowEffects,
}

impl MotionRole {
    pub const ALL: [Self; 6] = [
        Self::DefaultSpatial,
        Self::FastSpatial,
        Self::SlowSpatial,
        Self::DefaultEffects,
        Self::FastEffects,
        Self::SlowEffects,
    ];
}

/// Spring parameters; this data is not a physics simulator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
}

/// Named duration values.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DurationRole {
    DurationExtraLong1,
    DurationExtraLong2,
    DurationExtraLong3,
    DurationExtraLong4,
    DurationLong1,
    DurationLong2,
    DurationLong3,
    DurationLong4,
    DurationMedium1,
    DurationMedium2,
    DurationMedium3,
    DurationMedium4,
    DurationShort1,
    DurationShort2,
    DurationShort3,
    DurationShort4,
}

impl DurationRole {
    pub const ALL: [Self; 16] = [
        Self::DurationExtraLong1,
        Self::DurationExtraLong2,
        Self::DurationExtraLong3,
        Self::DurationExtraLong4,
        Self::DurationLong1,
        Self::DurationLong2,
        Self::DurationLong3,
        Self::DurationLong4,
        Self::DurationMedium1,
        Self::DurationMedium2,
        Self::DurationMedium3,
        Self::DurationMedium4,
        Self::DurationShort1,
        Self::DurationShort2,
        Self::DurationShort3,
        Self::DurationShort4,
    ];
}

/// Named easing control-point values.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EasingRole {
    EasingEmphasizedCubicBezier,
    EasingEmphasizedAccelerateCubicBezier,
    EasingEmphasizedDecelerateCubicBezier,
    EasingLegacyCubicBezier,
    EasingLegacyAccelerateCubicBezier,
    EasingLegacyDecelerateCubicBezier,
    EasingLinearCubicBezier,
    EasingStandardCubicBezier,
    EasingStandardAccelerateCubicBezier,
    EasingStandardDecelerateCubicBezier,
}

impl EasingRole {
    pub const ALL: [Self; 10] = [
        Self::EasingEmphasizedCubicBezier,
        Self::EasingEmphasizedAccelerateCubicBezier,
        Self::EasingEmphasizedDecelerateCubicBezier,
        Self::EasingLegacyCubicBezier,
        Self::EasingLegacyAccelerateCubicBezier,
        Self::EasingLegacyDecelerateCubicBezier,
        Self::EasingLinearCubicBezier,
        Self::EasingStandardCubicBezier,
        Self::EasingStandardAccelerateCubicBezier,
        Self::EasingStandardDecelerateCubicBezier,
    ];
}

/// Cubic Bézier control points; no easing curve is evaluated by this type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// Resolve a spatial/effects spring role in an explicit motion scheme.
pub const fn motion(scheme: MotionScheme, role: MotionRole) -> SpringSpec {
    use MotionRole::*;

    match (scheme, role) {
        (MotionScheme::Standard, DefaultSpatial) => spring(0.9, 700.0),
        (MotionScheme::Standard, FastSpatial) => spring(0.9, 1400.0),
        (MotionScheme::Standard, SlowSpatial) => spring(0.9, 300.0),
        (MotionScheme::Standard, DefaultEffects) => spring(1.0, 1600.0),
        (MotionScheme::Standard, FastEffects) => spring(1.0, 3800.0),
        (MotionScheme::Standard, SlowEffects) => spring(1.0, 800.0),
        (MotionScheme::Expressive, DefaultSpatial) => spring(0.8, 380.0),
        (MotionScheme::Expressive, FastSpatial) => spring(0.6, 800.0),
        (MotionScheme::Expressive, SlowSpatial) => spring(0.8, 200.0),
        (MotionScheme::Expressive, DefaultEffects) => spring(1.0, 1600.0),
        (MotionScheme::Expressive, FastEffects) => spring(1.0, 3800.0),
        (MotionScheme::Expressive, SlowEffects) => spring(1.0, 800.0),
    }
}

const fn spring(damping_ratio: f32, stiffness: f32) -> SpringSpec {
    SpringSpec {
        damping_ratio,
        stiffness,
    }
}

/// Resolve a duration independently from scheme spring parameters.
pub const fn duration(role: DurationRole) -> Milliseconds {
    use DurationRole::*;

    Milliseconds(match role {
        DurationExtraLong1 => 700.0,
        DurationExtraLong2 => 800.0,
        DurationExtraLong3 => 900.0,
        DurationExtraLong4 => 1000.0,
        DurationLong1 => 450.0,
        DurationLong2 => 500.0,
        DurationLong3 => 550.0,
        DurationLong4 => 600.0,
        DurationMedium1 => 250.0,
        DurationMedium2 => 300.0,
        DurationMedium3 => 350.0,
        DurationMedium4 => 400.0,
        DurationShort1 => 50.0,
        DurationShort2 => 100.0,
        DurationShort3 => 150.0,
        DurationShort4 => 200.0,
    })
}

/// Resolve cubic Bézier control points.
pub const fn easing(role: EasingRole) -> CubicBezier {
    use EasingRole::*;

    match role {
        EasingEmphasizedCubicBezier => bezier(0.2, 0.0, 0.0, 1.0),
        EasingEmphasizedAccelerateCubicBezier => bezier(0.3, 0.0, 0.8, 0.15),
        EasingEmphasizedDecelerateCubicBezier => bezier(0.05, 0.7, 0.1, 1.0),
        EasingLegacyCubicBezier => bezier(0.4, 0.0, 0.2, 1.0),
        EasingLegacyAccelerateCubicBezier => bezier(0.4, 0.0, 1.0, 1.0),
        EasingLegacyDecelerateCubicBezier => bezier(0.0, 0.0, 0.2, 1.0),
        EasingLinearCubicBezier => bezier(0.0, 0.0, 1.0, 1.0),
        EasingStandardCubicBezier => bezier(0.2, 0.0, 0.0, 1.0),
        EasingStandardAccelerateCubicBezier => bezier(0.3, 0.0, 1.0, 1.0),
        EasingStandardDecelerateCubicBezier => bezier(0.0, 0.0, 0.0, 1.0),
    }
}

const fn bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> CubicBezier {
    CubicBezier { x1, y1, x2, y2 }
}

#[cfg(test)]
mod tests {
    use super::{
        duration, easing, motion, CubicBezier, DurationRole, EasingRole, MotionRole, MotionScheme,
        SpringSpec,
    };

    #[test]
    fn both_schemes_resolve_all_six_spring_roles_explicitly() {
        use MotionRole::*;

        let expected_values = [
            (
                MotionScheme::Standard,
                [
                    SpringSpec {
                        damping_ratio: 0.9,
                        stiffness: 700.0,
                    },
                    SpringSpec {
                        damping_ratio: 0.9,
                        stiffness: 1400.0,
                    },
                    SpringSpec {
                        damping_ratio: 0.9,
                        stiffness: 300.0,
                    },
                    SpringSpec {
                        damping_ratio: 1.0,
                        stiffness: 1600.0,
                    },
                    SpringSpec {
                        damping_ratio: 1.0,
                        stiffness: 3800.0,
                    },
                    SpringSpec {
                        damping_ratio: 1.0,
                        stiffness: 800.0,
                    },
                ],
            ),
            (
                MotionScheme::Expressive,
                [
                    SpringSpec {
                        damping_ratio: 0.8,
                        stiffness: 380.0,
                    },
                    SpringSpec {
                        damping_ratio: 0.6,
                        stiffness: 800.0,
                    },
                    SpringSpec {
                        damping_ratio: 0.8,
                        stiffness: 200.0,
                    },
                    SpringSpec {
                        damping_ratio: 1.0,
                        stiffness: 1600.0,
                    },
                    SpringSpec {
                        damping_ratio: 1.0,
                        stiffness: 3800.0,
                    },
                    SpringSpec {
                        damping_ratio: 1.0,
                        stiffness: 800.0,
                    },
                ],
            ),
        ];
        let roles = [
            DefaultSpatial,
            FastSpatial,
            SlowSpatial,
            DefaultEffects,
            FastEffects,
            SlowEffects,
        ];

        assert_eq!(MotionScheme::ALL.len(), 2);
        assert_eq!(MotionRole::ALL.len(), 6);
        for (scheme, expected) in expected_values {
            for (index, role) in roles.into_iter().enumerate() {
                assert_eq!(motion(scheme, role), expected[index]);
            }
        }
    }

    #[test]
    fn all_legacy_durations_keep_their_authored_values() {
        use DurationRole::*;

        let expected = [
            700.0, 800.0, 900.0, 1000.0, 450.0, 500.0, 550.0, 600.0, 250.0, 300.0, 350.0, 400.0,
            50.0, 100.0, 150.0, 200.0,
        ];
        let roles = [
            DurationExtraLong1,
            DurationExtraLong2,
            DurationExtraLong3,
            DurationExtraLong4,
            DurationLong1,
            DurationLong2,
            DurationLong3,
            DurationLong4,
            DurationMedium1,
            DurationMedium2,
            DurationMedium3,
            DurationMedium4,
            DurationShort1,
            DurationShort2,
            DurationShort3,
            DurationShort4,
        ];

        assert_eq!(DurationRole::ALL.len(), 16);
        for (index, role) in roles.into_iter().enumerate() {
            assert_eq!(DurationRole::ALL[index], role);
            assert_eq!(
                duration(role).value().to_bits(),
                (expected[index] as f32).to_bits()
            );
        }
    }

    #[test]
    fn all_ten_legacy_easing_control_points_match_the_authored_values() {
        use EasingRole::*;

        let expected = [
            CubicBezier {
                x1: 0.2,
                y1: 0.0,
                x2: 0.0,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.3,
                y1: 0.0,
                x2: 0.8,
                y2: 0.15,
            },
            CubicBezier {
                x1: 0.05,
                y1: 0.7,
                x2: 0.1,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.4,
                y1: 0.0,
                x2: 0.2,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.4,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 0.2,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.2,
                y1: 0.0,
                x2: 0.0,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.3,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            },
            CubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 0.0,
                y2: 1.0,
            },
        ];
        let roles = [
            EasingEmphasizedCubicBezier,
            EasingEmphasizedAccelerateCubicBezier,
            EasingEmphasizedDecelerateCubicBezier,
            EasingLegacyCubicBezier,
            EasingLegacyAccelerateCubicBezier,
            EasingLegacyDecelerateCubicBezier,
            EasingLinearCubicBezier,
            EasingStandardCubicBezier,
            EasingStandardAccelerateCubicBezier,
            EasingStandardDecelerateCubicBezier,
        ];

        assert_eq!(EasingRole::ALL.len(), 10);
        for (index, role) in roles.into_iter().enumerate() {
            assert_eq!(EasingRole::ALL[index], role);
            assert_eq!(easing(role), expected[index]);
        }
    }

    #[test]
    fn duration_and_easing_are_distinct_typed_data() {
        assert_eq!(duration(DurationRole::DurationShort1).value(), 50.0);
        assert_eq!(
            easing(EasingRole::EasingLinearCubicBezier),
            CubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0
            }
        );
    }
}
