import { Column, Row, Spacer, Text, TextInput, Toggle } from "@shilpo/ext-sdk";
import type { ViewElement } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderSettingsPage(state: ShowcaseState): ViewElement {
  return (
    <Column gap={12} style={{ padding: 16 }}>
      <Text bold fontSize={18}>Showcase Preferences</Text>
      <Text>Configure options and live parameters for the showcase extension:</Text>
      <Spacer size={8} />
      <Row alignItems="center" justifyContent="space-between">
        <Text>Desktop Notifications:</Text>
        <Toggle value={state.notificationsEnabled} eventId="tog-notifications" />
      </Row>
      <Spacer size={6} />
      <Column gap={4}>
        <Text>Custom Accent Label:</Text>
        <TextInput
          eventId="input-label"
          value={state.accentLabel}
          placeholder="Enter label (e.g. Active, Work, Focus)"
        />
      </Column>
    </Column>
  );
}
