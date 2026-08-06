use anyhow::{Result, anyhow};
use gpui::{Background, Hsla, hsla, linear_color_stop, linear_gradient};

#[inline]
pub fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    hsla(h / 360., s / 100., l / 100., 1.)
}

pub trait Colorize: Sized {
    fn opacity(&self, value: f32) -> Self;
    fn divide(&self, value: f32) -> Self;
    fn invert(&self) -> Self;
    fn invert_l(&self) -> Self;
    fn lighten(&self, value: f32) -> Self;
    fn darken(&self, value: f32) -> Self;
    fn apply(&self, color: Self) -> Self;
    fn mix(&self, color: Self, factor: f32) -> Self;
    fn mix_oklab(&self, color: Self, factor: f32) -> Self;
    fn hue(&self, value: f32) -> Self;
    fn saturation(&self, value: f32) -> Self;
    fn lightness(&self, value: f32) -> Self;
    fn to_hex(&self) -> String;
    fn parse_hex(value: &str) -> Result<Self>;
}

impl Colorize for Hsla {
    fn opacity(&self, value: f32) -> Self {
        Self {
            a: self.a * value.clamp(0., 1.),
            ..*self
        }
    }
    fn divide(&self, value: f32) -> Self {
        Self { a: value, ..*self }
    }
    fn invert(&self) -> Self {
        Self {
            h: 1. - self.h,
            s: 1. - self.s,
            l: 1. - self.l,
            ..*self
        }
    }
    fn invert_l(&self) -> Self {
        Self {
            l: 1. - self.l,
            ..*self
        }
    }
    fn lighten(&self, value: f32) -> Self {
        Self {
            l: self.l * (1. + value.clamp(0., 1.)),
            ..*self
        }
    }
    fn darken(&self, value: f32) -> Self {
        Self {
            l: self.l * (1. - value.clamp(0., 1.)),
            ..*self
        }
    }
    fn apply(&self, color: Self) -> Self {
        Self {
            h: color.h,
            s: color.s,
            ..*self
        }
    }
    fn mix(&self, color: Self, factor: f32) -> Self {
        let t = factor.clamp(0., 1.);
        Self {
            h: self.h * t + color.h * (1. - t),
            s: self.s * t + color.s * (1. - t),
            l: self.l * t + color.l * (1. - t),
            a: self.a * t + color.a * (1. - t),
        }
    }
    fn mix_oklab(&self, color: Self, factor: f32) -> Self {
        self.mix(color, factor)
    }
    fn hue(&self, value: f32) -> Self {
        Self {
            h: value.clamp(0., 1.),
            ..*self
        }
    }
    fn saturation(&self, value: f32) -> Self {
        Self {
            s: value.clamp(0., 1.),
            ..*self
        }
    }
    fn lightness(&self, value: f32) -> Self {
        Self {
            l: value.clamp(0., 1.),
            ..*self
        }
    }
    fn to_hex(&self) -> String {
        let c = self.to_rgb();
        if c.a < 1. {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                (c.r * 255.) as u32,
                (c.g * 255.) as u32,
                (c.b * 255.) as u32,
                (c.a * 255.) as u32
            )
        } else {
            format!(
                "#{:02X}{:02X}{:02X}",
                (c.r * 255.) as u32,
                (c.g * 255.) as u32,
                (c.b * 255.) as u32
            )
        }
    }
    fn parse_hex(value: &str) -> Result<Self> {
        Ok(gpui::Rgba::try_from(value)?.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorName {
    White,
    Black,
    Red,
    Blue,
    Green,
    Yellow,
    Pink,
    Orange,
    Cyan,
    Purple,
}

impl TryFrom<&str> for ColorName {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "white" => Ok(Self::White),
            "black" => Ok(Self::Black),
            "red" => Ok(Self::Red),
            "blue" => Ok(Self::Blue),
            "green" => Ok(Self::Green),
            "yellow" => Ok(Self::Yellow),
            "pink" => Ok(Self::Pink),
            "orange" => Ok(Self::Orange),
            "cyan" => Ok(Self::Cyan),
            "purple" => Ok(Self::Purple),
            _ => Err(anyhow!("invalid color name")),
        }
    }
}

impl ColorName {
    pub fn scale(&self, _scale: usize) -> Hsla {
        match self {
            Self::White => hsl(0., 0., 100.),
            Self::Black => hsl(0., 0., 0.),
            Self::Red => hsl(0., 84., 60.),
            Self::Blue => hsl(221., 83., 53.),
            Self::Green => hsl(142., 71., 45.),
            Self::Yellow => hsl(45., 93., 47.),
            Self::Pink => hsl(330., 81., 60.),
            Self::Orange => hsl(24., 95., 53.),
            Self::Cyan => hsl(189., 94., 43.),
            Self::Purple => hsl(271., 81., 56.),
        }
    }
    pub fn all() -> [Self; 8] {
        [
            Self::Red,
            Self::Blue,
            Self::Green,
            Self::Yellow,
            Self::Pink,
            Self::Orange,
            Self::Cyan,
            Self::Purple,
        ]
    }
}

pub fn black() -> Hsla {
    ColorName::Black.scale(0)
}
pub fn white() -> Hsla {
    ColorName::White.scale(0)
}

macro_rules! named { ($($name:ident => $color:ident),+ $(,)?) => { $(pub fn $name(scale: usize) -> Hsla { ColorName::$color.scale(scale) })+ }; }
named!(red => Red, blue => Blue, green => Green, yellow => Yellow, pink => Pink, orange => Orange, cyan => Cyan, purple => Purple);
pub fn red_500() -> Hsla {
    red(500)
}
pub fn blue_500() -> Hsla {
    blue(500)
}
pub fn blue_600() -> Hsla {
    blue(600)
}
pub fn green_200() -> Hsla {
    green(200)
}
pub fn green_500() -> Hsla {
    green(500)
}
pub fn yellow_100() -> Hsla {
    yellow(100)
}
pub fn yellow_500() -> Hsla {
    yellow(500)
}
pub fn pink_500() -> Hsla {
    pink(500)
}

pub fn try_parse_color(value: &str) -> Result<Hsla> {
    if value.starts_with('#') {
        return Hsla::parse_hex(value);
    }
    let (name, opacity) = if let Some((name, opacity)) = value.split_once('/') {
        (name, Some(opacity.parse::<f32>()? / 100.))
    } else {
        (value, None)
    };
    let (name, scale) = name
        .split_once('-')
        .map_or((name, 500), |(n, s)| (n, s.parse().unwrap_or(500)));
    let color = ColorName::try_from(name)?.scale(scale);
    Ok(opacity.map_or(color, |a| color.opacity(a)))
}

pub fn try_parse_background(value: &str) -> Result<Background> {
    if let Ok(color) = try_parse_color(value) {
        return Ok(color.into());
    }
    let value = value.trim();
    let inner = value
        .strip_prefix("linear-gradient(")
        .and_then(|v| v.strip_suffix(')'))
        .ok_or_else(|| anyhow!("unsupported background"))?;
    let mut parts = inner.split(',').map(str::trim);
    let first = parts.next().ok_or_else(|| anyhow!("missing gradient"))?;
    let (angle, from) = if first.ends_with("deg") {
        (
            first.trim_end_matches("deg").parse()?,
            parts.next().ok_or_else(|| anyhow!("missing stop"))?,
        )
    } else {
        (180., first)
    };
    let to = parts.next().ok_or_else(|| anyhow!("missing stop"))?;
    Ok(linear_gradient(
        angle,
        linear_color_stop(try_parse_color(from)?, 0.),
        linear_color_stop(try_parse_color(to)?, 1.),
    ))
}
