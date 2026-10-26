import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import type { App } from "@modelcontextprotocol/ext-apps";
import type { AppState, BinaryFiles, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import type { CanonicalLabels } from "./canonical-app";
import { canonicalSceneTransport, type SceneDocument } from "./canonical-transfer";
import { createSceneSaves, type SceneSaveState } from "./canonical-saves";
import { serializeCanonicalScene, PendingSceneImages } from "./canonical-serialize";
import { createSceneReviewExport } from "./canonical-review";
import { createSceneApplication } from "./canonical-application";
const owners = new WeakMap<App, object>();
const IMAGE_IMPORT_DEADLINE_MS = 10_000;
export function CanonicalCanvas({
  app,
  itemId,
  initial,
  labels,
}: {
  app: App;
  itemId: string;
  initial: SceneDocument;
  labels: CanonicalLabels;
}) {
  const api = useRef<ExcalidrawImperativeAPI | null>(null);
  const initialized = useRef(false);
  const protocolReady = useRef(false);
  const refreshPrepared = useRef(false);
  const live = useRef(true);
  const [localError, setLocalError] = useState<string | null>(null);
  const [frozen, setFrozen] = useState(false);
  const [importing, setImporting] = useState(false);
  const importPending = useRef(false);
  const imageWaiters = useRef(new Set<{ resolve: () => void; reject: (error: Error) => void }>());
  const action = useRef<"copy" | "teardown" | "refresh" | null>(null);
  const freezing = useRef<"copy" | "teardown" | "refresh" | null>(null);
  const freezeAck = useRef<{ resolve: () => void; reject: (error: Error) => void } | null>(null);
  const captureError = useRef<string | null>(null);
  const [scene] = useState(() => JSON.parse(initial.json));
  const head = useRef({ version: initial.version, sha256: initial.sha256 });
  const [application] = useState(() =>
    createSceneApplication(() => {
      if (!live.current) throw new Error("This drawing has closed.");
    }),
  );
  const reportState = (state: SceneSaveState) => ({
    ...state,
    itemId,
    liveRefresh: 1,
    reviewPreview: 1,
    ready: initialized.current && protocolReady.current,
    clean: initialized.current && protocolReady.current && !importPending.current && saves.clean(),
    ...head.current,
  });
  const report = (state: SceneSaveState) => {
    if (!live.current) return;
    if (importPending.current && state.phase !== "error") state = { ...state, phase: "pending" };
    // This updates host context only; scene content never enters a model message.
    void app
      .updateModelContext({ structuredContent: { zoen: { sceneSave: reportState(state) } } })
      .catch(() => {});
  };
  const [transport] = useState(() =>
    canonicalSceneTransport(app, itemId, () => {
      if (!live.current) throw new Error("This drawing has closed.");
    }),
  );
  const [saves] = useState(() =>
    createSceneSaves({
      initial: initial.json,
      save: async (json, mutation) => {
        const accepted = await transport.save(json, mutation);
        head.current = { version: accepted.version, sha256: accepted.sha256 };
        return accepted;
      },
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
      report(saves.getSnapshot());
    } else saves.capture(json);
    report(saves.getSnapshot());
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
      application.observe(elements, appState, files);
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
    const review = createSceneReviewExport(itemId, check);
    const freeze = async (mode: "copy" | "teardown" | "refresh") => {
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
      refreshPrepared.current = false;
      if (!importPending.current) saves.resume();
      setFrozen(false);
    };
    let preparedCopy: Awaited<ReturnType<typeof transport.prepareCopy>> | null = null;
    let copyWork: Promise<Awaited<ReturnType<typeof transport.prepareCopy>>> | null = null;
    app.oncalltool = async ({ name, arguments: args }) => {
      check();
      if (name === "render_scene_review") return review.render(args);
      if (name === "resume_scene") {
        if (action.current && !freezing.current)
          throw new Error("This drawing action is already in progress.");
        resume();
        teardown = null;
        await app.updateModelContext({
          structuredContent: { zoen: { sceneSave: reportState(saves.getSnapshot()) } },
        });
        check();
        return { content: [], structuredContent: {} };
      }
      if (name === "refresh_scene") {
        const value = args as Record<string, unknown> | undefined;
        if (
          typeof value?.refreshId !== "string" ||
          !Number.isSafeInteger(value.version) ||
          typeof value.sha256 !== "string"
        )
          throw new Error("Invalid drawing refresh.");
        if (action.current) {
          if (action.current === "refresh" && refreshPrepared.current) resume();
          else throw new Error("This drawing action is already in progress.");
        }
        // Capture any input delivered before the host's request; a saved phase alone is insufficient.
        captureNow();
        if (!saves.clean() || importPending.current)
          throw new Error("Your drawing has unsaved changes.");
        let applying = false;
        try {
          await freeze("refresh");
          check();
          if (!saves.clean() || importPending.current)
            throw new Error("Your drawing has unsaved changes.");
          const candidate = await transport.readDocument(value.refreshId);
          check();
          if (candidate.version !== value.version || candidate.sha256 !== value.sha256)
            throw new Error("The shared drawing changed while opening.");
          applying = true;
          const json = await application.apply(api.current!, candidate.json);
          check();
          saves.rebase(json);
          head.current = { version: candidate.version, sha256: candidate.sha256 };
          await app.updateModelContext({
            structuredContent: { zoen: { sceneSave: reportState(saves.getSnapshot()) } },
          });
          check();
          refreshPrepared.current = true;
          // The host resumes this same frame only after accepting the canvas ACK.
          return {
            content: [],
            structuredContent: { refreshId: value.refreshId, itemId, ...head.current },
          };
        } catch (error) {
          if (live.current && owners.get(app) === owner) {
            // Keep any partially applied scene editable/copyable. The host retains its
            // old CAS base, so a later save cannot overwrite a newer shared version.
            resume();
            if (applying) {
              const message = error instanceof Error ? error.message : labels.failed;
              setLocalError(message);
              report({ phase: "error", sequence: saves.getSnapshot().sequence, error: message });
            }
          }
          throw error;
        }
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
    protocolReady.current = true;
    report(saves.getSnapshot());
    const background = () => {
      if (document.visibilityState === "hidden") void flush().catch(() => {});
    };
    document.addEventListener("visibilitychange", background);
    return () => {
      live.current = false;
      protocolReady.current = false;
      for (const waiter of imageWaiters.current)
        waiter.reject(new Error("This drawing has closed."));
      imageWaiters.current.clear();
      freezeAck.current?.reject(new Error("This drawing has closed."));
      freezeAck.current = null;
      review.dispose();
      application.dispose();
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
