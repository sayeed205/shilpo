import { DataValue, defineExtension } from "@shilpo/ext-sdk";
import type {
  Activation,
  DeactivateReason,
  ExtensionEvent,
  HostFacade,
  ViewTree,
} from "@shilpo/ext-sdk";

import { ShowcaseStateStore } from "./state.ts";
import { renderBarWidget } from "./contributions/bar_widget.ts";
import { renderBarMenu } from "./contributions/bar_menu.ts";
import { renderDesktopWidget } from "./contributions/desktop_widget.ts";
import { renderSettingsPage } from "./contributions/settings_page.ts";
import { renderSidePanel } from "./contributions/side_panel.ts";
import { handleAction } from "./contributions/actions.ts";
import { handleShortcut } from "./contributions/keyboard_shortcuts.ts";
import { handleBackgroundTask } from "./contributions/background_task.ts";

export function createShowcaseExtension(customHost?: HostFacade) {
  const store = new ShowcaseStateStore();

  const ext = defineExtension(
    {
      onActivate(act: Activation) {
        store.appendLog(`Extension activated by origin: ${act.origin}`);
        if (customHost?.state) {
          try {
            customHost.state.watch("showcase_clicks");
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
              case "btn-panel-clear-logs":
                store.reset();
                break;
              case "tog-notifications":
                if (input.value && DataValue.isBool(input.value)) {
                  store.setNotificationsEnabled(DataValue.toJs(input.value) as boolean);
                }
                break;
              case "input-label":
                if (input.value && DataValue.isText(input.value)) {
                  store.setAccentLabel(DataValue.toJs(input.value) as string);
                }
                break;
            }
            break;
          }

          case "palette-generated":
            store.appendLog("System theme palette generated");
            if (store.snapshot.notificationsEnabled && customHost?.notifications) {
              try {
                customHost.notifications.show({
                  title: "Palette Updated",
                  body: "Showcase components refreshed for new palette.",
                });
              } catch {
                // Ignore notification failure
              }
            }
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
        }
      },

      view(contributionId: string): ViewTree | undefined {
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
    },
    customHost,
  );

  return {
    store,
    ext,
    handleAction: (actionId: string) => handleAction(actionId, store, customHost),
    handleShortcut: (shortcutId: string) => handleShortcut(shortcutId, store, customHost),
    handleBackgroundTask: (taskId: string) => handleBackgroundTask(taskId, store, customHost),
  };
}

const defaultInstance = createShowcaseExtension();

export const activate = defaultInstance.ext.activate;
export const deactivate = defaultInstance.ext.deactivate;
export const onEvent = defaultInstance.ext.onEvent;
export const view = defaultInstance.ext.view;
