-- Shilpo — recommended Hyprland configuration.
-- Hyprland deprecated the classic hyprland.conf keyfile format in 0.55 in favor of this
-- native Lua config (the `hl` global API). See https://wiki.hypr.land/Configuring/Start/
--
-- Split into modular files under config.d/, mirroring Niri's config.d/ layout, so you can
-- edit individual sections without touching the rest:
--
--   10  Input, touchpad, gestures, cursor
--   20  Layout, gaps, borders, decoration
--   30  Window rules
--   40  Environment variables
--   50  Processes spawned at login
--   60  Animations
--   70  Key bindings (Shilpo shell + window management)
--   80  Layer rules
--   90  Your personal overrides (never touched by `shilpo setup`)

hl.monitor({
    output = "",
    mode = "preferred",
    position = "auto",
    scale = 1,
})

local home = os.getenv("HOME")
local config_dir = home .. "/.config/hypr/config.d"

for _, name in ipairs({
    "10-input-and-cursor",
    "20-layout-and-overview",
    "30-window-rules",
    "40-environment",
    "50-startup",
    "60-animations",
    "70-binds",
    "80-layer-rules",
}) do
    dofile(config_dir .. "/" .. name .. ".lua")
end

-- `shilpo setup` never overwrites shilpo-user-extra.lua once it exists.
local user_extra = home .. "/.config/hypr/shilpo-user-extra.lua"
local f = io.open(user_extra, "r")
if f then
    f:close()
    dofile(user_extra)
end
