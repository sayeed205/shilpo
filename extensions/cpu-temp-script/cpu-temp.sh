#!/usr/bin/env bash
# Deterministic, bounded Trusted Local Script for CPU temperature bar widget.
# Emits a JSON-formatted Shilpo ScriptRecord to stdout.
set -euo pipefail

temp_c=48
if [[ -f /sys/class/thermal/thermal_zone0/temp ]]; then
    raw_temp=$(cat /sys/class/thermal/thermal_zone0/temp 2>/dev/null || echo "48000")
    if [[ "$raw_temp" =~ ^[0-9]+$ ]] && [[ "$raw_temp" -gt 0 ]]; then
        temp_c=$(( raw_temp / 1000 ))
    fi
fi

cat <<EOF
{"schema_version":1,"contribution":"cpu-temp","kind":"text","text":"${temp_c}°C","tooltip":"CPU Package Temperature: ${temp_c}°C","icon":"device_thermostat"}
EOF
