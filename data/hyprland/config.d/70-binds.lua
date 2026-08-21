-- ─────────────────────────────────────────────────────────────────────────────
-- 70 — Key bindings
-- ─────────────────────────────────────────────────────────────────────────────
-- Mirrors data/niri/config.d/70-binds.kdl, with the column-based ones (swapcol,
-- colresize, consume_or_expel, ...) ported from that same file's Niri "scrolling-tiler"
-- semantics onto Hyprland's scrolling layout (20-layout-and-overview.lua) via the
-- `hl.dsp.layout(msg)` messages documented at
-- https://wiki.hypr.land/Configuring/Layouts/Scrolling-Layout/#layout-messages -- these
-- have no niri equivalent to fall back to, so a few niri-only concepts (focus/move
-- column first/last, switch-focus-between-floating-and-tiling, keyboard-shortcuts-inhibit)
-- are left unbound rather than approximated with something that isn't actually equivalent.

local mod = "SUPER"

-- Session & compositor
hl.bind(mod .. " + SHIFT + E", hl.dsp.exit())
-- DPMS must not be dispatched synchronously from a bind (undefined behavior per
-- https://wiki.hypr.land/Configuring/Basics/Dispatchers/#dispatchers) -- deferred via a
-- one-shot timer, exactly as the wiki's own workaround shows.
hl.bind(mod .. " + SHIFT + O", function()
    hl.timer(function()
        hl.dispatch(hl.dsp.dpms({ action = "off" }))
    end, { timeout = 500, type = "oneshot" })
end)

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
hl.bind(mod .. " + D", hl.dsp.window.fullscreen({ mode = "maximized" }))
hl.bind(mod .. " + F", hl.dsp.window.fullscreen({ mode = "fullscreen", layout_aware = true }))
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
hl.bind(mod .. " + SHIFT + left", hl.dsp.window.move({ direction = "l" }))
hl.bind(mod .. " + SHIFT + down", hl.dsp.window.move({ direction = "d" }))
hl.bind(mod .. " + SHIFT + up", hl.dsp.window.move({ direction = "u" }))
hl.bind(mod .. " + SHIFT + right", hl.dsp.window.move({ direction = "r" }))
hl.bind(mod .. " + SHIFT + H", hl.dsp.window.move({ direction = "l" }))
hl.bind(mod .. " + SHIFT + J", hl.dsp.window.move({ direction = "d" }))
hl.bind(mod .. " + SHIFT + K", hl.dsp.window.move({ direction = "u" }))
hl.bind(mod .. " + SHIFT + L", hl.dsp.window.move({ direction = "r" }))

-- Column layout (scrolling-tiler model, matching niri's column bindings)
hl.bind(mod .. " + R", hl.dsp.layout("colresize +conf")) -- cycle preset column widths
hl.bind(mod .. " + Minus", hl.dsp.layout("colresize -0.1"))
hl.bind(mod .. " + Equal", hl.dsp.layout("colresize +0.1"))
hl.bind(mod .. " + BracketLeft", hl.dsp.layout("consume_or_expel prev"))
hl.bind(mod .. " + BracketRight", hl.dsp.layout("consume_or_expel next"))
hl.bind(mod .. " + C", hl.dsp.layout("fit_into_view")) -- center the focused column
hl.bind(mod .. " + CTRL + BracketLeft", hl.dsp.layout("swapcol l"))
hl.bind(mod .. " + CTRL + BracketRight", hl.dsp.layout("swapcol r"))

-- Multi-monitor
hl.bind(mod .. " + CTRL + left", hl.dsp.focus({ monitor = "l" }))
hl.bind(mod .. " + CTRL + right", hl.dsp.focus({ monitor = "r" }))
hl.bind(mod .. " + CTRL + up", hl.dsp.focus({ monitor = "u" }))
hl.bind(mod .. " + CTRL + down", hl.dsp.focus({ monitor = "d" }))
hl.bind(mod .. " + CTRL + SHIFT + left", hl.dsp.window.move({ monitor = "l" }))
hl.bind(mod .. " + CTRL + SHIFT + right", hl.dsp.window.move({ monitor = "r" }))
hl.bind(mod .. " + CTRL + SHIFT + up", hl.dsp.window.move({ monitor = "u" }))
hl.bind(mod .. " + CTRL + SHIFT + down", hl.dsp.window.move({ monitor = "d" }))

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

-- Mouse wheel workspace navigation
hl.bind(mod .. " + mouse_down", hl.dsp.focus({ workspace = "e+1" }))
hl.bind(mod .. " + mouse_up", hl.dsp.focus({ workspace = "e-1" }))

-- Alt-Tab window cycling
hl.bind("ALT + Tab", hl.dsp.window.cycle_next())

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
