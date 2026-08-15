import { buildViewTree, column, row, spacer, text, textInput, toggle } from "@shilpo/ext-sdk";
import type { ViewTree } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderSettingsPage(state: ShowcaseState): ViewTree {
  return buildViewTree(
    column({
      gap: 12,
      style: { padding: 16 },
      children: [
        text("Showcase Preferences", { bold: true, fontSize: 18 }),
        text("Configure options and live parameters for the showcase extension:"),
        spacer(8),
        row({
          alignItems: "center",
          justifyContent: "space-between",
          children: [
            text("Desktop Notifications:"),
            toggle(state.notificationsEnabled, "tog-notifications"),
          ],
        }),
        spacer(6),
        column({
          gap: 4,
          children: [
            text("Custom Accent Label:"),
            textInput("input-label", state.accentLabel, {
              placeholder: "Enter label (e.g. Active, Work, Focus)",
            }),
          ],
        }),
      ],
    }),
  );
}
