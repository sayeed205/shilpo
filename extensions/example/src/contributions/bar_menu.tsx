import { Badge, Button, Column, Row, Spacer, Text } from "@shilpo/ext-sdk";
import type { ViewElement } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderBarMenu(state: ShowcaseState): ViewElement {
  return (
    <Column gap={8} style={{ padding: 12 }}>
      <Row alignItems="center" justifyContent="space-between">
        <Text bold fontSize={14}>Showcase Menu</Text>
        <Badge
          label={state.mode}
          style={{ color: state.mode === "active" ? "primary" : "outline" }}
        />
      </Row>
      <Text>{`Total clicks: ${state.clicks}`}</Text>
      <Text>{`Last background sync: ${state.lastSyncIso.substring(11, 19)}`}</Text>
      <Spacer size={4} />
      <Row gap={6}>
        <Button eventId="btn-menu-toggle" style={{ padding: 6 }}>Toggle Mode</Button>
        <Button eventId="btn-menu-copy" style={{ padding: 6 }}>Copy Status</Button>
        <Button eventId="btn-menu-refresh" style={{ padding: 6 }}>Refresh Demo</Button>
      </Row>
    </Column>
  );
}
