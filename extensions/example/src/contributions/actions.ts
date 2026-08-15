import type { HostFacade } from "@shilpo/ext-sdk";
import type { ShowcaseStateStore } from "../state.ts";

export function handleAction(
  actionId: string,
  store: ShowcaseStateStore,
  host?: HostFacade,
): void {
  if (actionId === "toggle-power") {
    const newMode = store.toggleMode();
    if (store.snapshot.notificationsEnabled && host?.notifications) {
      try {
        host.notifications.show({
          title: "Showcase Mode Changed",
          body: `Showcase is now in ${newMode} mode.`,
        });
      } catch {
        // Notification failure handled safely without crashing
      }
    }
  }
}
