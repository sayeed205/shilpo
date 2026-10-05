// Unit wrappers retain authored f32 precision and do not perform platform
// conversion or scaling.

/// Density-independent Material measurement in density-independent units.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Dp(pub f32);

impl Dp {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

/// Scale-independent Material text measurement.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Sp(pub f32);

impl Sp {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

/// State-layer alpha value.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Opacity(pub f32);

impl Opacity {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

/// Duration in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Milliseconds(pub f32);

impl Milliseconds {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

/// Elevation in density-independent units.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Elevation(pub f32);

impl Elevation {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Dp, Elevation, Milliseconds, Opacity, Sp};

    #[test]
    fn wrappers_keep_f32_values_without_unit_conversion() {
        assert_eq!(Dp::new(1.5).value().to_bits(), 1.5_f32.to_bits());
        assert_eq!(Sp::new(-0.2).value().to_bits(), (-0.2_f32).to_bits());
        assert_eq!(Opacity::new(0.08).value().to_bits(), 0.08_f32.to_bits());
        assert_eq!(
            Milliseconds::new(50.0).value().to_bits(),
            50.0_f32.to_bits()
        );
        assert_eq!(Elevation::new(12.0).value().to_bits(), 12.0_f32.to_bits());
    }
}
