import type { HostFacade } from "@shilpo/ext-sdk";
import type { ShowcaseStateStore } from "../state.ts";
import { handleAction } from "./actions.ts";

export function handleShortcut(
  shortcutId: string,
  store: ShowcaseStateStore,
  host?: HostFacade,
): void {
  if (shortcutId === "shortcut-toggle") {
    handleAction("toggle-power", store, host);
  }
}
