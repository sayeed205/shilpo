use anyhow::Result;
use std::{
    process::Command,
    sync::{Arc, Mutex},
};

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

/// System screen brightness service.
pub struct BrightnessService {
    info: Arc<Mutex<BrightnessInfo>>,
    _task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for BrightnessService {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl BrightnessService {
    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(BrightnessInfo::default()));

        let info_clone = info.clone();
        let task = tokio::spawn(async move {
            loop {
                let info = query_brightness()
                    .map(|percentage| BrightnessInfo {
                        percentage,
                        available: true,
                    })
                    .unwrap_or_default();
                *info_clone.lock().unwrap() = info;
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        });

        Ok(Self {
            info,
            _task: Some(task),
        })
    }

    pub fn brightness_info(&self) -> BrightnessInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn set_brightness(&self, percentage: u8) {
        let percentage = percentage.min(100);
        {
            let mut lock = self.info.lock().unwrap();
            if !lock.available {
                tracing::warn!("brightness backend unavailable; ignoring brightness change");
                return;
            }
            lock.percentage = percentage;
        }
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
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
        let task = tokio::spawn(async move {
            let _sentinel = tx;
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        });

        let service = BrightnessService {
            info: Arc::new(Mutex::new(BrightnessInfo::default())),
            _task: Some(task),
        };

        tokio::task::yield_now().await;
        drop(service);
        tokio::task::yield_now().await;

        assert!(
            rx.recv().await.is_none(),
            "Sentinel should be dropped, channel closed"
        );
    }
}
