-- ─────────────────────────────────────────────────────────────────────────────
-- 50 — Processes spawned at login
-- ─────────────────────────────────────────────────────────────────────────────
-- Shilpo's own daemons and session helpers are systemd user units grouped under
-- shilpo-session.target — starting that one target here pulls in all of them,
-- with systemd handling crash restart/rate-limiting. See data/systemd/user/.

hl.on("hyprland.start", function()
    hl.exec_cmd("systemctl --user start shilpo-session.target")
end)
