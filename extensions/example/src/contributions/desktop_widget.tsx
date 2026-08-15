import { Button, Column, Icon, Progress, Row, Spacer, Text } from "@shilpo/ext-sdk";
import type { ViewElement } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderDesktopWidget(state: ShowcaseState): ViewElement {
  const progressRatio = Math.min(1.0, (state.clicks % 20) / 20.0);
  return (
    <Column gap={10} style={{ padding: 16 }}>
      <Row alignItems="center" gap={8}>
        <Icon name="dashboard" size={20} />
        <Text bold fontSize={16}>Showcase Desktop Card</Text>
      </Row>
      <Text>{`Status: ${state.accentLabel} • Mode: ${state.mode}`}</Text>
      <Progress value={progressRatio} />
      <Text>{`Clicks toward milestone: ${state.clicks % 20}/20`}</Text>
      <Spacer size={6} />
      <Row gap={8}>
        <Button eventId="btn-desktop-increment" style={{ padding: 8 }}>Increment</Button>
        <Button eventId="btn-desktop-toggle" style={{ padding: 8 }}>Toggle Mode</Button>
      </Row>
    </Column>
  );
}
