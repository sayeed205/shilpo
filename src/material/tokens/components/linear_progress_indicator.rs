use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_thickness: Dp,
    pub active_wave_amplitude: Dp,
    pub active_wave_wavelength: Dp,
    pub height: Dp,
    pub indeterminate_active_wave_wavelength: Dp,
    pub stop_size: Dp,
    pub stop_trailing_space: Dp,
    pub track_active_space: Dp,
    pub track_thickness: Dp,
    pub wave_height: Dp,
}

/// Returns authored token values for this component.
pub const fn linear_progress_indicator() -> Tokens {
    Tokens {
        active_thickness: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        active_wave_amplitude: crate::material::tokens::units::Dp(f32::from_bits(0x40400000)),
        active_wave_wavelength: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        height: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        indeterminate_active_wave_wavelength: crate::material::tokens::units::Dp(f32::from_bits(
            0x41A00000,
        )),
        stop_size: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        stop_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        track_active_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        track_thickness: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        wave_height: crate::material::tokens::units::Dp(f32::from_bits(0x41200000)),
    }
}
