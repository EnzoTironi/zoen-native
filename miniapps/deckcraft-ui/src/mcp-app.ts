import { App } from "@modelcontextprotocol/ext-apps";
import {
  attachDeckcraftDocument,
  type DeckcraftBindings,
} from "../../../apps/zoen/src/miniapps/deckcraft-document";

const app = new App(
  { name: "DeckCraft", version: "0.9.0-zoen" },
  { tools: {} },
  { autoResize: false },
);
type Owner = Awaited<ReturnType<typeof attachDeckcraftDocument>>;
const runtime = (
  window as Window & { __zoenRuntime?: { load(): Promise<DeckcraftBindings>; dispose(): void } }
).__zoenRuntime;
let active = true;
let connected = false;
let checkpoint: string | undefined;
let opening: Promise<void> | undefined;
let owner: Owner | undefined;
let failedInitialOpen = false;
let paused = false;
function id() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6]! & 15) | 64;
  bytes[8] = (bytes[8]! & 63) | 128;
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
function state(phase: "saved" | "saving" | "pending" | "error", ready: boolean) {
  if (!active || !checkpoint) return;
  void app
    .updateModelContext({
      structuredContent: { zoen: { binarySave: { itemId: checkpoint, phase, ready } } },
    })
    .catch(() => {});
}
function retire() {
  if (!active) return;
  active = false;
  owner?.retireAuthority();
  runtime?.dispose();
}
function start() {
  if (!active || !connected || paused || !checkpoint || opening || owner || failedInitialOpen)
    return;
  const itemId = checkpoint;
  opening = (async () => {
    if (!runtime) throw Error("The verified runtime loader is unavailable.");
    const engine = await runtime.load();
    if (!active) return;
    const document = await attachDeckcraftDocument({
      app,
      engine,
      itemId,
      id,
      onState: (phase) => state(phase, true),
    });
    if (!active) {
      document.retireAuthority();
      return;
    }
    owner = document;
  })().catch(() => {
    if (!active) return;
    failedInitialOpen = true;
    // No accepted manual Session was exposed: never leave a failed empty engine interactive.
    runtime?.dispose();
    app.onteardown = initialTeardown;
    app.oncalltool = initialCallTool;
    state("error", false);
  });
}
app.ontoolresult = (result) => {
  const value = result.structuredContent?.checkpointId;
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(value)) return;
  if (checkpoint && checkpoint !== value) {
    retire();
    return;
  }
  checkpoint = value;
  start();
};
app.onhostcontextchanged = (context) => {
  const theme = context.theme;
  if (theme === "dark" || theme === "light") document.documentElement.style.colorScheme = theme;
};
// These readiness handlers exist before connect. The document client replaces
// them once it owns the Session; its serialized open precedes freeze/flush.
const initialTeardown: NonNullable<App["onteardown"]> = async () => {
  paused = true;
  await opening;
  if (owner) await owner.document.flush();
  return {};
};
const initialCallTool: NonNullable<App["oncalltool"]> = async ({ name }) => {
  if (name !== "resume_document") throw Error("The document is not ready.");
  paused = false;
  if (failedInitialOpen) throw Error("The document could not be opened.");
  start();
  await opening;
  if (owner) await owner.document.resume();
  return { content: [] };
};
app.onteardown = initialTeardown;
app.oncalltool = initialCallTool;
window.addEventListener("pagehide", retire, { once: true });
void app
  .connect()
  .then(() => {
    connected = true;
    start();
  })
  .catch(() => {
    if (!active) return;
    runtime?.dispose();
    state("error", false);
  });
