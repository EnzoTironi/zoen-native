import { createDeckcraftSession } from "../../../apps/zoen/src/miniapps/deckcraft-session";
import { streamDeckcraftExport } from "../../../apps/zoen/src/miniapps/deckcraft-export";
import {
  binaryEditorFormat,
  type BinaryExportFormat,
  type BinaryEditorId,
} from "../../../apps/zoen/src/miniapps/binary-formats";
import { sandboxHtml, type VerifiedMiniappResource } from "../../../apps/zoen/src/miniapps/sandbox";
import { hash, decode64, encode64 } from "../../../apps/zoen/src/miniapps/canonical-scene";
import { initializeLocalization } from "../../../apps/zoen/src/i18n/index";
import type { CoreDriver } from "../../../apps/zoen/src/core/types";

/** Actual official MCP bootstrap + rendered engine; the read/CAS port is a
 * synthetic component fixture, never evidence of Rust persistence or agent auth. */
export async function startProbe(config: {
  editor: BinaryEditorId;
  html: string;
  seed: string;
  version?: number;
  runtime: NonNullable<VerifiedMiniappResource["manifest"]["runtimePackage"]>;
}) {
  initializeLocalization("en");
  const format = binaryEditorFormat(config.editor);
  const frame = document.querySelector("iframe")!;
  const module = await (await fetch("/module.js")).text();
  const wasm = new Uint8Array(await (await fetch("/engine.wasm")).arrayBuffer());
  let active = true;
  let requested = false;
  let ids = 0;
  let writes = 0;
  let version = config.version ?? 1;
  if (!Number.isSafeInteger(version) || version < 1) throw Error("Invalid fixture version");
  let holdNext = false;
  let releaseWrite: (() => void) | undefined;
  let accepted = decode64(config.seed);
  const states: unknown[] = [];
  const exports: { format: string; data: string; sha256: string }[] = [];
  const check = () => {
    if (!active) throw Error("Fixture owner retired");
  };
  const read = () => ({
    item_id: "bootstrap-fixture",
    kind: format.id,
    version,
    current_version: version,
    sha256: hash(accepted),
    byte_length: accepted.length,
    data_base64: encode64(accepted),
    engine_revision: format.engineRevision,
    actor_id: "fixture-human",
    actor_kind: "human",
    author_id: "fixture-human",
  });
  const session = createDeckcraftSession({
    editor: format.id,
    itemId: "bootstrap-fixture",
    title: () => "Owned source",
    verified: {
      resource: { uri: format.resourceUri, text: config.html },
    } as VerifiedMiniappResource,
    driver: {
      request: (async (command) => {
        check();
        if (command.op !== "app_binary_read") throw Error("Unexpected read");
        return read();
      }) as CoreDriver["request"],
    },
    mutate: (async (command) => {
      check();
      if (
        command.op !== "app_binary_save" ||
        command.item_id !== "bootstrap-fixture" ||
        command.expected_version !== version ||
        command.expected_sha256 !== hash(accepted)
      )
        throw Error("Synthetic CAS refused");
      if (holdNext) {
        holdNext = false;
        await new Promise<void>((resolve) => {
          releaseWrite = resolve;
        });
        releaseWrite = undefined;
        check();
      }
      accepted = decode64(command.data_base64);
      version++;
      writes++;
      return read();
    }) as CoreDriver["request"],
    check,
    id: () => `probe-${++ids}`,
    context: () => ({
      theme: "light",
      displayMode: "fullscreen",
      containerDimensions: { width: window.innerWidth, height: window.innerHeight },
      safeAreaInsets: { top: 0, right: 0, bottom: 0, left: 0 },
    }),
    send: (message) => frame.contentWindow?.postMessage(message, "*"),
    disposeFrame: () => {
      frame.srcdoc = "";
    },
    close: () => {},
    onState: (state) => states.push(state),
    share: async (kind, head, call) => {
      let chunks: Uint8Array[] = [];
      await streamDeckcraftExport({
        editor: format.id,
        title: "Owned source",
        format: kind,
        head,
        call,
        check,
        create: () => ({
          write: async (bytes) => {
            chunks.push(bytes.slice());
          },
          finish: () => chunks.reduce((sum, c) => sum + c.length, 0),
          share: async () => {
            const bytes = new Uint8Array(chunks.reduce((sum, c) => sum + c.length, 0));
            let offset = 0;
            for (const chunk of chunks) {
              bytes.set(chunk, offset);
              offset += chunk.length;
            }
            exports.push({ format: kind, data: encode64(bytes), sha256: hash(bytes) });
          },
          dispose: () => {
            for (const chunk of chunks) chunk.fill(0);
            chunks = [];
          },
        }),
      });
    },
  });
  window.addEventListener("message", (event) => {
    if (!active || event.source !== frame.contentWindow || event.origin !== "null") return;
    if (event.data?.zoenRuntime === 1) {
      if (!requested && event.data.kind === "load") {
        requested = true;
        frame.contentWindow?.postMessage(
          { zoenRuntime: 1, kind: "ready", revision: config.runtime.revision, module, wasm },
          "*",
          [wasm.buffer],
        );
      }
      return;
    }
    void session.receive(typeof event.data === "string" ? event.data : JSON.stringify(event.data));
  });
  const probe = {
    get opened() {
      return session.opened;
    },
    states,
    exports,
    get writes() {
      return writes;
    },
    get version() {
      return version;
    },
    get accepted() {
      return encode64(accepted);
    },
    share: (kind: BinaryExportFormat) => session.share(kind),
    flush: () => session.controller.flush(),
    close: () => session.controller.flush({ retire: true }),
    holdNextWrite: () => {
      holdNext = true;
    },
    get writeHeld() {
      return !!releaseWrite;
    },
    releaseWrite: () => releaseWrite?.(),
    retire: () => {
      active = false;
      session.retire();
      frame.srcdoc = "";
    },
  };
  Object.assign(window, { probe });
  frame.srcdoc = sandboxHtml(config.html, [], config.runtime);
}
