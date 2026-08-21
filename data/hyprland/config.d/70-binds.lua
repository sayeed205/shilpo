-- ─────────────────────────────────────────────────────────────────────────────
-- 70 — Key bindings
-- ─────────────────────────────────────────────────────────────────────────────
-- Mirrors data/niri/config.d/70-binds.kdl. Dispatchers without a confirmed native `hl.dsp`
-- equivalent fall back to `hyprctl dispatch`, which is stable across Hyprland versions.

local mod = "SUPER"

-- Session & compositor
hl.bind(mod .. " + SHIFT + E", hl.dsp.exec_cmd("hyprctl dispatch exit"))
hl.bind(mod .. " + SHIFT + O", hl.dsp.exec_cmd("hyprctl dispatch dpms off"))

-- Shilpo shell overlays & controls
hl.bind(mod .. " + SPACE", hl.dsp.exec_cmd("@SHILPO_BIN@ overview toggle"))
hl.bind(mod .. " + COMMA", hl.dsp.exec_cmd("@SHILPO_BIN@ settings"))
hl.bind("CTRL + ALT + T", hl.dsp.exec_cmd("@SHILPO_BIN@ theme wallpaper random"))
hl.bind(mod .. " + ALT + L", hl.dsp.exec_cmd("swaylock"), { locked = true })

-- App launchers
hl.bind(mod .. " + T", hl.dsp.exec_cmd("kitty"))
hl.bind(mod .. " + RETURN", hl.dsp.exec_cmd("kitty"))
hl.bind("SUPER + E", hl.dsp.exec_cmd("nautilus"))

-- Window management
hl.bind(mod .. " + Q", hl.dsp.window.close())
hl.bind(mod .. " + D", hl.dsp.exec_cmd("hyprctl dispatch fullscreen 1"))
hl.bind(mod .. " + F", hl.dsp.exec_cmd("hyprctl dispatch fullscreen 0"))
hl.bind(mod .. " + A", hl.dsp.window.float({ action = "toggle" }))

-- Focus movement
hl.bind(mod .. " + left", hl.dsp.focus({ direction = "left" }))
hl.bind(mod .. " + down", hl.dsp.focus({ direction = "down" }))
hl.bind(mod .. " + up", hl.dsp.focus({ direction = "up" }))
hl.bind(mod .. " + right", hl.dsp.focus({ direction = "right" }))
hl.bind(mod .. " + H", hl.dsp.focus({ direction = "left" }))
hl.bind(mod .. " + J", hl.dsp.focus({ direction = "down" }))
hl.bind(mod .. " + K", hl.dsp.focus({ direction = "up" }))
hl.bind(mod .. " + L", hl.dsp.focus({ direction = "right" }))

-- Window movement
hl.bind(mod .. " + SHIFT + left", hl.dsp.exec_cmd("hyprctl dispatch movewindow l"))
hl.bind(mod .. " + SHIFT + down", hl.dsp.exec_cmd("hyprctl dispatch movewindow d"))
hl.bind(mod .. " + SHIFT + up", hl.dsp.exec_cmd("hyprctl dispatch movewindow u"))
hl.bind(mod .. " + SHIFT + right", hl.dsp.exec_cmd("hyprctl dispatch movewindow r"))
hl.bind(mod .. " + SHIFT + H", hl.dsp.exec_cmd("hyprctl dispatch movewindow l"))
hl.bind(mod .. " + SHIFT + J", hl.dsp.exec_cmd("hyprctl dispatch movewindow d"))
hl.bind(mod .. " + SHIFT + K", hl.dsp.exec_cmd("hyprctl dispatch movewindow u"))
hl.bind(mod .. " + SHIFT + L", hl.dsp.exec_cmd("hyprctl dispatch movewindow r"))

-- Workspace navigation
for i = 1, 9 do
    hl.bind(mod .. " + " .. i, hl.dsp.focus({ workspace = i }))
    hl.bind(mod .. " + SHIFT + " .. i, hl.dsp.window.move({ workspace = i }))
end

hl.bind(mod .. " + page_down", hl.dsp.focus({ workspace = "e+1" }))
hl.bind(mod .. " + page_up", hl.dsp.focus({ workspace = "e-1" }))
hl.bind(mod .. " + U", hl.dsp.focus({ workspace = "e+1" }))
hl.bind(mod .. " + I", hl.dsp.focus({ workspace = "e-1" }))

hl.bind(mod .. " + SHIFT + page_down", hl.dsp.window.move({ workspace = "e+1" }))
hl.bind(mod .. " + SHIFT + page_up", hl.dsp.window.move({ workspace = "e-1" }))
hl.bind(mod .. " + SHIFT + U", hl.dsp.window.move({ workspace = "e+1" }))
hl.bind(mod .. " + SHIFT + I", hl.dsp.window.move({ workspace = "e-1" }))

-- Media & hardware keys (locked = usable while the session is locked)
hl.bind("XF86AudioRaiseVolume", hl.dsp.exec_cmd("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+"), { locked = true, repeating = true })
hl.bind("XF86AudioLowerVolume", hl.dsp.exec_cmd("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-"), { locked = true, repeating = true })
hl.bind("XF86AudioMute", hl.dsp.exec_cmd("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"), { locked = true })
hl.bind("XF86AudioMicMute", hl.dsp.exec_cmd("wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"), { locked = true })
hl.bind("XF86MonBrightnessUp", hl.dsp.exec_cmd("brightnessctl set 5%+"), { locked = true, repeating = true })
hl.bind("XF86MonBrightnessDown", hl.dsp.exec_cmd("brightnessctl set 5%-"), { locked = true, repeating = true })
hl.bind("XF86AudioPlay", hl.dsp.exec_cmd("playerctl play-pause"), { locked = true })
hl.bind("XF86AudioNext", hl.dsp.exec_cmd("playerctl next"), { locked = true })
hl.bind("XF86AudioPrev", hl.dsp.exec_cmd("playerctl previous"), { locked = true })

-- Screenshots & capture
hl.bind("SUPER + SHIFT + S", hl.dsp.exec_cmd("@SHILPO_BIN@ capture region"))
hl.bind("SUPER + SHIFT + R", hl.dsp.exec_cmd("@SHILPO_BIN@ record toggle"))
hl.bind("Print", hl.dsp.exec_cmd("@SHILPO_BIN@ capture screen"))
hl.bind(mod .. " + Print", hl.dsp.exec_cmd("@SHILPO_BIN@ capture screen"))
hl.bind("ALT + Print", hl.dsp.exec_cmd("@SHILPO_BIN@ capture window"))
