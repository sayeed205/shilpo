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
  StateEvent,
  StateHost,
  StateMutation,
  StateSnapshot,
  ThemeHost,
  ThemeInfo,
  WallpaperHost,
  WallpaperInfo,
  WallpaperSetRequest,
  WatchRegistration,
} from "../generated/wit.ts";
import { err, ok } from "../generated/wit.ts";
import { createHostFacade, type HostFacade, type HostPorts } from "../host.ts";

export class FakeHost implements HostPorts {
  // Actions
  public readonly actionInvocations: Array<{ actionId: string; payload?: DataValue }> = [];
  public readonly actions: ActionsHost = {
    invoke: (actionId: string, payload?: DataValue): Result<void, ErrorPayload> => {
      this.actionInvocations.push({ actionId, payload });
      return ok(undefined);
    },
  };

  // Clipboard
  public clipboardContent = "";
  public readonly clipboard: ClipboardHost = {
    read: (): Result<string, ErrorPayload> => {
      return ok(this.clipboardContent);
    },
    write: (text: string): Result<void, ErrorPayload> => {
      this.clipboardContent = text;
      return ok(undefined);
    },
  };

  // Filesystem
  public readonly virtualFiles: Map<string, Uint8Array> = new Map();
  public readonly filesystem: FilesystemHost = {
    readFile: (path: string): Result<Uint8Array, ErrorPayload> => {
      const data = this.virtualFiles.get(path);
      if (!data) {
        return err({ kind: "not-found", message: `File not found: ${path}` });
      }
      return ok(data);
    },
    writeFile: (path: string, contents: Uint8Array): Result<void, ErrorPayload> => {
      this.virtualFiles.set(path, contents);
      return ok(undefined);
    },
  };

  // HTTP
  public readonly httpRequests: HttpRequest[] = [];
  public readonly http: HttpHost = {
    request: (req: HttpRequest): Result<string, ErrorPayload> => {
      this.httpRequests.push(req);
      return ok(req.requestId);
    },
    cancel: (_reqId: string): Result<void, ErrorPayload> => {
      return ok(undefined);
    },
  };

  // Location
  public nextLocationRequestId = "loc-req-1";
  public readonly location: LocationHost = {
    read: (): Result<string, ErrorPayload> => {
      return ok(this.nextLocationRequestId);
    },
  };

  // Notifications
  public readonly notificationsList: NotificationRequest[] = [];
  public readonly notifications: NotificationsHost = {
    show: (req: NotificationRequest): Result<void, ErrorPayload> => {
      this.notificationsList.push(req);
      return ok(undefined);
    },
  };

  // State
  private stateStore = new Map<string, DataValue>();
  private currentRevision = 1n;
  private nextWatchId = 1n;
  private watches = new Map<bigint, string>();
  public readonly stateEvents: StateEvent[] = [];

  public readonly state: StateHost = {
    read: (key: string): Result<StateSnapshot, ErrorPayload> => {
      const val = this.stateStore.get(key);
      return ok({ value: val, revision: this.currentRevision });
    },

    write: (key: string, value: DataValue): Result<StateMutation, ErrorPayload> => {
      if (value.tag === "secret-ref") {
        return err({
          kind: "invalid-argument",
          message: "SecretRef values cannot be stored in extension state",
        });
      }
      const existing = this.stateStore.get(key);
      const isIdentical = existing && JSON.stringify(existing) === JSON.stringify(value);
      if (isIdentical) {
        return ok({ changed: false, revision: this.currentRevision });
      }

      this.currentRevision += 1n;
      this.stateStore.set(key, value);

      // Trigger watches
      for (const [watchId, watchedKey] of this.watches.entries()) {
        if (watchedKey === key) {
          this.stateEvents.push({
            watchId,
            key,
            value,
            revision: this.currentRevision,
          });
        }
      }

      return ok({ changed: true, revision: this.currentRevision });
    },

    delete: (key: string): Result<StateMutation, ErrorPayload> => {
      if (!this.stateStore.has(key)) {
        return ok({ changed: false, revision: this.currentRevision });
      }

      this.currentRevision += 1n;
      this.stateStore.delete(key);

      for (const [watchId, watchedKey] of this.watches.entries()) {
        if (watchedKey === key) {
          this.stateEvents.push({
            watchId,
            key,
            value: undefined,
            revision: this.currentRevision,
          });
        }
      }

      return ok({ changed: true, revision: this.currentRevision });
    },

    watch: (key: string): Result<WatchRegistration, ErrorPayload> => {
      const watchId = this.nextWatchId++;
      this.watches.set(watchId, key);
      const snapshot: StateSnapshot = {
        value: this.stateStore.get(key),
        revision: this.currentRevision,
      };
      return ok({ watchId, snapshot });
    },

    unwatch: (watchId: bigint): Result<void, ErrorPayload> => {
      this.watches.delete(watchId);
      return ok(undefined);
    },
  };

  // Secrets
  private secretStore = new Map<string, Uint8Array>();
  private nextSecretSeq = 1;

  public readonly secrets: SecretsHost = {
    set: (purpose: string, value: Uint8Array): Result<SecretRef, ErrorPayload> => {
      const handle = `sec-${this.nextSecretSeq++}`;
      this.secretStore.set(`${purpose}:${handle}`, new Uint8Array(value));
      return ok({ handle });
    },

    read: (purpose: string, reference: SecretRef): Result<Uint8Array | undefined, ErrorPayload> => {
      const data = this.secretStore.get(`${purpose}:${reference.handle}`);
      return ok(data ? new Uint8Array(data) : undefined);
    },

    delete: (purpose: string, reference: SecretRef): Result<void, ErrorPayload> => {
      this.secretStore.delete(`${purpose}:${reference.handle}`);
      return ok(undefined);
    },
  };

  // Theme
  public themeMode = "dark";
  public themeAccent = "#6750A4";
  public readonly theme: ThemeHost = {
    read: (): Result<ThemeInfo, ErrorPayload> => {
      return ok({ mode: this.themeMode, accent: this.themeAccent });
    },
    setSourceColor: (color: string): Result<void, ErrorPayload> => {
      this.themeAccent = color;
      return ok(undefined);
    },
  };

  // Wallpaper
  public wallpaperPath = "/path/to/current_wallpaper.jpg";
  public readonly wallpaper: WallpaperHost = {
    read: (): Result<WallpaperInfo, ErrorPayload> => {
      return ok({ path: this.wallpaperPath });
    },
    set: (req: WallpaperSetRequest): Result<void, ErrorPayload> => {
      this.wallpaperPath = req.path;
      return ok(undefined);
    },
  };

  /**
   * Creates a typed `HostFacade` wired to this fake host.
   */
  toFacade(): HostFacade {
    return createHostFacade(this);
  }
}

/**
 * Creates a hermetic, in-memory `FakeHost` and wired `HostFacade` for testing.
 */
export function createTestHost(): { host: FakeHost; facade: HostFacade } {
  const host = new FakeHost();
  return { host, facade: host.toFacade() };
}
