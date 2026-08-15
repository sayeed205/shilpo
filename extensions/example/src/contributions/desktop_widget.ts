import { buildViewTree, button, column, icon, progress, row, spacer, text } from "@shilpo/ext-sdk";
import type { ViewTree } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderDesktopWidget(state: ShowcaseState): ViewTree {
  const progressRatio = Math.min(1.0, (state.clicks % 20) / 20.0);
  return buildViewTree(
    column({
      gap: 10,
      style: { padding: 16 },
      children: [
        row({
          alignItems: "center",
          gap: 8,
          children: [
            icon("dashboard", { size: 20 }),
            text("Showcase Desktop Card", { bold: true, fontSize: 16 }),
          ],
        }),
        text(`Status: ${state.accentLabel} • Mode: ${state.mode}`),
        progress(progressRatio),
        text(`Clicks toward milestone: ${state.clicks % 20}/20`),
        spacer(6),
        row({
          gap: 8,
          children: [
            button("Increment", "btn-desktop-increment", { style: { padding: 8 } }),
            button("Toggle Mode", "btn-desktop-toggle", { style: { padding: 8 } }),
          ],
        }),
      ],
    }),
  );
}
