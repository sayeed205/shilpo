import type {
  ActionsHost,
  ClipboardHost,
  DataValue,
  ErrorPayload,
  FilesystemHost,
  HttpHost,
  HttpRequest,
  LocationHost,
  NotificationRequest,
  NotificationsHost,
  Result,
  SecretRef,
  SecretsHost,
  StateHost,
  StateMutation,
  StateSnapshot,
  ThemeHost,
  ThemeInfo,
  WallpaperHost,
  WallpaperInfo,
  WallpaperSetRequest,
  WatchRegistration,
} from "./generated/wit.ts";
import { DataValue as DataValueHelper } from "./data.ts";

export type {
  ActionsHost,
  ClipboardHost,
  ErrorPayload,
  FilesystemHost,
  HttpHost,
  HttpRequest,
  LocationHost,
  NotificationRequest,
  NotificationsHost,
  Result,
  SecretRef,
  SecretsHost,
  StateHost,
  StateMutation,
  StateSnapshot,
  ThemeHost,
  ThemeInfo,
  WallpaperHost,
  WallpaperInfo,
  WallpaperSetRequest,
  WatchRegistration,
};

export class HostError extends Error {
  readonly kind: ErrorPayload["kind"];

  constructor(payload: ErrorPayload) {
    super(payload.message);
    this.name = "HostError";
    this.kind = payload.kind;
  }
}

function unwrapResult<T>(result: Result<T, ErrorPayload>): T {
  if (result.ok) {
    return result.value;
  }
  throw new HostError(result.error);
}

function missingHostError(name: string): never {
  throw new HostError({
    kind: "backend-unavailable",
    message:
      `Host capability '${name}' is not available in this environment. Provide an injected host or run inside the Shilpo extension runtime.`,
  });
}

export interface HostPorts {
  actions?: ActionsHost;
  clipboard?: ClipboardHost;
  filesystem?: FilesystemHost;
  http?: HttpHost;
  location?: LocationHost;
  notifications?: NotificationsHost;
  state?: StateHost;
  secrets?: SecretsHost;
  theme?: ThemeHost;
  wallpaper?: WallpaperHost;
}

export class ActionsFacade {
  private readonly port?: ActionsHost;

  constructor(port?: ActionsHost) {
    this.port = port;
  }

  invoke(actionId: string, payload?: DataValue): void {
    if (!this.port) missingHostError("actions");
    unwrapResult(this.port.invoke(actionId, payload));
  }
}

export class ClipboardFacade {
  private readonly port?: ClipboardHost;

  constructor(port?: ClipboardHost) {
    this.port = port;
  }

  read(): string {
    if (!this.port) missingHostError("clipboard");
    return unwrapResult(this.port.read());
  }

  write(text: string): void {
    if (!this.port) missingHostError("clipboard");
    unwrapResult(this.port.write(text));
  }
}

export class FilesystemFacade {
  private readonly port?: FilesystemHost;

  constructor(port?: FilesystemHost) {
    this.port = port;
  }

  readFile(path: string): Uint8Array {
    if (!this.port) missingHostError("filesystem");
    return unwrapResult(this.port.readFile(path));
  }

  writeFile(path: string, contents: Uint8Array): void {
    if (!this.port) missingHostError("filesystem");
    unwrapResult(this.port.writeFile(path, contents));
  }
}

export class HttpFacade {
  private readonly port?: HttpHost;

  constructor(port?: HttpHost) {
    this.port = port;
  }

  request(req: HttpRequest): string {
    if (!this.port) missingHostError("http");
    return unwrapResult(this.port.request(req));
  }

  cancel(reqId: string): void {
    if (!this.port) missingHostError("http");
    unwrapResult(this.port.cancel(reqId));
  }
}

export class LocationFacade {
  private readonly port?: LocationHost;

  constructor(port?: LocationHost) {
    this.port = port;
  }

  read(): string {
    if (!this.port) missingHostError("location");
    return unwrapResult(this.port.read());
  }
}

export class NotificationsFacade {
  private readonly port?: NotificationsHost;

  constructor(port?: NotificationsHost) {
    this.port = port;
  }

  show(req: NotificationRequest): void {
    if (!this.port) missingHostError("notifications");
    unwrapResult(this.port.show(req));
  }
}

export class StateFacade {
  private readonly port?: StateHost;

  constructor(port?: StateHost) {
    this.port = port;
  }

  read(key: string): StateSnapshot {
    if (!this.port) missingHostError("state");
    return unwrapResult(this.port.read(key));
  }

  write(key: string, value: DataValue): StateMutation {
    if (!this.port) missingHostError("state");
    return unwrapResult(this.port.write(key, value));
  }

  delete(key: string): StateMutation {
    if (!this.port) missingHostError("state");
    return unwrapResult(this.port.delete(key));
  }

  watch(key: string): WatchRegistration {
    if (!this.port) missingHostError("state");
    return unwrapResult(this.port.watch(key));
  }

  unwatch(watchId: bigint): void {
    if (!this.port) missingHostError("state");
    unwrapResult(this.port.unwatch(watchId));
  }

  getString(key: string): string | undefined {
    const snap = this.read(key);
    if (snap.value && snap.value.tag === "text-value") {
      return snap.value.val;
    }
    return undefined;
  }

  setString(key: string, value: string): StateMutation {
    return this.write(key, DataValueHelper.text(value));
  }

  getNumber(key: string): number | undefined {
    const snap = this.read(key);
    if (!snap.value) return undefined;
    if (snap.value.tag === "float-value") return snap.value.val;
    if (snap.value.tag === "int-value") return Number(snap.value.val);
    return undefined;
  }

  setNumber(key: string, value: number): StateMutation {
    if (Number.isInteger(value)) {
      return this.write(key, DataValueHelper.int(value));
    }
    return this.write(key, DataValueHelper.float(value));
  }

  getBoolean(key: string): boolean | undefined {
    const snap = this.read(key);
    if (snap.value && snap.value.tag === "bool-value") {
      return snap.value.val;
    }
    return undefined;
  }

  setBoolean(key: string, value: boolean): StateMutation {
    return this.write(key, DataValueHelper.bool(value));
  }
}

export class SecretsFacade {
  private readonly port?: SecretsHost;

  constructor(port?: SecretsHost) {
    this.port = port;
  }

  set(purpose: string, value: Uint8Array): SecretRef {
    if (!this.port) missingHostError("secrets");
    return unwrapResult(this.port.set(purpose, value));
  }

  read(purpose: string, reference: SecretRef): Uint8Array | undefined {
    if (!this.port) missingHostError("secrets");
    return unwrapResult(this.port.read(purpose, reference));
  }

  delete(purpose: string, reference: SecretRef): void {
    if (!this.port) missingHostError("secrets");
    unwrapResult(this.port.delete(purpose, reference));
  }
}

export class ThemeFacade {
  private readonly port?: ThemeHost;

  constructor(port?: ThemeHost) {
    this.port = port;
  }

  read(): ThemeInfo {
    if (!this.port) missingHostError("theme");
    return unwrapResult(this.port.read());
  }

  setSourceColor(color: string): void {
    if (!this.port) missingHostError("theme");
    unwrapResult(this.port.setSourceColor(color));
  }
}

export class WallpaperFacade {
  private readonly port?: WallpaperHost;

  constructor(port?: WallpaperHost) {
    this.port = port;
  }

  read(): WallpaperInfo {
    if (!this.port) missingHostError("wallpaper");
    return unwrapResult(this.port.read());
  }

  set(req: WallpaperSetRequest): void {
    if (!this.port) missingHostError("wallpaper");
    unwrapResult(this.port.set(req));
  }
}

export interface HostFacade {
  readonly actions: ActionsFacade;
  readonly clipboard: ClipboardFacade;
  readonly filesystem: FilesystemFacade;
  readonly http: HttpFacade;
  readonly location: LocationFacade;
  readonly notifications: NotificationsFacade;
  readonly state: StateFacade;
  readonly secrets: SecretsFacade;
  readonly theme: ThemeFacade;
  readonly wallpaper: WallpaperFacade;
}

export function createHostFacade(ports: HostPorts = {}): HostFacade {
  return {
    actions: new ActionsFacade(ports.actions),
    clipboard: new ClipboardFacade(ports.clipboard),
    filesystem: new FilesystemFacade(ports.filesystem),
    http: new HttpFacade(ports.http),
    location: new LocationFacade(ports.location),
    notifications: new NotificationsFacade(ports.notifications),
    state: new StateFacade(ports.state),
    secrets: new SecretsFacade(ports.secrets),
    theme: new ThemeFacade(ports.theme),
    wallpaper: new WallpaperFacade(ports.wallpaper),
  };
}
