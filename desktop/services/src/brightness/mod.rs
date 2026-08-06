use anyhow::Result;
use std::process::Command;

/// Screen brightness status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrightnessInfo {
    pub percentage: u8,
    pub available: bool,
}

impl BrightnessInfo {
    /// Converts a perceptual linear slider percentage (0..=100) to logarithmic raw percentage (0..=100) via quadratic curve scaling.
    pub fn perceptual_percent_to_raw(percent: u8) -> u8 {
        let p = (percent.min(100) as f64) / 100.0;
        let log_scaled = p * p;
        (log_scaled * 100.0).round() as u8
    }

    /// Converts a raw percentage (0..=100) back to perceptual linear slider percentage (0..=100).
    pub fn raw_to_perceptual_percent(raw: u8) -> u8 {
        let r = (raw.min(100) as f64) / 100.0;
        let linear = r.sqrt();
        (linear * 100.0).round() as u8
    }
}

use crate::polled::PolledService;
use std::time::Duration;
use tokio::sync::watch;

/// System screen brightness service.
pub struct BrightnessService {
    polled: PolledService<BrightnessInfo>,
}

impl BrightnessService {
    pub fn new() -> Result<Self> {
        let polled = PolledService::new(
            BrightnessInfo::default(),
            Duration::from_secs(3),
            None,
            |_curr| -> Result<BrightnessInfo, std::convert::Infallible> {
                let info = query_brightness()
                    .map(|percentage| BrightnessInfo {
                        percentage,
                        available: true,
                    })
                    .unwrap_or_default();
                Ok(info)
            },
        );
        Ok(Self { polled })
    }

    pub fn new_offline() -> Self {
        Self {
            polled: PolledService::new_offline(BrightnessInfo::default()),
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<BrightnessInfo> {
        self.polled.subscribe()
    }

    pub fn brightness_info(&self) -> BrightnessInfo {
        self.polled.get()
    }

    pub fn set_brightness(&self, percentage: u8) {
        let percentage = percentage.min(100);
        let mut current = self.polled.get();
        if !current.available {
            tracing::warn!("brightness backend unavailable; ignoring brightness change");
            return;
        }
        current.percentage = percentage;
        self.polled.send_replace(current);

        let _ = Command::new("brightnessctl")
            .args(["set", &format!("{}%", percentage)])
            .spawn();
    }

    pub fn set_brightness_smooth(&self, target_percentage: u8) {
        let target = target_percentage.min(100);
        let log_target = BrightnessInfo::perceptual_percent_to_raw(target);
        self.set_brightness(log_target);
    }
}

fn query_brightness() -> Option<u8> {
    let output_curr = Command::new("brightnessctl").args(["get"]).output().ok()?;

    let output_max = Command::new("brightnessctl").args(["max"]).output().ok()?;

    if !output_curr.status.success() || !output_max.status.success() {
        return None;
    }

    let curr_str = String::from_utf8_lossy(&output_curr.stdout)
        .trim()
        .parse::<f64>()
        .ok()?;
    let max_str = String::from_utf8_lossy(&output_max.stdout)
        .trim()
        .parse::<f64>()
        .ok()?;

    if max_str == 0.0 {
        return Some(0);
    }

    Some(((curr_str / max_str) * 100.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unavailable() {
        assert_eq!(BrightnessInfo::default().percentage, 0);
        assert!(!BrightnessInfo::default().available);
    }

    #[test]
    fn test_logarithmic_brightness_curves_and_stepping() {
        assert_eq!(BrightnessInfo::perceptual_percent_to_raw(0), 0);
        assert_eq!(BrightnessInfo::perceptual_percent_to_raw(100), 100);
        assert_eq!(BrightnessInfo::perceptual_percent_to_raw(50), 25);
        assert_eq!(BrightnessInfo::raw_to_perceptual_percent(25), 50);
    }

    #[tokio::test]
    async fn test_brightness_task_cancellation() {
        let service = BrightnessService::new().unwrap();
        tokio::task::yield_now().await;
        drop(service);
        tokio::task::yield_now().await;
    }
}
