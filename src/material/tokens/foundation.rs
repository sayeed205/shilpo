// This module exposes authored elevation and state-layer values only; runtime
// interaction policy is out of scope.

use super::units::{Elevation, Opacity};

/// Named elevation levels.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ElevationRole {
    Level0,
    Level1,
    Level2,
    Level3,
    Level4,
    Level5,
}

impl ElevationRole {
    pub const ALL: [Self; 6] = [
        Self::Level0,
        Self::Level1,
        Self::Level2,
        Self::Level3,
        Self::Level4,
        Self::Level5,
    ];
}

/// State-layer opacity roles.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StateRole {
    Dragged,
    Focus,
    Hover,
    Pressed,
}

impl StateRole {
    pub const ALL: [Self; 4] = [Self::Dragged, Self::Focus, Self::Hover, Self::Pressed];
}

/// Resolve an elevation token in density-independent units.
pub const fn elevation(role: ElevationRole) -> Elevation {
    Elevation(match role {
        ElevationRole::Level0 => 0.0,
        ElevationRole::Level1 => 1.0,
        ElevationRole::Level2 => 3.0,
        ElevationRole::Level3 => 6.0,
        ElevationRole::Level4 => 8.0,
        ElevationRole::Level5 => 12.0,
    })
}

/// Resolve an authored state-layer opacity token.
pub const fn state(role: StateRole) -> Opacity {
    Opacity(match role {
        StateRole::Dragged => 0.16,
        StateRole::Focus => 0.1,
        StateRole::Hover => 0.08,
        StateRole::Pressed => 0.1,
    })
}

#[cfg(test)]
mod tests {
    use super::{elevation, state, ElevationRole, StateRole};
    use crate::material::tokens::units::{Elevation, Opacity};

    #[test]
    fn all_elevation_levels_match_authored_values() {
        let expected = [0.0, 1.0, 3.0, 6.0, 8.0, 12.0];
        assert_eq!(ElevationRole::ALL.len(), 6);
        for (index, role) in ElevationRole::ALL.into_iter().enumerate() {
            assert_eq!(elevation(role), Elevation(expected[index]));
        }
    }

    #[test]
    fn all_state_opacities_match_authored_values() {
        let expected = [0.16, 0.1, 0.08, 0.1];
        assert_eq!(StateRole::ALL.len(), 4);
        for (index, role) in StateRole::ALL.into_iter().enumerate() {
            assert_eq!(state(role), Opacity(expected[index]));
        }
    }
}
