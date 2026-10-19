import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { Excalidraw, serializeAsJSON } from "@excalidraw/excalidraw";
import type { App } from "@modelcontextprotocol/ext-apps";
import type { AppState, BinaryFiles, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import { canonicalSceneTransport } from "./canonical-transfer";
import { createSceneSaves, type SceneSaveState } from "./canonical-saves";

class PendingSceneImages extends Error {}
const IMAGE_IMPORT_DEADLINE_MS = 10_000;

// Excalidraw's database serializer deliberately omits files. Keep its chosen
// element/appState fields, then attach only binary assets referenced by live images.
export function serializeCanonicalScene(
  elements: readonly ExcalidrawElement[],
  appState: AppState,
  files: BinaryFiles,
): string {
  const document = JSON.parse(serializeAsJSON(elements, appState, files, "database"));
  const owned: BinaryFiles = {};
  for (const element of document.elements as ExcalidrawElement[]) {
    if (element.type !== "image" || element.isDeleted) continue;
    const file = element.fileId && files[element.fileId];
    // Upstream inserts the element before its asynchronous image import finishes.
    // Keep that whole scene local until its owned bytes are available.
    if (!file?.dataURL) throw new PendingSceneImages();
    owned[element.fileId!] = file;
  }
  return JSON.stringify({ ...document, files: owned });
}

const owners = new WeakMap<App, object>();
export function canonicalSceneMarker(app: App): { version: 1; itemId: string } | null {
  const extension = app.getHostCapabilities()?.experimental?.zoen;
  const marker = extension && "canonicalScene" in extension ? extension.canonicalScene : null;
  if (!marker || typeof marker !== "object" || !("version" in marker) || !("itemId" in marker))
    return null;
  return marker.version === 1 &&
    typeof marker.itemId === "string" &&
    /^[A-Za-z0-9_-]{1,128}$/.test(marker.itemId)
    ? { version: 1, itemId: marker.itemId }
    : null;
}
const copy = {
  en: {
    loading: "Opening drawing…",
    saved: "Saved in this chat",
    pending: "Changes waiting to save",
    saving: "Saving…",
    retry: "Try again",
    failed: "Your changes remain open. Try saving again.",
    unavailable: "The drawing could not be opened.",
    importing: "Importing image… Changes waiting to save",
    importFailed: "The image is still importing. Your drawing remains open. Try again.",
  },
  pt: {
    loading: "Abrindo desenho…",
    saved: "Salvo nesta conversa",
    pending: "Alterações aguardando salvar",
    saving: "Salvando…",
    retry: "Tentar novamente",
    failed: "Suas alterações continuam abertas. Tente salvar novamente.",
    unavailable: "Não foi possível abrir o desenho.",
    importing: "Importando imagem… Alterações aguardando salvar",
    importFailed:
      "A imagem ainda está sendo importada. Seu desenho continua aberto. Tente novamente.",
  },
};

export function CanonicalExcalidrawApp({ app, itemId }: { app: App; itemId: string }) {
  const labels = (app.getHostContext()?.locale ?? "").toLowerCase().startsWith("pt")
    ? copy.pt
    : copy.en;
  const [attempt, retry] = useState(0);
  const [load, setLoad] = useState<{ json?: string; error?: string }>({});
  useEffect(() => {
    let active = true;
    const check = () => {
      if (!active) throw new Error("This drawing has closed.");
    };
    const transport = canonicalSceneTransport(app, itemId, check);
    setLoad({});
    void transport
      .read()
      .then((json) => {
        if (active) setLoad({ json });
      })
      .catch((error) => {
        if (active) setLoad({ error: error instanceof Error ? error.message : labels.unavailable });
      });
    return () => {
      active = false;
    };
  }, [app, itemId, attempt]);
  if (!load.json)
    return (
      <main className="canonical-loading" aria-live="polite">
        {load.error ?? labels.loading}
        {load.error && (
          <button className="app-button" onClick={() => retry((n) => n + 1)}>
            {labels.retry}
          </button>
        )}
      </main>
    );
  return (
    <CanonicalCanvas
      key={`${itemId}:${attempt}`}
      app={app}
      itemId={itemId}
      initial={load.json}
      labels={labels}
    />
  );
}

function CanonicalCanvas({
  app,
  itemId,
  initial,
  labels,
}: {
  app: App;
  itemId: string;
  initial: string;
  labels: typeof copy.en;
}) {
  const api = useRef<ExcalidrawImperativeAPI | null>(null);
  const initialized = useRef(false);
  const live = useRef(true);
  const [localError, setLocalError] = useState<string | null>(null);
  const [frozen, setFrozen] = useState(false);
  const [importing, setImporting] = useState(false);
  const importPending = useRef(false);
  const imageWaiters = useRef(new Set<{ resolve: () => void; reject: (error: Error) => void }>());
  const action = useRef<"copy" | "teardown" | null>(null);
  const freezing = useRef<"copy" | "teardown" | null>(null);
  const freezeAck = useRef<{ resolve: () => void; reject: (error: Error) => void } | null>(null);
  const captureError = useRef<string | null>(null);
  const [scene] = useState(() => JSON.parse(initial));
  const report = (state: SceneSaveState) => {
    if (!live.current) return;
    if (importPending.current && state.phase !== "error") state = { ...state, phase: "pending" };
    // This updates host context only; scene content never enters a model message.
    void app
      .updateModelContext({ structuredContent: { zoen: { sceneSave: state } } })
      .catch(() => {});
  };
  const [transport] = useState(() =>
    canonicalSceneTransport(app, itemId, () => {
      if (!live.current) throw new Error("This drawing has closed.");
    }),
  );
  const [saves] = useState(() =>
    createSceneSaves({
      initial,
      save: transport.save,
      schedule(work) {
        const timer = setTimeout(work, 400);
        return () => clearTimeout(timer);
      },
      publish: report,
    }),
  );
  const state = useSyncExternalStore(saves.subscribe, saves.getSnapshot, saves.getSnapshot);
  const deferImages = () => {
    importPending.current = true;
    setImporting(true);
    // Pause synchronously, including an older complete scene's pending debounce.
    void saves.pause().catch(() => {});
    report(saves.getSnapshot());
  };
  const completeImages = () => {
    const wasPending = importPending.current;
    importPending.current = false;
    setImporting(false);
    for (const waiter of imageWaiters.current) waiter.resolve();
    imageWaiters.current.clear();
    if (!action.current) saves.resume();
    return wasPending;
  };
  const remember = (json: string) => {
    const wasPending = completeImages();
    if (!initialized.current) {
      initialized.current = true;
      saves.baseline(json);
    } else saves.capture(json);
    captureError.current = null;
    setLocalError(null);
    if (wasPending && !action.current && saves.getSnapshot().phase !== "error")
      void saves.flush().catch(() => {});
  };
  const capture = (
    elements: readonly ExcalidrawElement[],
    appState: AppState,
    files: BinaryFiles,
  ) => {
    if (!live.current) return;
    if (freezing.current) {
      if (appState.viewModeEnabled) {
        freezeAck.current?.resolve();
        freezeAck.current = null;
      }
      return;
    }
    try {
      const json = serializeCanonicalScene(elements, appState, files);
      remember(json);
    } catch (error) {
      if (error instanceof PendingSceneImages) {
        deferImages();
        return;
      }
      const message = error instanceof Error ? error.message : labels.failed;
      captureError.current = message;
      setLocalError(message);
      report({ phase: "error", sequence: state.sequence, error: message });
    }
  };
  const captureNow = () => {
    if (!api.current || !initialized.current) throw new Error("The drawing is still opening.");
    try {
      const json = serializeCanonicalScene(
        api.current.getSceneElements(),
        api.current.getAppState(),
        api.current.getFiles(),
      );
      remember(json);
      return true;
    } catch (error) {
      if (!(error instanceof PendingSceneImages)) throw error;
      deferImages();
      return false;
    }
  };
  const awaitImages = async () => {
    if (captureNow()) return;
    try {
      await new Promise<void>((resolve, reject) => {
        const finish = (error?: Error) => {
          clearTimeout(timer);
          imageWaiters.current.delete(waiter);
          if (error) reject(error);
          else resolve();
        };
        const waiter = { resolve: () => finish(), reject: (error: Error) => finish(error) };
        const timer = setTimeout(
          () => finish(new Error(labels.importFailed)),
          IMAGE_IMPORT_DEADLINE_MS,
        );
        imageWaiters.current.add(waiter);
      });
      if (!live.current) throw new Error("This drawing has closed.");
      if (!captureNow()) throw new Error(labels.importFailed);
    } catch (error) {
      if (live.current) {
        const message = error instanceof Error ? error.message : labels.importFailed;
        const current = saves.getSnapshot();
        if (current.phase !== "error") {
          setLocalError(message);
          report({ phase: "error", sequence: current.sequence, error: message });
        }
      }
      throw error;
    }
  };
  const flush = async () => {
    // Pointer/visibility callbacks must never unpause a copy or navigation barrier.
    if (action.current) return;
    await awaitImages();
    if (!live.current || action.current) return;
    await saves.flush();
  };
  useEffect(() => {
    const owner = {};
    owners.set(app, owner);
    live.current = true;
    report(saves.getSnapshot());
    const check = () => {
      if (!live.current || owners.get(app) !== owner) throw new Error("This drawing has closed.");
    };
    const freeze = async (mode: "copy" | "teardown") => {
      check();
      if (action.current) throw new Error("This drawing action is already in progress.");
      if (!api.current || !initialized.current) throw new Error("The drawing is still opening.");
      action.current = mode;
      await Promise.all([saves.pause(), awaitImages()]);
      check();
      freezing.current = mode;
      const acknowledged = api.current.getAppState().viewModeEnabled
        ? Promise.resolve()
        : new Promise<void>((resolve, reject) => {
            freezeAck.current = { resolve, reject };
          });
      setFrozen(true);
      if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
      await Promise.all([saves.pause(), acknowledged]);
      check();
      // An import can start between readiness and the positive view-only ACK.
      if (!captureNow()) throw new Error(labels.importFailed);
    };
    const resume = () => {
      check();
      if (action.current === "copy") throw new Error("A drawing copy is already prepared.");
      freezing.current = null;
      action.current = null;
      if (!importPending.current) saves.resume();
      setFrozen(false);
    };
    let preparedCopy: Awaited<ReturnType<typeof transport.prepareCopy>> | null = null;
    let copyWork: Promise<Awaited<ReturnType<typeof transport.prepareCopy>>> | null = null;
    app.oncalltool = async ({ name }) => {
      check();
      if (name === "resume_scene") {
        if (action.current && !freezing.current)
          throw new Error("This drawing action is already in progress.");
        resume();
        teardown = null;
        return { content: [], structuredContent: {} };
      }
      if (name !== "prepare_scene_copy") throw new Error("This drawing action is unavailable.");
      if (preparedCopy) return { content: [], structuredContent: preparedCopy };
      if (!copyWork) {
        if (action.current) throw new Error("This drawing action is already in progress.");
        copyWork = (async () => {
          let staged = false;
          try {
            await freeze("copy");
            const current = api.current!;
            const json = serializeCanonicalScene(
              current.getSceneElements(),
              current.getAppState(),
              current.getFiles(),
            );
            const result = await transport.prepareCopy(json);
            preparedCopy = result;
            staged = true;
            return result;
          } finally {
            if (!staged && live.current && owners.get(app) === owner && action.current === "copy") {
              freezing.current = null;
              action.current = null;
              if (!importPending.current) saves.resume();
              setFrozen(false);
            }
          }
        })();
      }
      try {
        return { content: [], structuredContent: await copyWork };
      } finally {
        if (!preparedCopy) copyWork = null;
      }
    };
    let teardown: Promise<void> | null = null;
    app.onteardown = async () => {
      check();
      if (!teardown) {
        const work = (async () => {
          await freeze("teardown");
          // No editor events can change this snapshot after the positive view-only ACK.
          captureNow();
          await saves.flush();
          await saves.pause();
          check();
        })();
        teardown = work;
        void work.catch(() => {
          if (live.current && owners.get(app) === owner && action.current === "teardown") resume();
          if (teardown === work) teardown = null;
        });
      }
      await teardown;
      return {};
    };
    const background = () => {
      if (document.visibilityState === "hidden") void flush().catch(() => {});
    };
    document.addEventListener("visibilitychange", background);
    return () => {
      live.current = false;
      for (const waiter of imageWaiters.current)
        waiter.reject(new Error("This drawing has closed."));
      imageWaiters.current.clear();
      freezeAck.current?.reject(new Error("This drawing has closed."));
      freezeAck.current = null;
      saves.dispose();
      document.removeEventListener("visibilitychange", background);
      if (owners.get(app) === owner) owners.delete(app);
    };
  }, [app, saves]);
  const message =
    localError ??
    (state.phase === "error"
      ? `${labels.failed} ${state.error ?? ""}`
      : importing
        ? labels.importing
        : labels[state.phase]);
  return (
    <main className="canonical-app" data-canonical-editor="true">
      <span role="status" aria-live="polite" className="canonical-screen-reader-status">
        {message}
      </span>
      <div
        className="canonical-canvas"
        onPointerUp={() => {
          void flush().catch(() => {});
        }}
      >
        <Excalidraw
          excalidrawAPI={(value) => {
            api.current = value;
          }}
          initialData={{ ...scene, scrollToContent: true }}
          onChange={capture}
          langCode={
            (app.getHostContext()?.locale ?? "").toLowerCase().startsWith("pt") ? "pt-BR" : "en"
          }
          theme={app.getHostContext()?.theme === "dark" ? "dark" : "light"}
          aiEnabled={false}
          viewModeEnabled={frozen}
          UIOptions={{
            canvasActions: {
              loadScene: false,
              saveToActiveFile: false,
              saveAsImage: false,
              export: false,
            },
          }}
          validateEmbeddable={false}
          onLinkOpen={(_element, event) => {
            event.preventDefault();
          }}
        />
      </div>
    </main>
  );
}
