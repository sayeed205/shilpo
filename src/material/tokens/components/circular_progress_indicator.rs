use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_thickness: Dp,
    pub active_wave_amplitude: Dp,
    pub active_wave_wavelength: Dp,
    pub size: Dp,
    pub track_active_space: Dp,
    pub track_thickness: Dp,
    pub wave_size: Dp,
}

/// Returns authored token values for this component.
pub const fn circular_progress_indicator() -> Tokens {
    Tokens {
        active_thickness: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        active_wave_amplitude: crate::material::tokens::units::Dp(f32::from_bits(0x3FCCCCCD)),
        active_wave_wavelength: crate::material::tokens::units::Dp(f32::from_bits(0x41700000)),
        size: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        track_active_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        track_thickness: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        wave_size: crate::material::tokens::units::Dp(f32::from_bits(0x42400000)),
    }
}
