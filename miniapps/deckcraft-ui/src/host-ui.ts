/** DeckCraft adapter over the shared SDK presentation lifetime; no default App singleton. */
import {
  installManagedHostUI as installSharedManagedHostUI,
  type ManagedHostUIErrorSource,
} from "../../../packages/zoen-miniapp-sdk/src/managed-host-ui.ts";
export {
  hostLayout,
  startupFailureCode,
} from "../../../packages/zoen-miniapp-sdk/src/managed-host-ui.ts";
export type {
  HostLayout,
  ManagedHostUIErrorSource,
} from "../../../packages/zoen-miniapp-sdk/src/managed-host-ui.ts";

type ManagedUIBindings = {
  zoenHostRequest: (json: string, bytes: Uint8Array) => Promise<unknown>;
  zoenHostOnExport: (callback: ((format: string) => boolean) | undefined) => unknown;
};

/** Trusted DeckCraft format/ABI adapter; native/core validators remain authoritative. */
export function installManagedHostUI(options: {
  app: {
    callServerTool: (args: {
      name: string;
      arguments: Record<string, unknown>;
    }) => Promise<{ isError?: boolean }>;
  };
  engine: ManagedUIBindings;
  itemId: string;
  ready: () => boolean;
  onError: (error: unknown, source?: ManagedHostUIErrorSource) => void;
}) {
  if (typeof options.engine.zoenHostOnExport !== "function")
    throw Error("The managed runtime does not support native export intents.");
  return installSharedManagedHostUI({
    engine: {
      applyHostLayout: (input) =>
        options.engine.zoenHostRequest(JSON.stringify({ op: "layout", input }), new Uint8Array()),
      setExportIntentHandler: (callback) => options.engine.zoenHostOnExport(callback),
    },
    exportFormats: ["pdf", "pptx"] as const,
    share: (format) =>
      options.app.callServerTool({
        name: "app_binary_share",
        arguments: { id: options.itemId, format },
      }),
    exportRefusedMessage: "The native presentation share was refused.",
    ready: options.ready,
    onError: options.onError,
  });
}
