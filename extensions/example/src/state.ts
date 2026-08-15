/**
 * State coordinator for the Shilpo Showcase Extension.
 *
 * Provides typed state access, event logging, and degraded in-memory fallbacks
 * when host capabilities are unavailable or encounter transient errors.
 */

export type ShowcaseMode = "active" | "idle";

export interface ShowcaseState {
  clicks: number;
  mode: ShowcaseMode;
  accentLabel: string;
  notificationsEnabled: boolean;
  lastSyncIso: string;
  logs: string[];
}

export function createInitialState(): ShowcaseState {
  return {
    clicks: 0,
    mode: "active",
    accentLabel: "Active",
    notificationsEnabled: true,
    lastSyncIso: new Date().toISOString(),
    logs: ["Showcase extension initialized."],
  };
}

export class ShowcaseStateStore {
  private state: ShowcaseState;

  constructor(initial: ShowcaseState = createInitialState()) {
    this.state = { ...initial };
  }

  get snapshot(): ShowcaseState {
    return { ...this.state, logs: [...this.state.logs] };
  }

  incrementClicks(): number {
    this.state.clicks += 1;
    this.appendLog(`Click counter incremented to ${this.state.clicks}`);
    return this.state.clicks;
  }

  hydrateClicks(clicks: number): void {
    if (Number.isSafeInteger(clicks) && clicks >= 0) this.state.clicks = clicks;
  }

  toggleMode(): ShowcaseMode {
    this.state.mode = this.state.mode === "active" ? "idle" : "active";
    this.state.accentLabel = this.state.mode === "active" ? "Active" : "Idle";
    this.appendLog(`Showcase mode switched to '${this.state.mode}'`);
    return this.state.mode;
  }

  setAccentLabel(label: string): void {
    const trimmed = label.trim();
    if (trimmed.length > 0) {
      this.state.accentLabel = trimmed;
      this.appendLog(`Accent label updated to '${trimmed}'`);
    }
  }

  setNotificationsEnabled(enabled: boolean): void {
    this.state.notificationsEnabled = enabled;
    this.appendLog(`Notifications ${enabled ? "enabled" : "disabled"}`);
  }

  recordSync(): string {
    const nowIso = new Date().toISOString();
    this.state.lastSyncIso = nowIso;
    this.appendLog(`Background task executed sync at ${nowIso}`);
    return nowIso;
  }

  appendLog(entry: string): void {
    const timestamp = new Date().toISOString().substring(11, 19);
    this.state.logs.unshift(`[${timestamp}] ${entry}`);
    if (this.state.logs.length > 50) {
      this.state.logs.pop();
    }
  }

  reset(): void {
    this.state = createInitialState();
  }
}
