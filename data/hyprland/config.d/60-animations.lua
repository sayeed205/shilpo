-- ─────────────────────────────────────────────────────────────────────────────
-- 60 — Animations
-- ─────────────────────────────────────────────────────────────────────────────

hl.curve("shilpoEase", { type = "bezier", points = { { 0.05, 0.9 }, { 0.1, 1.05 } } })
hl.animation({ leaf = "windows", enabled = true, speed = 4, bezier = "shilpoEase" })
hl.animation({ leaf = "fade", enabled = true, speed = 4, bezier = "shilpoEase" })
hl.animation({ leaf = "workspaces", enabled = true, speed = 4, bezier = "shilpoEase", style = "slidevert" })
