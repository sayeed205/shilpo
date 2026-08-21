-- ─────────────────────────────────────────────────────────────────────────────
-- 30 — Window rules
-- ─────────────────────────────────────────────────────────────────────────────

hl.window_rule({
    name = "shilpo-settings-float",
    match = { class = "^(shilpo-settings)$" },
    float = true,
})

hl.window_rule({
    name = "pavucontrol-float",
    match = { class = "^(pavucontrol)$" },
    float = true,
})

-- Fixes dragging/focus issues with borderless XWayland popups (context menus, etc.)
hl.window_rule({
    name = "fix-xwayland-drags",
    match = {
        class = "^$",
        title = "^$",
        xwayland = true,
        float = true,
        fullscreen = false,
        pin = false,
    },
    no_focus = true,
})

hl.window_rule({
    name = "file-dialog-float",
    match = { title = "^(Open File|Select a File|Choose wallpaper)(.*)$" },
    float = true,
})
