import type {
  Activation,
  BarMenuClosedPayload,
  BarMenuOpenedPayload,
  DeactivateReason,
  ExtensionEvent,
  HttpResponseEvent,
  InputEvent,
  LocationResponseEvent,
  StateEvent,
  ViewTree,
  WallpaperRequest,
  WorkspaceEvent,
} from "./generated/wit.ts";
import { createHostFacade, type HostFacade } from "./host.ts";
import { type ViewNodeSpec } from "./builder/nodes.ts";
import { buildViewTree } from "./builder/tree.ts";
import { isFragment, normalizeChildren } from "./jsx/components.ts";
import type { FragmentSpec } from "./jsx/types.ts";

export interface ExtensionDefinition {
  onActivate?: (act: Activation, host: HostFacade) => void | Promise<void>;
  onDeactivate?: (reason: DeactivateReason, host: HostFacade) => void | Promise<void>;
  onEvent?: (event: ExtensionEvent, host: HostFacade) => void | Promise<void>;
  view?: (
    contributionId: string,
    host: HostFacade,
  ) => ViewTree | ViewNodeSpec | FragmentSpec | undefined | null;

  // Specific event handlers for ergonomics
  onInput?: (event: InputEvent, host: HostFacade) => void;
  onWorkspaceChanged?: (event: WorkspaceEvent, host: HostFacade) => void;
  onWallpaperRequest?: (req: WallpaperRequest, host: HostFacade) => void;
  onStateValue?: (event: StateEvent, host: HostFacade) => void;
  onHttpResponse?: (event: HttpResponseEvent, host: HostFacade) => void;
  onLocationResponse?: (event: LocationResponseEvent, host: HostFacade) => void;
  onBarMenuOpened?: (payload: BarMenuOpenedPayload, host: HostFacade) => void;
  onBarMenuClosed?: (payload: BarMenuClosedPayload, host: HostFacade) => void;
}

export interface ExtensionExports {
  activate: (act: Activation) => void;
  deactivate: (reason: DeactivateReason) => void;
  onEvent: (event: ExtensionEvent) => void;
  view: (contributionId: string) => ViewTree | undefined;
}

function sanitizeErrorMessage(err: unknown): string {
  const raw = err instanceof Error
    ? err.message
    : typeof err === "string"
    ? err
    : "Unknown extension error";
  return raw
    .replace(/(?:\/home|\/Users|\/tmp|\/private|[A-Za-z]:\\)[^\s"']+/g, "<path>")
    .replace(/(?:SecretRef\s*\(|secret(?:[-_ ]?handle)?\s*[=:])[^\s,)]+\)?/gi, "<secret-redacted>")
    .replace(/\b(handle|token|password|secret)=[^\s&]+/gi, "$1=<redacted>");
}

function runSync<T>(operation: () => T): T {
  try {
    const value = operation();
    if (value && typeof value === "object" && "then" in value) {
      throw new Error("Async lifecycle handlers are not supported by the synchronous WIT ABI");
    }
    return value;
  } catch (err) {
    throw new Error(sanitizeErrorMessage(err));
  }
}

/**
 * Defines a Shilpo extension with typed lifecycle handlers and automatic error boundary wrapping.
 *
 * Produces the four canonical guest exports: `activate`, `deactivate`, `onEvent`, and `view`.
 */
export function defineExtension(
  definition: ExtensionDefinition,
  customHost?: HostFacade,
): ExtensionExports {
  const host = customHost ?? createHostFacade();

  return {
    activate(act: Activation): void {
      runSync(() => definition.onActivate?.(act, host));
    },

    deactivate(reason: DeactivateReason): void {
      runSync(() => definition.onDeactivate?.(reason, host));
    },

    onEvent(event: ExtensionEvent): void {
      runSync(() => {
        // Dispatch specific event handlers
        switch (event.tag) {
          case "input":
            if (definition.onInput) definition.onInput(event.val, host);
            break;
          case "workspace-changed":
            if (definition.onWorkspaceChanged) definition.onWorkspaceChanged(event.val, host);
            break;
          case "wallpaper-request":
            if (definition.onWallpaperRequest) definition.onWallpaperRequest(event.val, host);
            break;
          case "state-value":
            if (definition.onStateValue) definition.onStateValue(event.val, host);
            break;
          case "http-response":
            if (definition.onHttpResponse) definition.onHttpResponse(event.val, host);
            break;
          case "location-response":
            if (definition.onLocationResponse) definition.onLocationResponse(event.val, host);
            break;
          case "bar-menu-opened":
            if (definition.onBarMenuOpened) definition.onBarMenuOpened(event.val, host);
            break;
          case "bar-menu-closed":
            if (definition.onBarMenuClosed) definition.onBarMenuClosed(event.val, host);
            break;
        }

        // Generic event handler
        if (definition.onEvent) {
          definition.onEvent(event, host);
        }
      });
    },

    view(contributionId: string): ViewTree | undefined {
      return runSync(() => {
        if (!definition.view) {
          return undefined;
        }
        const result = definition.view(contributionId, host);
        if (result === undefined || result === null) {
          return undefined;
        }
        if ("nodes" in result && "root" in result) {
          return result as ViewTree;
        }
        if (isFragment(result)) {
          const children = normalizeChildren(result.children, "Fragment");
          if (children.length === 0) {
            throw new Error(
              "View returned an empty fragment. A view must normalize to exactly one root ViewNode.",
            );
          }
          if (children.length > 1) {
            throw new Error(
              `View returned multiple root elements (${children.length}). A view must normalize to exactly one root ViewNode; wrap elements in a Container, Row, Column, or Stack.`,
            );
          }
          return buildViewTree(children[0]!);
        }
        return buildViewTree(result as ViewNodeSpec);
      });
    },
  };
}
