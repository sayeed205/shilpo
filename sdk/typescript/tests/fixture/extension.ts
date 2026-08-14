import {
  button,
  Colors,
  column,
  defineExtension,
  list,
  progress,
  row,
  style,
  text,
  toggle,
} from "../../src/index.ts";

let clickCount = 0;

const ext = defineExtension({
  onActivate(_act, host) {
    host.notifications.show({
      title: "TS Fixture",
      body: "Activated",
    });
    host.state.setString("status", "active");
  },

  onDeactivate(_reason, host) {
    host.state.setString("status", "inactive");
  },

  onInput(event, host) {
    if (event.eventId === "increment") {
      clickCount += 1;
      host.state.setNumber("clicks", clickCount);
    }
  },

  onHttpResponse(event, host) {
    host.state.setString("http_status", String(event.status ?? 0));
  },

  view(contributionId) {
    if (contributionId !== "bar_widget") {
      return undefined;
    }

    return column({
      gap: 6,
      style: style({ padding: 8, background: Colors.surfaceContainer }),
      children: [
        row({
          children: [
            text("TypeScript Extension", { bold: true, style: style({ color: Colors.primary }) }),
          ],
        }),
        text(`Clicks: ${clickCount}`),
        toggle(true, "tog_active"),
        progress(0.5),
        list([
          text("List Entry 1"),
          text("List Entry 2"),
        ]),
        button("Increment", "increment"),
      ],
    });
  },
});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
