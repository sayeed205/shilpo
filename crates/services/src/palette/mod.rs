use anyhow::{Context, Result, bail};
use image::DynamicImage;
use image::imageops::FilterType;
use mcu_material_color::{Hct, QuantizerCelebi, Score};
use std::path::Path;

/// All official Material Design 3 dynamic scheme variants + Auto selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemeVariant {
    #[default]
    Auto,
    TonalSpot,
    Vibrant,
    Expressive,
    Fidelity,
    Content,
    FruitSalad,
    Rainbow,
    Neutral,
    Monochrome,
}

/// Advanced Material You / MCU Palette Extractor with image downscaling, HCT chroma pre-filtering,
/// MCU score ranking, and automatic scheme variant decision tree.
#[derive(Debug, Clone, Default)]
pub struct PaletteExtractor;

impl PaletteExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extracts the optimal dominant ARGB source seed color from an image file path.
    pub fn extract_source_argb_from_file(&self, path: impl AsRef<Path>) -> Result<u32> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("Image file does not exist: {}", path.display());
        }

        let bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read image file: {}", path.display()))?;
        self.extract_source_argb_from_image_bytes(&bytes)
    }

    /// Extracts the optimal dominant ARGB source seed color from raw PNG/JPEG/WEBP image bytes.
    pub fn extract_source_argb_from_image_bytes(&self, bytes: &[u8]) -> Result<u32> {
        if bytes.is_empty() {
            bail!("Empty image byte buffer");
        }

        let dynamic_image = image::load_from_memory(bytes)
            .with_context(|| "Failed to decode image data for MCU color extraction")?;

        Ok(self.extract_source_argb_from_image(&dynamic_image))
    }

    /// Extracts the optimal dominant ARGB source seed color from a decoded DynamicImage.
    pub fn extract_source_argb_from_image(&self, img: &DynamicImage) -> u32 {
        // 1. Downscale image to 112x112 grid for fast processing (<5ms)
        let resized = img.resize(112, 112, FilterType::Triangle);
        let rgba = resized.to_rgba8();

        // 2. Filter solid pixels (alpha >= 128) and convert to ARGB 0xFFRRGGBB
        let pixels: Vec<u32> = rgba
            .pixels()
            .filter(|p| p.0[3] >= 128)
            .map(|p| {
                0xff00_0000 | ((p.0[0] as u32) << 16) | ((p.0[1] as u32) << 8) | (p.0[2] as u32)
            })
            .collect();

        if pixels.is_empty() {
            return 0xff6750a4; // Fallback Material Purple
        }

        // 3. MCU Celebi Quantization
        let mut color_to_count = QuantizerCelebi::quantize(&pixels, 128);

        // 4. HCT Chroma Pre-filtering: Filter out muddy grays (Chroma < 5.0)
        color_to_count.retain(|&argb, _| Hct::from_int(argb).chroma() >= 5.0);

        // 5. MCU Score ranking with fallback
        let scored = Score::score(&color_to_count);
        scored.first().copied().unwrap_or(0xff6750a4)
    }

    /// Automatically determines the optimal M3 SchemeVariant based on image statistical axes
    /// (Colorfulness, Mean Saturation, and Hue Spread).
    pub fn auto_detect_variant(&self, img: &DynamicImage) -> SchemeVariant {
        let resized = img.resize(256, 256, FilterType::Triangle);
        let rgba = resized.to_rgba8();

        let mut total_sat = 0.0f32;
        let mut hues = Vec::with_capacity(rgba.pixels().len());
        let mut rg_diffs = Vec::with_capacity(rgba.pixels().len());
        let mut yb_diffs = Vec::with_capacity(rgba.pixels().len());

        for p in rgba.pixels() {
            let r = p.0[0] as f32;
            let g = p.0[1] as f32;
            let b = p.0[2] as f32;

            // RGB to HSV saturation & hue calculation
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let delta = max - min;

            let sat = if max > 0.0 { delta / max } else { 0.0 };
            total_sat += sat;

            if delta > 0.0 {
                let hue = if (max - r).abs() < f32::EPSILON {
                    (g - b) / delta + (if g < b { 6.0 } else { 0.0 })
                } else if (max - g).abs() < f32::EPSILON {
                    (b - r) / delta + 2.0
                } else {
                    (r - g) / delta + 4.0
                };
                hues.push(hue * 60.0);
            }

            // Hasler-Süsstrunk colorfulness components
            let rg = (r - g).abs();
            let yb = (0.5 * (r + g) - b).abs();
            rg_diffs.push(rg);
            yb_diffs.push(yb);
        }

        let count = rgba.pixels().len() as f32;
        let mean_sat = (total_sat / count) * 255.0;

        let mean_rg = rg_diffs.iter().sum::<f32>() / count;
        let mean_yb = yb_diffs.iter().sum::<f32>() / count;
        let std_rg = (rg_diffs.iter().map(|v| (v - mean_rg).powi(2)).sum::<f32>() / count).sqrt();
        let std_yb = (yb_diffs.iter().map(|v| (v - mean_yb).powi(2)).sum::<f32>() / count).sqrt();
        let colorfulness = (std_rg.powi(2) + std_yb.powi(2)).sqrt()
            + 0.3 * (mean_rg.powi(2) + mean_yb.powi(2)).sqrt();

        let mean_hue = if !hues.is_empty() {
            hues.iter().sum::<f32>() / hues.len() as f32
        } else {
            0.0
        };
        let hue_spread = if !hues.is_empty() {
            (hues.iter().map(|h| (h - mean_hue).powi(2)).sum::<f32>() / hues.len() as f32).sqrt()
        } else {
            0.0
        };

        // Decision Tree
        if mean_sat < 20.0 {
            return SchemeVariant::Monochrome;
        }
        if colorfulness < 30.0 {
            if mean_sat < 55.0 {
                return SchemeVariant::Neutral;
            }
            if hue_spread < 22.0 {
                return SchemeVariant::Content;
            }
            return SchemeVariant::TonalSpot;
        }
        if colorfulness > 90.0 {
            if hue_spread > 55.0 && mean_sat > 150.0 {
                return SchemeVariant::Rainbow;
            }
            if mean_sat > 160.0 {
                return SchemeVariant::Fidelity;
            }
            if hue_spread > 45.0 {
                return SchemeVariant::Expressive;
            }
        }

        SchemeVariant::TonalSpot
    }

    /// Converts an ARGB u32 color integer to an HCT color instance.
    pub fn extract_hct(&self, source_argb: u32) -> Hct {
        Hct::from_int(source_argb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn test_palette_extractor_dynamic_image_scoring() {
        let mut img = RgbaImage::new(10, 10);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 255, 255]); // Solid Blue
        }
        let dynamic = DynamicImage::ImageRgba8(img);
        let extractor = PaletteExtractor::new();
        let seed = extractor.extract_source_argb_from_image(&dynamic);
        assert_eq!(seed, 0xff0000ff);
    }

    #[test]
    fn test_palette_extractor_auto_detect_grayscale() {
        let mut img = RgbaImage::new(10, 10);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([128, 128, 128, 255]); // Gray
        }
        let dynamic = DynamicImage::ImageRgba8(img);
        let extractor = PaletteExtractor::new();
        let variant = extractor.auto_detect_variant(&dynamic);
        assert_eq!(variant, SchemeVariant::Monochrome);
    }
}
