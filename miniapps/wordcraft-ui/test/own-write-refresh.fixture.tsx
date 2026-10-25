import { createRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { MiniappFrame } from "../../../apps/zoen/src/miniapps/frame.web";
import type { MiniappFrameHandle } from "../../../apps/zoen/src/miniapps/frame-types";
import { createDeckcraftSession } from "../../../apps/zoen/src/miniapps/deckcraft-session";
import { ClientStore } from "../../../apps/zoen/src/core/store";
import { Driver, empty, deferred } from "../../../apps/zoen/src/core/store.fixtures";
import { initializeLocalization } from "../../../apps/zoen/src/i18n/index";
import { sandboxHtml, type VerifiedMiniappResource } from "../../../apps/zoen/src/miniapps/sandbox";
import { streamDeckcraftExport } from "../../../apps/zoen/src/miniapps/deckcraft-export";
import { binaryEditorFormat } from "../../../apps/zoen/src/miniapps/binary-formats";
import { hash, decode64, encode64 } from "../../../apps/zoen/src/miniapps/canonical-scene";

/** Actual React web frame, ClientStore, official MCP and Word runtime. Only native
 * Core storage/CAS and the OS share sink are synthetic. No iPhone/agent claim. */
export async function startProbe(config: {
  html: string;
  seed: string;
  version?: number;
  runtime: NonNullable<VerifiedMiniappResource["manifest"]["runtimePackage"]>;
}) {
  initializeLocalization("en");
  const format = binaryEditorFormat("wordcraft");
  const ref = createRef<MiniappFrameHandle>();
  let version = config.version ?? 1,
    writes = 0,
    ids = 0,
    live = true;
  let accepted = decode64(config.seed),
    holding = true,
    held = false;
  const ack = deferred<void>();
  const exports: { data: string; sha256: string }[] = [];
  const states: { blocked: boolean; phase: string }[] = [];
  const notifications: number[] = [];
  const errors: string[] = [];
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
  const driver = new Driver();
  driver.operation = async (command) => {
    if (command.op === "snapshot") return empty;
    if (command.op === "app_binary_read") return read();
    if (
      command.op !== "app_binary_save" ||
      command.item_id !== "bootstrap-fixture" ||
      command.expected_version !== version ||
      command.expected_sha256 !== hash(accepted)
    )
      throw Error("Synthetic CAS refused");
    accepted = decode64(command.data_base64);
    version++;
    writes++;
    return read();
  };
  const store = new ClientStore(driver);
  await store.open("demo");
  const check = () => {
    if (!live) throw Error("Fixture owner retired");
  };
  let renderBlocked: ((blocked: boolean) => void) | undefined;
  const session = createDeckcraftSession({
    editor: "wordcraft",
    itemId: "bootstrap-fixture",
    title: () => "Owned Word",
    verified: {
      resource: { uri: format.resourceUri, text: config.html },
    } as VerifiedMiniappResource,
    driver,
    mutate: (async (command) => {
      const receipt = await store.request(command);
      if (holding) {
        holding = false;
        held = true;
        await ack.promise;
        held = false;
      }
      check();
      return receipt;
    }) as typeof driver.request,
    check,
    id: () => `own-browser-${++ids}`,
    context: () => ({
      theme: "light",
      displayMode: "fullscreen",
      containerDimensions: { width: innerWidth, height: innerHeight },
      safeAreaInsets: { top: 0, right: 0, bottom: 0, left: 0 },
    }),
    send: (message) => ref.current?.send(message),
    disposeFrame: () => ref.current?.dispose(),
    close: () => {},
    onState: (state) => {
      states.push(state);
      renderBlocked?.(state.blocked);
    },
    share: async (format, head, call) => {
      let chunks: Uint8Array[] = [];
      await streamDeckcraftExport({
        editor: "wordcraft",
        title: "Owned Word",
        format,
        head,
        call,
        check,
        create: () => ({
          write: async (bytes) => {
            chunks.push(bytes.slice());
          },
          finish: () => chunks.reduce((n, bytes) => n + bytes.length, 0),
          share: async () => {
            const bytes = new Uint8Array(chunks.reduce((n, b) => n + b.length, 0));
            let at = 0;
            for (const chunk of chunks) {
              bytes.set(chunk, at);
              at += chunk.length;
            }
            exports.push({ data: encode64(bytes), sha256: hash(bytes) });
          },
          dispose: () => {
            for (const chunk of chunks) chunk.fill(0);
            chunks = [];
          },
        }),
      });
    },
  });
  let notified = version;
  const refreshing: Promise<void>[] = [];
  const stop = store.subscribe(() => {
    if (!live || version <= notified) return;
    notified = version;
    notifications.push(version);
    const task = session.refresh(version);
    void task.catch((error) => errors.push(String(error)));
    refreshing.push(task);
  });
  const html = sandboxHtml(config.html, [], config.runtime);
  function Surface() {
    const [blocked, setBlocked] = useState(true);
    renderBlocked = setBlocked;
    return (
      <MiniappFrame
        ref={ref}
        html={html}
        title="Owned Word"
        height={innerHeight}
        blocked={blocked}
        runtimePackage={config.runtime}
        onMessage={(message) => {
          void session.receive(message);
        }}
        onError={(error) => errors.push(error)}
      />
    );
  }
  const root = createRoot(document.getElementById("root")!);
  root.render(<Surface />);
  Object.assign(window, {
    probe: {
      get opened() {
        return session.opened;
      },
      get writeHeld() {
        return held;
      },
      get writes() {
        return writes;
      },
      get version() {
        return version;
      },
      get accepted() {
        return encode64(accepted);
      },
      states,
      notifications,
      exports,
      errors,
      markInput: () => {
        states.length = 0;
      },
      releaseWrite: () => ack.resolve(),
      finish: async () => {
        ack.resolve();
        await session.controller.flush();
        await Promise.all(refreshing);
        await session.share("docx");
      },
      retire: async () => {
        stop();
        ack.resolve();
        live = false;
        session.retire();
        root.unmount();
        await store.close({ force: true });
      },
    },
  });
}
