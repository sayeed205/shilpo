import { badge, buildViewTree, button, column, row, spacer, text } from "@shilpo/ext-sdk";
import type { ViewTree } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderBarMenu(state: ShowcaseState): ViewTree {
  return buildViewTree(
    column({
      gap: 8,
      style: { padding: 12 },
      children: [
        row({
          alignItems: "center",
          justifyContent: "space-between",
          children: [
            text("Showcase Menu", { bold: true, fontSize: 14 }),
            badge(state.mode, {
              style: { color: state.mode === "active" ? "primary" : "outline" },
            }),
          ],
        }),
        text(`Total clicks: ${state.clicks}`),
        text(`Last background sync: ${state.lastSyncIso.substring(11, 19)}`),
        spacer(4),
        row({
          gap: 6,
          children: [
            button("Toggle Mode", "btn-menu-toggle", { style: { padding: 6 } }),
            button("Copy Status", "btn-menu-copy", { style: { padding: 6 } }),
            button("Refresh Demo", "btn-menu-refresh", { style: { padding: 6 } }),
          ],
        }),
      ],
    }),
  );
}
