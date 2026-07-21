use anyhow::Result;
use std::{
    process::Command,
    sync::{Arc, Mutex},
};

/// Screen brightness status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrightnessInfo {
    pub percentage: u8,
}

impl Default for BrightnessInfo {
    fn default() -> Self {
        Self { percentage: 70 }
    }
}

/// System screen brightness service.
pub struct BrightnessService {
    info: Arc<Mutex<BrightnessInfo>>,
}

impl BrightnessService {
    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(BrightnessInfo::default()));
        let service = Self { info };

        let info_clone = service.info.clone();
        tokio::spawn(async move {
            loop {
                let percentage = query_brightness().unwrap_or(70);
                {
                    let mut lock = info_clone.lock().unwrap();
                    *lock = BrightnessInfo { percentage };
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        });

        Ok(service)
    }

    pub fn brightness_info(&self) -> BrightnessInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn set_brightness(&self, percentage: u8) {
        let percentage = percentage.min(100);
        {
            let mut lock = self.info.lock().unwrap();
            lock.percentage = percentage;
        }
        let _ = Command::new("brightnessctl")
            .args(["set", &format!("{}%", percentage)])
            .spawn();
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
