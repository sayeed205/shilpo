import { DataValue, defineExtension } from "@shilpo/ext-sdk";
import type {
  Activation,
  DeactivateReason,
  ExtensionEvent,
  HostFacade,
  ViewElement,
} from "@shilpo/ext-sdk";

import { ShowcaseStateStore } from "./state.ts";
import { renderBarWidget } from "./contributions/bar_widget.tsx";
import { renderBarMenu } from "./contributions/bar_menu.tsx";
import { renderDesktopWidget } from "./contributions/desktop_widget.tsx";
import { renderSettingsPage } from "./contributions/settings_page.tsx";
import { renderSidePanel } from "./contributions/side_panel.tsx";
import { handleAction } from "./contributions/actions.ts";
import { handleShortcut } from "./contributions/keyboard_shortcuts.ts";
import { handleBackgroundTask } from "./contributions/background_task.ts";
import { handleSearch } from "./contributions/search_provider.ts";

export function createShowcaseExtension(customHost?: HostFacade) {
  const store = new ShowcaseStateStore();

  const ext = defineExtension(
    {
      onActivate(act: Activation) {
        store.appendLog(`Extension activated by origin: ${act.origin}`);
        if (customHost?.state) {
          try {
            customHost.state.watch("showcase_clicks");
            const persisted = customHost.state.getString("showcase_clicks");
            if (persisted) store.hydrateClicks(Number.parseInt(persisted, 10));
          } catch {
            // Degraded state fallback
          }
        }
      },

      onDeactivate(reason: DeactivateReason) {
        store.appendLog(`Extension deactivated (${reason})`);
      },

      onEvent(event: ExtensionEvent) {
        switch (event.tag) {
          case "input": {
            const input = event.val;
            switch (input.eventId) {
              case "btn-bar-increment":
              case "btn-desktop-increment":
              case "btn-panel-increment":
                store.incrementClicks();
                persistState(customHost, store);
                break;
              case "btn-menu-toggle":
              case "btn-desktop-toggle":
                handleAction("toggle-power", store, customHost);
                break;
              case "btn-menu-copy":
                if (customHost?.clipboard) {
                  try {
                    customHost.clipboard.write(
                      `Showcase Status: ${store.snapshot.mode} (${store.snapshot.clicks} clicks)`,
                    );
                    store.appendLog("Copied status summary to clipboard");
                  } catch {
                    // Clipboard error handled safely
                  }
                }
                break;
              case "btn-menu-refresh":
                if (customHost?.http) {
                  try {
                    customHost.http.request({
                      requestId: "showcase-refresh",
                      url: "https://api.example.com/showcase",
                      method: "GET",
                      headers: [],
                    });
                    store.appendLog("Manual HTTPS refresh requested");
                  } catch {
                    store.appendLog("HTTPS refresh unavailable");
                  }
                }
                break;
              case "btn-panel-clear-logs":
                store.reset();
                break;
              case "tog-notifications":
                if (input.value && DataValue.isBool(input.value)) {
                  store.setNotificationsEnabled(DataValue.toJs(input.value) as boolean);
                  persistState(customHost, store);
                }
                break;
              case "input-label":
                if (input.value && DataValue.isText(input.value)) {
                  store.setAccentLabel(DataValue.toJs(input.value) as string);
                  persistState(customHost, store);
                }
                break;
            }
            break;
          }

          case "palette-generated":
            store.appendLog("System theme palette generated");
            break;

          case "wallpaper-changed":
            store.appendLog("System wallpaper changed");
            break;

          case "state-value": {
            const stateEvent = event.val;
            if (stateEvent.key === "showcase_clicks" && stateEvent.value) {
              store.appendLog(`State watch update for key '${stateEvent.key}'`);
            }
            break;
          }

          case "http-response":
            store.appendLog(
              event.val.error
                ? `HTTPS refresh failed: ${event.val.error}`
                : `HTTPS refresh completed (${event.val.status ?? 0})`,
            );
            break;
        }
      },

      view(contributionId: string): ViewElement | undefined {
        switch (contributionId) {
          case "status-bar":
            return renderBarWidget(store.snapshot);
          case "status-menu":
            return renderBarMenu(store.snapshot);
          case "system-card":
            return renderDesktopWidget(store.snapshot);
          case "preferences":
            return renderSettingsPage(store.snapshot);
          case "side-panel":
            return renderSidePanel(store.snapshot);
          default:
            return undefined;
        }
      },

      search(contributionId, request) {
        return handleSearch(contributionId, request);
      },
    },
    customHost,
  );

  return {
    store,
    ext,
    handleAction: (actionId: string) => handleAction(actionId, store, customHost),
    handleShortcut: (shortcutId: string) => handleShortcut(shortcutId, store, customHost),
    handleBackgroundTask: (taskId: string) => handleBackgroundTask(taskId, store, customHost),
    handleSearch: (contributionId: string, req: Parameters<typeof handleSearch>[1]) =>
      handleSearch(contributionId, req),
  };
}

function persistState(host: HostFacade | undefined, store: ShowcaseStateStore): void {
  try {
    host?.state?.write("showcase_clicks", DataValue.text(String(store.snapshot.clicks)));
  } catch {
    // In-memory state remains available when durable state is degraded.
  }
}

const defaultInstance = createShowcaseExtension();

export const activate = defaultInstance.ext.activate;
export const deactivate = defaultInstance.ext.deactivate;
export const onEvent = defaultInstance.ext.onEvent;
export const view = defaultInstance.ext.view;
export const search = defaultInstance.ext.search;
