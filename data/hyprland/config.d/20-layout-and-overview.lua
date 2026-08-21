-- ─────────────────────────────────────────────────────────────────────────────
-- 20 — Layout, gaps, borders, decoration
-- ─────────────────────────────────────────────────────────────────────────────

hl.config({
    general = {
        gaps_in = 4,
        gaps_out = 8,
        border_size = 2,
        layout = "scrolling",
    },

    scrolling = {
        fullscreen_on_one_column = true,
    },

    decoration = {
        rounding = 8,
        blur = {
            enabled = true,
            size = 4,
            passes = 2,
        },
    },

    animations = {
        enabled = true,
    },
})
