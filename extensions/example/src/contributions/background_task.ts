import { DataValue } from "@shilpo/ext-sdk";
import type { HostFacade } from "@shilpo/ext-sdk";
import type { ShowcaseStateStore } from "../state.ts";

export function handleBackgroundTask(
  taskId: string,
  store: ShowcaseStateStore,
  host?: HostFacade,
): void {
  if (taskId === "sync-task") {
    store.recordSync();
    if (host?.state) {
      try {
        host.state.write("last_sync", DataValue.text(store.snapshot.lastSyncIso));
      } catch {
        // Degraded fallback: State stored safely in-memory
      }
    }
  }
}
