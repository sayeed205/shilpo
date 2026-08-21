-- ─────────────────────────────────────────────────────────────────────────────
-- 10 — Input devices, gestures & cursor
-- ─────────────────────────────────────────────────────────────────────────────

hl.config({
    input = {
        kb_layout = "us",
        follow_mouse = 1,

        touchpad = {
            natural_scroll = true,
        },
    },
})

hl.env("XCURSOR_THEME", "capitaine-cursors")
hl.env("XCURSOR_SIZE", "24")

-- Touchpad gestures
hl.gesture({ fingers = 3, direction = "horizontal", action = "scroll_move" })
hl.gesture({ fingers = 3, direction = "vertical", action = "workspace" })
hl.gesture({ fingers = 3, direction = "pinch", action = "fullscreen" })
hl.gesture({
    fingers = 4,
    direction = "up",
    action = function()
        hl.dispatch(hl.dsp.exec_cmd("@SHILPO_BIN@ overview toggle"))
    end,
})
hl.gesture({
    fingers = 4,
    direction = "down",
    action = function()
        hl.dispatch(hl.dsp.exec_cmd("@SHILPO_BIN@ overview toggle"))
    end,
})
