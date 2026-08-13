use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
use zbus::proxy;

#[cfg(target_os = "linux")]
#[proxy(
    interface = "org.freedesktop.GeoClue2.Manager",
    default_service = "org.freedesktop.GeoClue2",
    default_path = "/org/freedesktop/GeoClue2/Manager"
)]
pub trait GeoClueManager {
    fn get_client(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[cfg(target_os = "linux")]
#[proxy(
    interface = "org.freedesktop.GeoClue2.Client",
    default_service = "org.freedesktop.GeoClue2"
)]
pub trait GeoClueClient {
    #[zbus(property)]
    fn set_desktop_id(&self, value: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn location(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    fn start(&self) -> zbus::Result<()>;
}

#[cfg(target_os = "linux")]
#[proxy(
    interface = "org.freedesktop.GeoClue2.Location",
    default_service = "org.freedesktop.GeoClue2"
)]
pub trait GeoClueLocation {
    #[zbus(property)]
    fn latitude(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn longitude(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn accuracy(&self) -> zbus::Result<f64>;
}

/// Host location coordinates and accuracy.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocationInfo {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy_meters: f64,
}

impl LocationInfo {
    pub fn is_valid(&self) -> bool {
        (-90.0..=90.0).contains(&self.latitude) && (-180.0..=180.0).contains(&self.longitude)
    }
}

use tokio::sync::watch;

/// System Location service for querying desktop GeoClue / D-Bus coordinates.
#[derive(Clone)]
pub struct LocationService {
    cached: Arc<Mutex<Option<LocationInfo>>>,
    tx: watch::Sender<Option<LocationInfo>>,
}

impl Default for LocationService {
    fn default() -> Self {
        Self::new()
    }
}

impl LocationService {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(None);
        Self {
            cached: Arc::new(Mutex::new(None)),
            tx,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<LocationInfo>> {
        self.tx.subscribe()
    }

    /// Reads location coordinates from cache, environment, or GeoClue D-Bus service.
    pub fn read_location(&self) -> Result<LocationInfo, String> {
        // Return cached location if available
        if let Some(cached) = self.cached.lock().unwrap().as_ref() {
            return Ok(cached.clone());
        }

        // Allow environment overrides for testing / headless setups
        if let (Ok(lat_str), Ok(lon_str)) = (
            std::env::var("SHILPO_LOCATION_LAT"),
            std::env::var("SHILPO_LOCATION_LON"),
        ) && let (Ok(lat), Ok(lon)) = (lat_str.parse::<f64>(), lon_str.parse::<f64>())
        {
            let info = LocationInfo {
                latitude: lat,
                longitude: lon,
                accuracy_meters: 1000.0,
            };
            self.set_cached_location(info.clone());
            return Ok(info);
        }

        // Attempt Linux GeoClue query
        #[cfg(target_os = "linux")]
        {
            if let Ok(info) = self.fetch_geoclue_sync() {
                self.set_cached_location(info.clone());
                return Ok(info);
            }
        }

        Err("GeoClue location service is unavailable".to_string())
    }

    /// Asynchronously reads location coordinates from cache, environment, or GeoClue D-Bus service without blocking.
    pub async fn read_location_async(&self) -> Result<LocationInfo, String> {
        if let Some(cached) = self.cached_location() {
            return Ok(cached);
        }

        if let (Ok(lat_str), Ok(lon_str)) = (
            std::env::var("SHILPO_LOCATION_LAT"),
            std::env::var("SHILPO_LOCATION_LON"),
        ) && let (Ok(lat), Ok(lon)) = (lat_str.parse::<f64>(), lon_str.parse::<f64>())
        {
            let info = LocationInfo {
                latitude: lat,
                longitude: lon,
                accuracy_meters: 1000.0,
            };
            self.set_cached_location(info.clone());
            return Ok(info);
        }

        #[cfg(target_os = "linux")]
        {
            let mut last_error = None;
            for attempt in 0..3 {
                match fetch_geoclue_async().await {
                    Ok(info) => {
                        self.set_cached_location(info.clone());
                        return Ok(info);
                    }
                    Err(error) => {
                        last_error = Some(error);
                        if attempt < 2 {
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        }
                    }
                }
            }
            Err(last_error.unwrap_or_else(|| "GeoClue location lookup failed".into()))
        }

        #[cfg(not(target_os = "linux"))]
        Err("GeoClue location service is unavailable on non-Linux platforms".to_string())
    }

    #[cfg(target_os = "linux")]
    fn fetch_geoclue_sync(&self) -> Result<LocationInfo, String> {
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            tokio::task::block_in_place(|| handle.block_on(fetch_geoclue_async()))
        } else {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            rt.block_on(fetch_geoclue_async())
        }
    }

    pub fn set_cached_location(&self, location: LocationInfo) {
        *self.cached.lock().unwrap() = Some(location.clone());
        let _ = self.tx.send_replace(Some(location));
    }

    pub fn cached_location(&self) -> Option<LocationInfo> {
        self.cached.lock().unwrap().clone()
    }

    /// Checks whether the location service is available (cached, environment override, or GeoClue service).
    pub fn is_available(&self) -> bool {
        self.cached_location().is_some() || self.read_location().is_ok()
    }
}

#[cfg(target_os = "linux")]
async fn fetch_geoclue_async() -> Result<LocationInfo, String> {
    let connection = zbus::Connection::system()
        .await
        .map_err(|e| format!("Failed to connect to D-Bus system bus: {e}"))?;

    let manager_proxy = GeoClueManagerProxy::new(&connection)
        .await
        .map_err(|e| format!("Failed to create GeoClue manager proxy: {e}"))?;

    let client_path = manager_proxy
        .get_client()
        .await
        .map_err(|e| format!("Failed to obtain GeoClue client: {e}"))?;

    let client_proxy = GeoClueClientProxy::builder(&connection)
        .path(client_path)
        .map_err(|e| format!("Invalid GeoClue client path: {e}"))?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|e| format!("Failed to build GeoClue client proxy: {e}"))?;

    let _ = client_proxy.set_desktop_id("shilpo").await;
    client_proxy
        .start()
        .await
        .map_err(|e| format!("Failed to start GeoClue client: {e}"))?;

    let location_path = client_proxy
        .location()
        .await
        .map_err(|e| format!("Failed to obtain GeoClue location path: {e}"))?;

    let location_proxy = GeoClueLocationProxy::builder(&connection)
        .path(location_path)
        .map_err(|e| format!("Invalid GeoClue location path: {e}"))?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|e| format!("Failed to build GeoClue location proxy: {e}"))?;

    let lat = location_proxy
        .latitude()
        .await
        .map_err(|e| format!("Failed to get latitude: {e}"))?;
    let lon = location_proxy
        .longitude()
        .await
        .map_err(|e| format!("Failed to get longitude: {e}"))?;
    let accuracy = location_proxy.accuracy().await.unwrap_or(1000.0);

    let info = LocationInfo {
        latitude: lat,
        longitude: lon,
        accuracy_meters: accuracy,
    };

    if !info.is_valid() {
        return Err("GeoClue returned invalid coordinates".into());
    }

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_info_validation() {
        let valid = LocationInfo {
            latitude: 22.5726,
            longitude: 88.3639,
            accuracy_meters: 100.0,
        };
        assert!(valid.is_valid());

        let invalid = LocationInfo {
            latitude: 100.0,
            longitude: 88.3639,
            accuracy_meters: 100.0,
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn location_service_cache() {
        let service = LocationService::new();
        assert_eq!(service.cached_location(), None);

        let info = LocationInfo {
            latitude: 40.7128,
            longitude: -74.0060,
            accuracy_meters: 50.0,
        };
        service.set_cached_location(info.clone());
        assert_eq!(service.cached_location(), Some(info.clone()));
        assert_eq!(service.read_location().unwrap(), info);
    }
}
