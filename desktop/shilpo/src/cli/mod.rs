pub mod adapters;
pub mod args;
pub mod output;
#[cfg(test)]
pub mod tests;

use std::time::Duration;

pub fn parse_duration(s: Option<&str>) -> Result<Duration, String> {
    let Some(s) = s else {
        return Ok(Duration::from_secs(10));
    };
    let s = s.trim();
    if let Some(rest) = s.strip_suffix("ms") {
        rest.parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| format!("invalid timeout duration '{s}'"))
    } else if let Some(rest) = s.strip_suffix('s') {
        rest.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("invalid timeout duration '{s}'"))
    } else {
        s.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("invalid timeout duration '{s}'"))
    }
}
