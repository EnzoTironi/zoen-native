import { App } from "@modelcontextprotocol/ext-apps";
import type { JSONRPCMessage, Transport } from "@modelcontextprotocol/client";
import { createDeckcraftSession } from "../../../apps/zoen/src/miniapps/deckcraft-session";
import {
  attachDeckcraftDocument,
  type DeckcraftBindings,
} from "../../../apps/zoen/src/miniapps/deckcraft-document";
import { streamDeckcraftExport } from "../../../apps/zoen/src/miniapps/deckcraft-export";
import { DECKCRAFT_ENGINE_REVISION } from "../../../apps/zoen/src/miniapps/canonical-binary";
import { hash } from "../../../apps/zoen/src/miniapps/canonical-scene";
import { initializeLocalization } from "../../../apps/zoen/src/i18n/index";
import type { CoreDriver } from "../../../apps/zoen/src/core/types";
import type { VerifiedMiniappResource } from "../../../apps/zoen/src/miniapps/sandbox";

const assert = (value: unknown, message: string) => {
  if (!value) throw Error(message);
};
export async function runHistoryExport(
  engine: DeckcraftBindings,
  original: Uint8Array,
  changed: Uint8Array,
) {
  initializeLocalization("en");
  const selected = { version: "1", sha256: hash(original) };
  let transport: Transport;
  let active = true;
  let current = 1;
  let ids = 0;
  let writes = 0;
  const reads: unknown[] = [];
  const exports: { format: string; filename: string; sha256: string; data: string }[] = [];
  const app = new App(
    { name: "Historical export real engine fixture", version: "1" },
    { tools: {} },
    { autoResize: false },
  );
  const check = () => {
    if (!active) throw Error("Fixture owner retired");
  };
  const session = createDeckcraftSession({
    itemId: "history-fixture",
    selectedHead: selected,
    title: () => "Historical source",
    verified: {
      resource: {
        uri: "ui://zoen/deckcraft/index.html",
        mime_type: "text/html;profile=mcp-app",
        text: "",
      },
    } as VerifiedMiniappResource,
    driver: {
      request: (async (command) => {
        assert(command.op === "app_binary_read", "No command other than canonical read");
        if (command.op !== "app_binary_read") throw Error("Unexpected command");
        reads.push({ version: command.version, sha256: command.expected_sha256, current });
        assert(
          command.version === 1 && command.expected_sha256 === selected.sha256,
          "No fallback to current",
        );
        return {
          item_id: "history-fixture",
          kind: "deckcraft",
          version: 1,
          current_version: current,
          sha256: selected.sha256,
          byte_length: original.length,
          data_base64: btoa(String.fromCharCode(...original)),
          engine_revision: DECKCRAFT_ENGINE_REVISION,
          actor_id: "fixture-human",
          actor_kind: "human",
          author_id: "fixture-human",
        };
      }) as CoreDriver["request"],
    },
    mutate: (async () => {
      writes++;
      throw Error("Readonly mutation refused");
    }) as CoreDriver["request"],
    check,
    id: () => `fixture-${++ids}`,
    context: () => ({ theme: "light" }),
    send: (message) => transport.onmessage?.(message as JSONRPCMessage),
    disposeFrame: () => {},
    close: () => {},
    onState: () => {},
    share: async (format, head, call) => {
      assert(
        head.version === selected.version && head.sha256 === selected.sha256,
        "Selected export head",
      );
      await streamDeckcraftExport({
        title: "Historical source",
        format,
        head,
        call,
        check,
        create: (filename) => {
          const chunks: Uint8Array[] = [];
          return {
            write: async (bytes) => {
              chunks.push(bytes.slice());
            },
            finish: () => chunks.reduce((n, c) => n + c.length, 0),
            share: async () => {
              check();
              const bytes = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
              let offset = 0;
              for (const chunk of chunks) {
                bytes.set(chunk, offset);
                offset += chunk.length;
              }
              let encoded = "";
              for (const byte of bytes) encoded += String.fromCharCode(byte);
              exports.push({ format, filename, sha256: hash(bytes), data: btoa(encoded) });
            },
            dispose: () => {
              for (const chunk of chunks) chunk.fill(0);
            },
          };
        },
      });
    },
  });
  transport = {
    start: async () => {},
    close: async () => {},
    send: async (message) => {
      void session.receive(JSON.stringify(message));
    },
  };
  let owner: Awaited<ReturnType<typeof attachDeckcraftDocument>> | undefined;
  try {
    await app.connect(transport);
    owner = await attachDeckcraftDocument({
      app,
      engine,
      itemId: "history-fixture",
      id: () => `export-${++ids}`,
    });
    await app.updateModelContext({
      structuredContent: {
        zoen: { binarySave: { itemId: "history-fixture", phase: "saved", ready: true } },
      },
    });
    current = 2;
    assert(hash(changed) !== selected.sha256, "Current v2 differs from selected v1");
    await owner.document.resume();
    const status = JSON.parse(
      (await engine.zoenHostRequest('{"op":"status"}', new Uint8Array())).metadata,
    );
    assert(
      status.frozen === true && status.head.sha256 === selected.sha256,
      "Readonly engine stays frozen",
    );
    let editRefused = false;
    try {
      await engine.zoenHostRequest(JSON.stringify({ op: "command", input: {} }), new Uint8Array());
    } catch {
      editRefused = true;
    }
    assert(editRefused, "Actual engine rejects editing while frozen");
    await session.share("pptx");
    await session.share("pdf");
    assert(exports.length === 2 && writes === 0, "Both genuine formats and no save");
    active = false;
    let staleRefused = false;
    try {
      await session.share("pdf");
    } catch {
      staleRefused = true;
    }
    assert(staleRefused && exports.length === 2, "Retirement prevents another share");
    return {
      selected,
      current: { version: 2, sha256: hash(changed) },
      reads,
      writes,
      frozen: status.frozen,
      editRefused,
      staleRefused,
      exports,
    };
  } finally {
    owner?.retireAuthority();
    session.retire();
    await app.close();
  }
}
