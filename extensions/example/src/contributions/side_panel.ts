import { badge, buildViewTree, button, column, icon, row, spacer, text } from "@shilpo/ext-sdk";
import type { ViewTree } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderSidePanel(state: ShowcaseState): ViewTree {
  const logNodes = state.logs.slice(0, 8).map((log) => text(log, { fontSize: 12 }));

  return buildViewTree(
    column({
      gap: 10,
      style: { padding: 16 },
      children: [
        row({
          alignItems: "center",
          gap: 8,
          children: [
            icon("view_sidebar", { size: 20 }),
            text("Showcase Side Panel", { bold: true, fontSize: 16 }),
          ],
        }),
        row({
          alignItems: "center",
          justifyContent: "space-between",
          children: [
            text("Current Mode:"),
            badge(state.mode),
          ],
        }),
        text(`Registered Clicks: ${state.clicks}`),
        text(`Last Sync: ${state.lastSyncIso}`),
        spacer(6),
        row({
          gap: 8,
          children: [
            button("Increment", "btn-panel-increment", { style: { padding: 6 } }),
            button("Clear Logs", "btn-panel-clear-logs", { style: { padding: 6 } }),
          ],
        }),
        spacer(8),
        text("Recent Activity Log:", { bold: true }),
        ...logNodes,
      ],
    }),
  );
}
