// Typeface roles remain symbolic; no installed font asset is selected and no
// platform font scaling is applied here.

use super::units::Sp;

/// Material 3 text-style roles, including emphasized variants.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TypeRole {
    BodyLarge,
    BodyMedium,
    BodySmall,
    DisplayLarge,
    DisplayMedium,
    DisplaySmall,
    HeadlineLarge,
    HeadlineMedium,
    HeadlineSmall,
    LabelLarge,
    LabelMedium,
    LabelSmall,
    TitleLarge,
    TitleMedium,
    TitleSmall,
    BodyLargeEmphasized,
    BodyMediumEmphasized,
    BodySmallEmphasized,
    DisplayLargeEmphasized,
    DisplayMediumEmphasized,
    DisplaySmallEmphasized,
    HeadlineLargeEmphasized,
    HeadlineMediumEmphasized,
    HeadlineSmallEmphasized,
    LabelLargeEmphasized,
    LabelMediumEmphasized,
    LabelSmallEmphasized,
    TitleLargeEmphasized,
    TitleMediumEmphasized,
    TitleSmallEmphasized,
}

impl TypeRole {
    /// All 30 roles in stable enum order.
    pub const ALL: [Self; 30] = [
        Self::BodyLarge,
        Self::BodyMedium,
        Self::BodySmall,
        Self::DisplayLarge,
        Self::DisplayMedium,
        Self::DisplaySmall,
        Self::HeadlineLarge,
        Self::HeadlineMedium,
        Self::HeadlineSmall,
        Self::LabelLarge,
        Self::LabelMedium,
        Self::LabelSmall,
        Self::TitleLarge,
        Self::TitleMedium,
        Self::TitleSmall,
        Self::BodyLargeEmphasized,
        Self::BodyMediumEmphasized,
        Self::BodySmallEmphasized,
        Self::DisplayLargeEmphasized,
        Self::DisplayMediumEmphasized,
        Self::DisplaySmallEmphasized,
        Self::HeadlineLargeEmphasized,
        Self::HeadlineMediumEmphasized,
        Self::HeadlineSmallEmphasized,
        Self::LabelLargeEmphasized,
        Self::LabelMediumEmphasized,
        Self::LabelSmallEmphasized,
        Self::TitleLargeEmphasized,
        Self::TitleMediumEmphasized,
        Self::TitleSmallEmphasized,
    ];
}

/// A symbolic typeface family; both roles currently refer to SansSerif.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FontRole {
    Brand,
    Plain,
}

/// Named font weight used by a typography style.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FontWeight {
    Regular,
    Medium,
    Bold,
}

impl FontWeight {
    /// The numeric weight used by consumers that need a concrete value.
    pub const fn numeric_value(self) -> u16 {
        match self {
            Self::Regular => 400,
            Self::Medium => 500,
            Self::Bold => 700,
        }
    }
}

/// Concrete authored type-scale values for one role.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    pub font: FontRole,
    pub size: Sp,
    pub line_height: Sp,
    pub tracking: Sp,
    pub weight: FontWeight,
}

/// Resolve a text role to its authored metrics.
pub const fn typography(role: TypeRole) -> Typography {
    use FontRole::{Brand, Plain};
    use FontWeight::{Bold, Medium, Regular};
    use TypeRole::*;

    match role {
        BodyLarge => style(Plain, 16.0, 24.0, 0.5, Regular),
        BodyMedium => style(Plain, 14.0, 20.0, 0.2, Regular),
        BodySmall => style(Plain, 12.0, 16.0, 0.4, Regular),
        DisplayLarge => style(Brand, 57.0, 64.0, -0.2, Regular),
        DisplayMedium => style(Brand, 45.0, 52.0, 0.0, Regular),
        DisplaySmall => style(Brand, 36.0, 44.0, 0.0, Regular),
        HeadlineLarge => style(Brand, 32.0, 40.0, 0.0, Regular),
        HeadlineMedium => style(Brand, 28.0, 36.0, 0.0, Regular),
        HeadlineSmall => style(Brand, 24.0, 32.0, 0.0, Regular),
        LabelLarge => style(Plain, 14.0, 20.0, 0.1, Medium),
        LabelMedium => style(Plain, 12.0, 16.0, 0.5, Medium),
        LabelSmall => style(Plain, 11.0, 16.0, 0.5, Medium),
        TitleLarge => style(Brand, 22.0, 28.0, 0.0, Regular),
        TitleMedium => style(Plain, 16.0, 24.0, 0.2, Medium),
        TitleSmall => style(Plain, 14.0, 20.0, 0.1, Medium),
        BodyLargeEmphasized => style(Plain, 16.0, 24.0, 0.15, Medium),
        BodyMediumEmphasized => style(Plain, 14.0, 20.0, 0.25, Medium),
        BodySmallEmphasized => style(Plain, 12.0, 16.0, 0.4, Medium),
        DisplayLargeEmphasized => style(Brand, 57.0, 64.0, 0.0, Medium),
        DisplayMediumEmphasized => style(Brand, 45.0, 52.0, 0.0, Medium),
        DisplaySmallEmphasized => style(Brand, 36.0, 44.0, 0.0, Medium),
        HeadlineLargeEmphasized => style(Brand, 32.0, 40.0, 0.0, Medium),
        HeadlineMediumEmphasized => style(Brand, 28.0, 36.0, 0.0, Medium),
        HeadlineSmallEmphasized => style(Brand, 24.0, 32.0, 0.0, Medium),
        LabelLargeEmphasized => style(Plain, 14.0, 20.0, 0.1, Bold),
        LabelMediumEmphasized => style(Plain, 12.0, 16.0, 0.5, Bold),
        LabelSmallEmphasized => style(Plain, 11.0, 16.0, 0.5, Bold),
        TitleLargeEmphasized => style(Brand, 22.0, 28.0, 0.0, Medium),
        TitleMediumEmphasized => style(Plain, 16.0, 24.0, 0.15, Bold),
        TitleSmallEmphasized => style(Plain, 14.0, 20.0, 0.1, Bold),
    }
}

const fn style(
    font: FontRole,
    size: f32,
    line_height: f32,
    tracking: f32,
    weight: FontWeight,
) -> Typography {
    Typography {
        font,
        size: Sp(size),
        line_height: Sp(line_height),
        tracking: Sp(tracking),
        weight,
    }
}

#[cfg(test)]
mod tests {
    use super::{typography, FontRole, FontWeight, Sp, TypeRole, Typography};

    fn expected(
        font: FontRole,
        size: f32,
        line: f32,
        tracking: f32,
        weight: FontWeight,
    ) -> Typography {
        Typography {
            font,
            size: Sp(size),
            line_height: Sp(line),
            tracking: Sp(tracking),
            weight,
        }
    }

    #[test]
    fn all_30_type_scale_entries_match_the_authored_values() {
        use FontRole::{Brand, Plain};
        use FontWeight::{Bold, Medium, Regular};
        use TypeRole::*;

        let expected_values = [
            (BodyLarge, expected(Plain, 16.0, 24.0, 0.5, Regular)),
            (BodyMedium, expected(Plain, 14.0, 20.0, 0.2, Regular)),
            (BodySmall, expected(Plain, 12.0, 16.0, 0.4, Regular)),
            (DisplayLarge, expected(Brand, 57.0, 64.0, -0.2, Regular)),
            (DisplayMedium, expected(Brand, 45.0, 52.0, 0.0, Regular)),
            (DisplaySmall, expected(Brand, 36.0, 44.0, 0.0, Regular)),
            (HeadlineLarge, expected(Brand, 32.0, 40.0, 0.0, Regular)),
            (HeadlineMedium, expected(Brand, 28.0, 36.0, 0.0, Regular)),
            (HeadlineSmall, expected(Brand, 24.0, 32.0, 0.0, Regular)),
            (LabelLarge, expected(Plain, 14.0, 20.0, 0.1, Medium)),
            (LabelMedium, expected(Plain, 12.0, 16.0, 0.5, Medium)),
            (LabelSmall, expected(Plain, 11.0, 16.0, 0.5, Medium)),
            (TitleLarge, expected(Brand, 22.0, 28.0, 0.0, Regular)),
            (TitleMedium, expected(Plain, 16.0, 24.0, 0.2, Medium)),
            (TitleSmall, expected(Plain, 14.0, 20.0, 0.1, Medium)),
            (
                BodyLargeEmphasized,
                expected(Plain, 16.0, 24.0, 0.15, Medium),
            ),
            (
                BodyMediumEmphasized,
                expected(Plain, 14.0, 20.0, 0.25, Medium),
            ),
            (
                BodySmallEmphasized,
                expected(Plain, 12.0, 16.0, 0.4, Medium),
            ),
            (
                DisplayLargeEmphasized,
                expected(Brand, 57.0, 64.0, 0.0, Medium),
            ),
            (
                DisplayMediumEmphasized,
                expected(Brand, 45.0, 52.0, 0.0, Medium),
            ),
            (
                DisplaySmallEmphasized,
                expected(Brand, 36.0, 44.0, 0.0, Medium),
            ),
            (
                HeadlineLargeEmphasized,
                expected(Brand, 32.0, 40.0, 0.0, Medium),
            ),
            (
                HeadlineMediumEmphasized,
                expected(Brand, 28.0, 36.0, 0.0, Medium),
            ),
            (
                HeadlineSmallEmphasized,
                expected(Brand, 24.0, 32.0, 0.0, Medium),
            ),
            (LabelLargeEmphasized, expected(Plain, 14.0, 20.0, 0.1, Bold)),
            (
                LabelMediumEmphasized,
                expected(Plain, 12.0, 16.0, 0.5, Bold),
            ),
            (LabelSmallEmphasized, expected(Plain, 11.0, 16.0, 0.5, Bold)),
            (
                TitleLargeEmphasized,
                expected(Brand, 22.0, 28.0, 0.0, Medium),
            ),
            (
                TitleMediumEmphasized,
                expected(Plain, 16.0, 24.0, 0.15, Bold),
            ),
            (TitleSmallEmphasized, expected(Plain, 14.0, 20.0, 0.1, Bold)),
        ];

        assert_eq!(TypeRole::ALL.len(), 30);
        for (index, (role, metrics)) in expected_values.into_iter().enumerate() {
            assert_eq!(TypeRole::ALL[index], role);
            assert_eq!(typography(role), metrics, "{role:?}");
        }
    }

    #[test]
    fn type_weights_preserve_the_authored_values() {
        assert_eq!(FontWeight::Regular.numeric_value(), 400);
        assert_eq!(FontWeight::Medium.numeric_value(), 500);
        assert_eq!(FontWeight::Bold.numeric_value(), 700);
        assert_eq!(typography(TypeRole::DisplayLarge).font, FontRole::Brand);
        assert_eq!(typography(TypeRole::BodyLarge).font, FontRole::Plain);
    }
}
