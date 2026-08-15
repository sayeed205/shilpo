import { Button, Icon, Row, Text } from "@shilpo/ext-sdk";
import type { ViewElement } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export function renderBarWidget(state: ShowcaseState): ViewElement {
  const iconName = state.mode === "active" ? "stars" : "bedtime";
  return (
    <Row gap={6} alignItems="center">
      <Icon name={iconName} size={16} />
      <Text bold>{`Showcase: ${state.accentLabel} (${state.clicks})`}</Text>
      <Button eventId="btn-bar-increment" style={{ padding: 4 }}>Click</Button>
    </Row>
  );
}
