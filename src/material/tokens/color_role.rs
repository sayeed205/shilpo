// Semantic role names are represented directly as typed values.

/// A role in the Material 3 color scheme.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorRole {
    Background,
    Error,
    ErrorContainer,
    InverseOnSurface,
    InversePrimary,
    InverseSurface,
    OnBackground,
    OnError,
    OnErrorContainer,
    OnPrimary,
    OnPrimaryContainer,
    OnPrimaryFixed,
    OnPrimaryFixedVariant,
    OnSecondary,
    OnSecondaryContainer,
    OnSecondaryFixed,
    OnSecondaryFixedVariant,
    OnSurface,
    OnSurfaceVariant,
    OnTertiary,
    OnTertiaryContainer,
    OnTertiaryFixed,
    OnTertiaryFixedVariant,
    Outline,
    OutlineVariant,
    Primary,
    PrimaryContainer,
    PrimaryFixed,
    PrimaryFixedDim,
    Scrim,
    Secondary,
    SecondaryContainer,
    SecondaryFixed,
    SecondaryFixedDim,
    Surface,
    SurfaceBright,
    SurfaceContainer,
    SurfaceContainerHigh,
    SurfaceContainerHighest,
    SurfaceContainerLow,
    SurfaceContainerLowest,
    SurfaceDim,
    SurfaceTint,
    SurfaceVariant,
    Tertiary,
    TertiaryContainer,
    TertiaryFixed,
    TertiaryFixedDim,
}

impl ColorRole {
    /// All roles in stable enum order.
    pub const ALL: [Self; 48] = [
        Self::Background,
        Self::Error,
        Self::ErrorContainer,
        Self::InverseOnSurface,
        Self::InversePrimary,
        Self::InverseSurface,
        Self::OnBackground,
        Self::OnError,
        Self::OnErrorContainer,
        Self::OnPrimary,
        Self::OnPrimaryContainer,
        Self::OnPrimaryFixed,
        Self::OnPrimaryFixedVariant,
        Self::OnSecondary,
        Self::OnSecondaryContainer,
        Self::OnSecondaryFixed,
        Self::OnSecondaryFixedVariant,
        Self::OnSurface,
        Self::OnSurfaceVariant,
        Self::OnTertiary,
        Self::OnTertiaryContainer,
        Self::OnTertiaryFixed,
        Self::OnTertiaryFixedVariant,
        Self::Outline,
        Self::OutlineVariant,
        Self::Primary,
        Self::PrimaryContainer,
        Self::PrimaryFixed,
        Self::PrimaryFixedDim,
        Self::Scrim,
        Self::Secondary,
        Self::SecondaryContainer,
        Self::SecondaryFixed,
        Self::SecondaryFixedDim,
        Self::Surface,
        Self::SurfaceBright,
        Self::SurfaceContainer,
        Self::SurfaceContainerHigh,
        Self::SurfaceContainerHighest,
        Self::SurfaceContainerLow,
        Self::SurfaceContainerLowest,
        Self::SurfaceDim,
        Self::SurfaceTint,
        Self::SurfaceVariant,
        Self::Tertiary,
        Self::TertiaryContainer,
        Self::TertiaryFixed,
        Self::TertiaryFixedDim,
    ];
}

#[cfg(test)]
mod tests {
    use super::ColorRole;

    #[test]
    fn all_48_color_roles_are_present_in_declared_order() {
        assert_eq!(ColorRole::ALL.len(), 48);
        assert_eq!(ColorRole::ALL[0], ColorRole::Background);
        assert_eq!(ColorRole::ALL[25], ColorRole::Primary);
        assert_eq!(ColorRole::ALL[34], ColorRole::Surface);
        assert_eq!(ColorRole::ALL[47], ColorRole::TertiaryFixedDim);
    }
}
