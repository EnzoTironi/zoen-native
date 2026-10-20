export type HostLayout = {
  width: number;
  height: number;
  insets: { top: number; right: number; bottom: number; left: number };
  closeButton: { x: number; y: number; width: number; height: number } | null;
};
type ManagedUIBindings = {
  zoenHostRequest: (json: string, bytes: Uint8Array) => Promise<unknown>;
  zoenHostOnExport: (callback: ((format: string) => boolean) | undefined) => unknown;
};
const record = (value: unknown): Record<string, unknown> | undefined =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
const finite = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && value >= 0;
/** Official host context is converted to a bounded UI-only engine input. */
export function hostLayout(context: unknown): HostLayout | null {
  const c = record(context);
  const d = record(c?.containerDimensions);
  const i = record(c?.safeAreaInsets);
  if (
    !d ||
    !i ||
    !finite(d.width) ||
    !finite(d.height) ||
    d.width < 1 ||
    d.height < 1 ||
    d.width > 16384 ||
    d.height > 16384 ||
    !["top", "right", "bottom", "left"].every((key) => finite(i[key]))
  )
    return null;
  const insets = {
    top: Number(i.top),
    right: Number(i.right),
    bottom: Number(i.bottom),
    left: Number(i.left),
  };
  if (insets.left + insets.right >= d.width || insets.top + insets.bottom >= d.height) return null;
  const chrome = record(record(c?.zoen)?.chrome);
  const button = record(chrome?.closeButton);
  let closeButton: HostLayout["closeButton"] = null;
  if (chrome?.closeButton !== undefined && chrome.closeButton !== null) {
    if (
      !button ||
      !["top", "right", "width", "height"].every((key) => finite(button[key])) ||
      Number(button.width) <= 0 ||
      Number(button.height) <= 0
    )
      return null;
    const x = d.width - Number(button.right) - Number(button.width);
    const y = Number(button.top);
    if (x < 0 || y + Number(button.height) > d.height) return null;
    closeButton = { x, y, width: Number(button.width), height: Number(button.height) };
  }
  return { width: d.width, height: d.height, insets, closeButton };
}
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
  onError: (error: unknown) => void;
}) {
  if (typeof options.engine.zoenHostOnExport !== "function")
    throw Error("The managed runtime does not support native export intents.");
  let live = true;
  let exporting = false;
  let layoutTask: Promise<void> | null = null;
  let pending: string | null = null;
  let applied: string | null = null;
  function pump(): Promise<void> {
    if (layoutTask) return layoutTask;
    if (!live) return Promise.reject(Error("The managed UI owner has retired."));
    if (!pending) return Promise.resolve();
    layoutTask = (async () => {
      while (live && pending) {
        const next = pending;
        pending = null;
        const reply = record(
          await options.engine.zoenHostRequest(
            JSON.stringify({ op: "layout", input: JSON.parse(next) }),
            new Uint8Array(),
          ),
        );
        if (!live) return;
        if (
          !reply ||
          typeof reply.metadata !== "string" ||
          reply.metadata.length > 1024 ||
          record(JSON.parse(reply.metadata))?.layoutAccepted !== true ||
          !(reply.bytes instanceof Uint8Array) ||
          reply.bytes.length
        )
          throw Error("The engine did not acknowledge host geometry.");
        applied = next;
      }
    })()
      .catch((error) => {
        pending = null;
        if (live) options.onError(error);
        throw error;
      })
      .finally(() => {
        layoutTask = null;
        if (live && pending) return pump();
      });
    return layoutTask;
  }
  options.engine.zoenHostOnExport((format) => {
    if (!live || exporting || !options.ready() || !["pdf", "pptx"].includes(format)) return false;
    exporting = true;
    void options.app
      .callServerTool({ name: "app_binary_share", arguments: { id: options.itemId, format } })
      .then((result) => {
        if (result.isError) throw Error("The native presentation share was refused.");
      })
      .catch((error) => {
        if (live) options.onError(error);
      })
      .finally(() => {
        exporting = false;
      });
    return true;
  });
  return {
    layout(context: unknown) {
      if (!live) return Promise.reject(Error("The managed UI owner has retired."));
      const value = hostLayout(context);
      if (!value) return Promise.reject(Error("Invalid managed host geometry."));
      const json = JSON.stringify(value);
      if (json === applied && !layoutTask) return Promise.resolve();
      pending = json;
      return pump();
    },
    dispose() {
      if (!live) return;
      live = false;
      pending = null;
      try {
        options.engine.zoenHostOnExport(undefined);
      } catch {}
    },
  };
}

/** Whitelist fixed nonpayload engine startup categories; never echo arbitrary errors. */
export function startupFailureCode(error: unknown): string | undefined {
  const allowed = [
    "graphics_unavailable",
    "canvas_missing",
    "runtime_start_failed",
    "resource_fetch_failed",
    "module_init_failed",
    "document_open_failed",
  ];
  if (
    error instanceof Error &&
    allowed.includes(String((error as Error & { zoenStartupCode?: unknown }).zoenStartupCode))
  )
    return String((error as Error & { zoenStartupCode?: unknown }).zoenStartupCode);
  if (typeof error !== "string" || error.length > 4096) return undefined;
  try {
    const code = record(JSON.parse(error))?.code;
    return typeof code === "string" && allowed.includes(code) ? code : undefined;
  } catch {
    return undefined;
  }
}
