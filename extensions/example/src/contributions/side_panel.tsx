import { Badge, Button, Column, Icon, Row, Spacer, Text } from "@shilpo/ext-sdk";
import type { ViewElement } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderSidePanel(state: ShowcaseState): ViewElement {
  const logNodes = state.logs.slice(0, 8).map((log) => <Text fontSize={12}>{log}</Text>);

  return (
    <Column gap={10} style={{ padding: 16 }}>
      <Row alignItems="center" gap={8}>
        <Icon name="view_sidebar" size={20} />
        <Text bold fontSize={16}>Showcase Side Panel</Text>
      </Row>
      <Row alignItems="center" justifyContent="space-between">
        <Text>Current Mode:</Text>
        <Badge label={state.mode} />
      </Row>
      <Text>{`Registered Clicks: ${state.clicks}`}</Text>
      <Text>{`Last Sync: ${state.lastSyncIso}`}</Text>
      <Spacer size={6} />
      <Row gap={8}>
        <Button eventId="btn-panel-increment" style={{ padding: 6 }}>Increment</Button>
        <Button eventId="btn-panel-clear-logs" style={{ padding: 6 }}>Clear Logs</Button>
      </Row>
      <Spacer size={8} />
      <Text bold>Recent Activity Log:</Text>
      {logNodes}
    </Column>
  );
}
