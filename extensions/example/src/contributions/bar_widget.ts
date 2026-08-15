import { buildViewTree, button, icon, row, text } from "@shilpo/ext-sdk";
import type { ViewTree } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderBarWidget(state: ShowcaseState): ViewTree {
  const iconName = state.mode === "active" ? "stars" : "bedtime";
  return buildViewTree(
    row({
      gap: 6,
      alignItems: "center",
      children: [
        icon(iconName, { size: 16 }),
        text(`Showcase: ${state.accentLabel} (${state.clicks})`, { bold: true }),
        button("Click", "btn-bar-increment", {
          style: { padding: 4 },
        }),
      ],
    }),
  );
}
