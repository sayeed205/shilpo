# Weather

The official Weather extension adds `ext:org.shilpo.weather/bar`. It uses Open-Meteo's geocoding and forecast endpoints
through Shilpo's capability-checked HTTP transport. The guest has no direct network, filesystem, environment, or
location access.

Location is explicit: configure a city/postal code or exact coordinates. The extension does not infer location from the
public IP address. Exact coordinates avoid geocoding and take precedence over `location`.

## Build and run

Install the WASI Preview 2 target once:

```bash
rustup target add wasm32-wasip2
```

From the Shilpo repository:

```bash
cargo build --manifest-path extensions/Cargo.toml \
  -p shilpo-weather-extension --target wasm32-wasip2 --release
cp extensions/target/wasm32-wasip2/release/shilpo_weather_extension.wasm \
  extensions/weather/extension.wasm
cargo run -p shilpo-shell -- ext check extensions/weather
cargo run -p shilpo-shell -- ext dev "$(pwd)/extensions/weather"
```

Development mode grants the manifest's declared capabilities. Installed packages start disabled and require permission
review before the network capability is granted.

## Package and install

Build the component first, then create and install the same package an end user receives:

```bash
cargo run -p shilpo-shell -- ext pack extensions/weather \
  --output extensions/target/packages
cargo run -p shilpo-shell -- ext install \
  extensions/target/packages/org.shilpo.weather-1.0.0.shilpo-ext
cargo run -p shilpo-shell -- ext approve org.shilpo.weather --grant-all
cargo run -p shilpo-shell -- ext enable org.shilpo.weather
```

An official release signs and publishes that package through Shilpo's official registry. Source location alone never
marks a locally built package as official or grants its capabilities.

## Configure

Add the widget to the existing `[bar.widgets]` section in `~/.config/shilpo/config.toml`; do not create a second section
with the same name. Add the extension-wide settings separately:

```toml
[bar.widgets]
end = [
    "builtin:network",
    "builtin:audio",
    "builtin:battery",
    "ext:org.shilpo.weather/bar",
]

[extensions.settings."org.shilpo.weather"]
location = "Kolkata"
temperature_unit = "celsius"
refresh_minutes = 30
show_condition = false
```

Coordinates may be used instead:

```toml
[extensions.settings."org.shilpo.weather"]
location = "Kolkata"
latitude = 22.5726
longitude = 88.3639
temperature_unit = "celsius"
```

Reload a running shell after editing the configuration:

```bash
./target/release/shilpo-shell msg reload-config
```

When the shell was built with the default development profile, use `./target/debug/shilpo-shell` instead.

## Development loop

After changing the guest source, rebuild and copy the component, then tell the running shell to replace the registered
generation:

```bash
cargo build --manifest-path extensions/Cargo.toml \
  -p shilpo-weather-extension --target wasm32-wasip2 --release
cp extensions/target/wasm32-wasip2/release/shilpo_weather_extension.wasm \
  extensions/weather/extension.wasm
./target/release/shilpo-shell ext reload org.shilpo.weather
```

The shell does not need to restart. Run `ext dev` from the build section once to register the development path; later
iterations only need `ext reload`. If no release binary exists, the final command can instead be run as:

```bash
cargo run -p shilpo-shell -- ext reload org.shilpo.weather
```

The widget normally moves through these states:

- `◌ Loading` while location or forecast data is being fetched;
- a weather symbol and temperature after a successful response;
- `! Weather` when the location is missing or after a network, provider, or response error;
- `○ Weather` briefly before the first settings event is delivered.

Inspect registration state and follow extension diagnostics with:

```bash
./target/release/shilpo-shell ext list
./target/release/shilpo-shell ext logs org.shilpo.weather --follow
```

Weather data is provided by [Open-Meteo](https://open-meteo.com/). Location search data is based on GeoNames through
Open-Meteo's geocoding endpoint.
