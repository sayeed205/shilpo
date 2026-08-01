#![cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]

use serde::Deserialize;
use serde_json::{json, Value};

const GEOCODE_REQUEST: &str = "weather-geocode";
const IP_GEOCODE_REQUEST: &str = "weather-ip-geocode";
const FORECAST_REQUEST: &str = "weather-forecast";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum LocationMode {
    #[default]
    Automatic,
    Manual,
    Ip,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
struct WeatherSettings {
    location_mode: LocationMode,
    location: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    temperature_unit: TemperatureUnit,
    refresh_minutes: u32,
    show_condition: bool,
}

impl Default for WeatherSettings {
    fn default() -> Self {
        Self {
            location_mode: LocationMode::Automatic,
            location: String::new(),
            latitude: None,
            longitude: None,
            temperature_unit: TemperatureUnit::Celsius,
            refresh_minutes: 30,
            show_condition: false,
        }
    }
}

impl WeatherSettings {
    fn normalized(mut self) -> Self {
        self.location = self.location.trim().to_owned();
        self.refresh_minutes = self.refresh_minutes.clamp(15, 180);
        if !matches!(self.latitude, Some(value) if (-90.0..=90.0).contains(&value))
            || !matches!(self.longitude, Some(value) if (-180.0..=180.0).contains(&value))
        {
            self.latitude = None;
            self.longitude = None;
        }
        self
    }

    fn has_coordinates(&self) -> bool {
        self.latitude.is_some() && self.longitude.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TemperatureUnit {
    fn query_value(self) -> &'static str {
        match self {
            Self::Celsius => "celsius",
            Self::Fahrenheit => "fahrenheit",
        }
    }

    fn fallback_symbol(self) -> &'static str {
        match self {
            Self::Celsius => "°C",
            Self::Fahrenheit => "°F",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Location {
    latitude: f64,
    longitude: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct WeatherSnapshot {
    temperature: f64,
    unit: String,
    weather_code: i32,
    is_day: bool,
}

#[derive(Default)]
struct WeatherState {
    settings: WeatherSettings,
    location: Option<Location>,
    snapshot: Option<WeatherSnapshot>,
    loading: bool,
    error: Option<String>,
    minutes_since_refresh: u32,
    request_serial: u64,
    active_request: Option<(String, RequestKind)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RequestKind {
    Geocode,
    IpGeocode,
    Forecast,
}

impl WeatherState {
    fn handle_event(&mut self, event: Event) -> Vec<Value> {
        match event {
            Event::ContributionSettingsChanged {
                contribution_id,
                settings,
                ..
            } if contribution_id == "bar" => {
                let next = serde_json::from_value::<WeatherSettings>(settings)
                    .unwrap_or_default()
                    .normalized();
                if next != self.settings {
                    self.settings = next;
                    self.location = None;
                    self.snapshot = None;
                    self.loading = false;
                    self.error = None;
                    self.minutes_since_refresh = 0;
                    self.active_request = None;
                }
                self.start_refresh()
            }
            Event::TimerFired { name } if name == "minute" => {
                self.minutes_since_refresh = self.minutes_since_refresh.saturating_add(1);
                if self.minutes_since_refresh >= self.settings.refresh_minutes && !self.loading {
                    self.start_refresh()
                } else {
                    Vec::new()
                }
            }
            Event::NetworkChanged { connected } if connected && !self.loading => {
                if self.snapshot.is_none() || self.error.is_some() {
                    self.start_refresh()
                } else {
                    Vec::new()
                }
            }
            Event::LocationResponse {
                latitude,
                longitude,
                ..
            } => {
                self.loading = false;
                if let (Some(lat), Some(lon)) = (latitude, longitude) {
                    self.location = Some(Location {
                        latitude: lat,
                        longitude: lon,
                    });
                    self.request_forecast()
                } else {
                    self.refresh_manual_or_fallback()
                }
            }
            Event::HttpResponse {
                request_id,
                status,
                body,
                error,
            } => self.handle_http_response(&request_id, status, &body, error),
            Event::Input {
                contribution_id,
                event_id,
                ..
            } if contribution_id == "bar" && event_id == "refresh" => self.start_refresh(),
            _ => Vec::new(),
        }
    }

    fn start_refresh(&mut self) -> Vec<Value> {
        if self.loading {
            return Vec::new();
        }
        self.error = None;
        match self.settings.location_mode {
            LocationMode::Automatic => {
                self.loading = true;
                vec![json!({ "kind": "location_read" })]
            }
            LocationMode::Ip => {
                let url = "https://ipwho.is/".to_string();
                vec![self.http_request(RequestKind::IpGeocode, IP_GEOCODE_REQUEST, url)]
            }
            LocationMode::Manual => self.refresh_manual_or_fallback(),
        }
    }

    fn refresh_manual_or_fallback(&mut self) -> Vec<Value> {
        if self.settings.has_coordinates() {
            self.location = Some(Location {
                latitude: self.settings.latitude.unwrap_or_default(),
                longitude: self.settings.longitude.unwrap_or_default(),
            });
            self.request_forecast()
        } else if self.settings.location.len() >= 2 {
            let url = format!(
                "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=en&format=json",
                encode_query(&self.settings.location)
            );
            vec![self.http_request(RequestKind::Geocode, GEOCODE_REQUEST, url)]
        } else {
            if self.snapshot.is_none() {
                self.error = Some("Set a weather location".into());
            }
            self.loading = false;
            vec![invalidate_bar()]
        }
    }

    fn request_forecast(&mut self) -> Vec<Value> {
        let Some(location) = &self.location else {
            return Vec::new();
        };
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,weather_code,is_day&forecast_days=1&timezone=auto&temperature_unit={}",
            location.latitude,
            location.longitude,
            self.settings.temperature_unit.query_value()
        );
        vec![self.http_request(RequestKind::Forecast, FORECAST_REQUEST, url)]
    }

    fn handle_http_response(
        &mut self,
        request_id: &str,
        status: Option<u16>,
        body: &str,
        transport_error: Option<String>,
    ) -> Vec<Value> {
        let Some((active_id, request_kind)) = self.active_request.as_ref() else {
            return Vec::new();
        };
        if active_id != request_id {
            return Vec::new();
        }
        let request_kind = *request_kind;
        self.active_request = None;
        self.loading = false;
        if let Some(error) = transport_error {
            return self.fail(error);
        }
        if !status.is_some_and(|status| (200..300).contains(&status)) {
            return self.fail(format!(
                "weather provider returned status {}",
                status.unwrap_or(0)
            ));
        }
        match request_kind {
            RequestKind::Geocode => match parse_location(body) {
                Ok(location) => {
                    self.location = Some(location);
                    self.request_forecast()
                }
                Err(error) => self.fail(error),
            },
            RequestKind::IpGeocode => match parse_ip_location(body) {
                Ok(location) => {
                    self.location = Some(location);
                    self.request_forecast()
                }
                Err(_error) => self.refresh_manual_or_fallback(),
            },
            RequestKind::Forecast => match parse_forecast(body, self.settings.temperature_unit) {
                Ok(snapshot) => {
                    self.snapshot = Some(snapshot);
                    self.error = None;
                    self.minutes_since_refresh = 0;
                    vec![invalidate_bar()]
                }
                Err(error) => self.fail(error),
            },
        }
    }

    fn http_request(&mut self, kind: RequestKind, prefix: &str, url: String) -> Value {
        self.request_serial = self.request_serial.wrapping_add(1);
        let request_id = format!("{prefix}-{}", self.request_serial);
        self.active_request = Some((request_id.clone(), kind));
        self.loading = true;
        http_request(&request_id, url)
    }

    fn fail(&mut self, error: String) -> Vec<Value> {
        self.error = Some(error);
        vec![invalidate_bar()]
    }

    fn view(&self) -> Value {
        let (icon, temperature, condition) = if let Some(snapshot) = &self.snapshot {
            (
                weather_icon(snapshot.weather_code, snapshot.is_day),
                format!("{:.0}{}", snapshot.temperature, snapshot.unit),
                weather_description(snapshot.weather_code),
            )
        } else if self.loading {
            ("", String::new(), "")
        } else if self.error.is_some() {
            ("warning", "Weather".into(), "")
        } else {
            ("cloud", "Weather".into(), "")
        };
        let mut children = vec![if self.loading {
            json!({
                "kind": "loading_indicator",
                "size": 24.0,
                "color": "on_surface_variant",
                "style": null
            })
        } else {
            json!({
                "kind": "icon",
                "name": icon,
                "size": 16.0,
                "style": {
                    "padding": null,
                    "margin": null,
                    "width": null,
                    "height": null,
                    "corner_radius": null,
                    "opacity": null,
                    "color": "on_surface_variant",
                    "background": null,
                    "flex_grow": null
                }
            })
        }];
        if !temperature.is_empty() {
            children.push(json!({
                "kind": "text",
                "content": temperature,
                "font_size": 13.0,
                "bold": true,
                "style": {
                    "padding": null,
                    "margin": null,
                    "width": null,
                    "height": null,
                    "corner_radius": null,
                    "opacity": null,
                    "color": "on_surface",
                    "background": null,
                    "flex_grow": null
                }
            }));
        }
        if self.settings.show_condition && !condition.is_empty() {
            children.push(json!({
                "kind": "text",
                "content": condition,
                "font_size": 12.0,
                "bold": false,
                "style": {
                    "padding": null,
                    "margin": null,
                    "width": null,
                    "height": null,
                    "corner_radius": null,
                    "opacity": 0.75,
                    "color": "on_surface_variant",
                    "background": null,
                    "flex_grow": null
                }
            }));
        }
        json!({
            "root": {
                "kind": "container",
                "direction": "row",
                "children": children,
                "style": null,
                "gap": 5.0
            }
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Event {
    ContributionSettingsChanged {
        contribution_id: String,
        settings: Value,
    },
    TimerFired {
        name: String,
    },
    NetworkChanged {
        connected: bool,
    },
    HttpResponse {
        request_id: String,
        status: Option<u16>,
        body: String,
        error: Option<String>,
    },
    LocationResponse {
        #[serde(default)]
        latitude: Option<f64>,
        #[serde(default)]
        longitude: Option<f64>,
        #[allow(dead_code)]
        #[serde(default)]
        accuracy_meters: Option<f64>,
        #[allow(dead_code)]
        #[serde(default)]
        error: Option<String>,
    },
    Input {
        contribution_id: String,
        event_id: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<GeocodingResult>,
}

#[derive(Deserialize)]
struct GeocodingResult {
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: CurrentWeather,
    #[serde(default)]
    current_units: CurrentUnits,
}

#[derive(Deserialize)]
struct CurrentWeather {
    temperature_2m: f64,
    weather_code: i32,
    is_day: i32,
}

#[derive(Default, Deserialize)]
struct CurrentUnits {
    temperature_2m: Option<String>,
}

fn parse_location(body: &str) -> Result<Location, String> {
    let response: GeocodingResponse = serde_json::from_str(body)
        .map_err(|error| format!("invalid location response: {error}"))?;
    let result = response
        .results
        .into_iter()
        .next()
        .ok_or_else(|| "location was not found".to_owned())?;
    Ok(Location {
        latitude: result.latitude,
        longitude: result.longitude,
    })
}

#[derive(Deserialize)]
struct IpLocationResponse {
    success: bool,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    message: Option<String>,
}

fn parse_ip_location(body: &str) -> Result<Location, String> {
    let response: IpLocationResponse = serde_json::from_str(body)
        .map_err(|error| format!("invalid ip location response: {error}"))?;
    if !response.success {
        return Err(response
            .message
            .unwrap_or_else(|| "IP geolocation failed".into()));
    }
    Ok(Location {
        latitude: response.latitude,
        longitude: response.longitude,
    })
}

fn parse_forecast(body: &str, unit: TemperatureUnit) -> Result<WeatherSnapshot, String> {
    let response: ForecastResponse = serde_json::from_str(body)
        .map_err(|error| format!("invalid forecast response: {error}"))?;
    Ok(WeatherSnapshot {
        temperature: response.current.temperature_2m,
        unit: response
            .current_units
            .temperature_2m
            .unwrap_or_else(|| unit.fallback_symbol().into()),
        weather_code: response.current.weather_code,
        is_day: response.current.is_day != 0,
    })
}

fn http_request(request_id: &str, url: String) -> Value {
    json!({
        "kind": "http_request",
        "request_id": request_id,
        "url": url,
        "method": "GET"
    })
}

fn invalidate_bar() -> Value {
    json!({
        "kind": "invalidate_view",
        "contribution_id": "bar"
    })
}

fn encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn weather_icon(code: i32, is_day: bool) -> &'static str {
    match code {
        0 => {
            if is_day {
                "clear_day"
            } else {
                "bedtime"
            }
        }
        1 | 2 => {
            if is_day {
                "partly_cloudy_day"
            } else {
                "partly_cloudy_night"
            }
        }
        3 => "cloud",
        45 | 48 => "foggy",
        51..=55 => "rainy_light",
        56..=57 | 66..=67 => "rainy_snow",
        61..=63 | 80..=81 => "rainy",
        65 | 82 => "rainy_heavy",
        71..=77 => "weather_snowy",
        85..=86 => "snowing_heavy",
        95 => "thunderstorm",
        96 | 99 => "weather_hail",
        _ => "cloud_alert",
    }
}

fn weather_description(code: i32) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=67 => "Drizzle",
        71..=77 => "Snow",
        80..=82 => "Showers",
        85..=86 => "Snow showers",
        95..=99 => "Thunderstorm",
        _ => "Weather",
    }
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::{Event, WeatherState};
    use std::cell::RefCell;

    wit_bindgen::generate!({
        path: "../../crates/ext/wit",
        world: "extension",
    });

    thread_local! {
        static STATE: RefCell<WeatherState> = RefCell::new(WeatherState::default());
    }

    struct WeatherExtension;

    impl Guest for WeatherExtension {
        fn on_event(event_json: String) -> String {
            let Ok(event) = serde_json::from_str::<Event>(&event_json) else {
                return "[]".into();
            };
            STATE.with(|state| {
                serde_json::to_string(&state.borrow_mut().handle_event(event))
                    .unwrap_or_else(|_| "[]".into())
            })
        }

        fn view(contribution_id: String) -> String {
            if contribution_id != "bar" {
                return "null".into();
            }
            STATE.with(|state| state.borrow().view().to_string())
        }
    }

    export!(WeatherExtension);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(location: &str) -> Event {
        Event::ContributionSettingsChanged {
            contribution_id: "bar".into(),
            settings: json!({
                "location_mode": "manual",
                "location": location,
                "temperature_unit": "celsius",
                "refresh_minutes": 30,
                "show_condition": true
            }),
        }
    }

    #[test]
    fn automatic_mode_emits_location_read_and_handles_location_response() {
        let mut state = WeatherState::default();
        let effects = state.handle_event(Event::ContributionSettingsChanged {
            contribution_id: "bar".into(),
            settings: json!({
                "location_mode": "automatic"
            }),
        });
        assert_eq!(effects, vec![json!({ "kind": "location_read" })]);

        let effects = state.handle_event(Event::LocationResponse {
            latitude: Some(22.5726),
            longitude: Some(88.3639),
            accuracy_meters: Some(500.0),
            error: None,
        });
        let forecast_request = effects[0]["request_id"].as_str().unwrap().to_owned();
        assert!(forecast_request.starts_with(FORECAST_REQUEST));
        assert!(effects[0]["url"]
            .as_str()
            .unwrap()
            .contains("latitude=22.5726"));
    }

    #[test]
    fn ip_mode_fetches_ip_geocoding() {
        let mut state = WeatherState::default();
        let effects = state.handle_event(Event::ContributionSettingsChanged {
            contribution_id: "bar".into(),
            settings: json!({
                "location_mode": "ip"
            }),
        });
        let ip_request = effects[0]["request_id"].as_str().unwrap().to_owned();
        assert!(ip_request.starts_with(IP_GEOCODE_REQUEST));
        assert!(effects[0]["url"].as_str().unwrap().contains("ipwho.is"));

        let effects = state.handle_event(Event::HttpResponse {
            request_id: ip_request,
            status: Some(200),
            body: json!({
                "success": true,
                "latitude": 22.5726,
                "longitude": 88.3639
            })
            .to_string(),
            error: None,
        });
        let forecast_request = effects[0]["request_id"].as_str().unwrap().to_owned();
        assert!(forecast_request.starts_with(FORECAST_REQUEST));
        assert!(effects[0]["url"]
            .as_str()
            .unwrap()
            .contains("latitude=22.5726"));
    }

    #[test]
    fn city_settings_geocode_then_fetch_forecast() {
        let mut state = WeatherState::default();
        let effects = state.handle_event(settings("Kolkata"));
        let geocode_request = effects[0]["request_id"].as_str().unwrap().to_owned();
        assert!(geocode_request.starts_with(GEOCODE_REQUEST));
        assert!(effects[0]["url"].as_str().unwrap().contains("Kolkata"));
        assert!(state.view().to_string().contains("loading_indicator"));

        let effects = state.handle_event(Event::HttpResponse {
            request_id: geocode_request,
            status: Some(200),
            body: json!({
                "results": [{
                    "name": "Kolkata",
                    "latitude": 22.5726,
                    "longitude": 88.3639,
                    "country": "India",
                    "admin1": "West Bengal"
                }]
            })
            .to_string(),
            error: None,
        });
        let forecast_request = effects[0]["request_id"].as_str().unwrap().to_owned();
        assert!(forecast_request.starts_with(FORECAST_REQUEST));
        assert!(effects[0]["url"]
            .as_str()
            .unwrap()
            .contains("latitude=22.5726"));

        let effects = state.handle_event(Event::HttpResponse {
            request_id: forecast_request,
            status: Some(200),
            body: json!({
                "current": {
                    "temperature_2m": 29.4,
                    "weather_code": 0,
                    "is_day": 1
                },
                "current_units": {
                    "temperature_2m": "°C"
                }
            })
            .to_string(),
            error: None,
        });
        assert_eq!(effects, vec![invalidate_bar()]);
        let view = state.view().to_string();
        assert!(view.contains("29°C"));
        assert!(view.contains("Clear"));
    }

    #[test]
    fn coordinates_skip_geocoding() {
        let mut state = WeatherState::default();
        let effects = state.handle_event(Event::ContributionSettingsChanged {
            contribution_id: "bar".into(),
            settings: json!({
                "location_mode": "manual",
                "location": "Kolkata",
                "latitude": 22.5726,
                "longitude": 88.3639
            }),
        });
        assert!(effects[0]["request_id"]
            .as_str()
            .unwrap()
            .starts_with(FORECAST_REQUEST));
    }

    #[test]
    fn refresh_interval_is_clamped_and_tick_driven() {
        let mut state = WeatherState::default();
        let _ = state.handle_event(Event::ContributionSettingsChanged {
            contribution_id: "bar".into(),
            settings: json!({
                "location_mode": "manual",
                "latitude": 1.0,
                "longitude": 2.0,
                "refresh_minutes": 1
            }),
        });
        state.loading = false;
        state.minutes_since_refresh = 14;
        let effects = state.handle_event(Event::TimerFired {
            name: "minute".into(),
        });
        assert!(effects[0]["request_id"]
            .as_str()
            .unwrap()
            .starts_with(FORECAST_REQUEST));
    }

    #[test]
    fn stale_responses_do_not_replace_newer_settings() {
        let mut state = WeatherState::default();
        let first = state.handle_event(settings("Kolkata"))[0]["request_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let second = state.handle_event(settings("London"))[0]["request_id"]
            .as_str()
            .unwrap()
            .to_owned();

        assert_ne!(first, second);
        let effects = state.handle_event(Event::HttpResponse {
            request_id: first,
            status: Some(200),
            body: json!({
                "results": [{"latitude": 22.5726, "longitude": 88.3639}]
            })
            .to_string(),
            error: None,
        });
        assert!(effects.is_empty());
        assert!(state.location.is_none());
        assert_eq!(
            state.active_request.as_ref().map(|(id, _)| id.as_str()),
            Some(second.as_str())
        );
    }

    #[test]
    fn weather_codes_have_stable_fallbacks() {
        assert_eq!(weather_icon(0, true), "clear_day");
        assert_eq!(weather_icon(0, false), "bedtime");
        assert_eq!(weather_icon(53, true), "rainy_light");
        assert_eq!(weather_icon(63, true), "rainy");
        assert_eq!(weather_icon(95, true), "thunderstorm");
        assert_eq!(weather_description(95), "Thunderstorm");
        assert_eq!(weather_icon(999, true), "cloud_alert");
        assert_eq!(weather_description(999), "Weather");
    }

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(encode_query("São Paulo"), "S%C3%A3o%20Paulo");
    }
}
