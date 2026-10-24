import { useHostSafeArea } from "./host-safe-area";
import { CanonicalExcalidrawApp, canonicalSceneMarker } from './canonical-app';
import { useApp } from "@modelcontextprotocol/ext-apps/react";
import type { App } from "@modelcontextprotocol/ext-apps";
import {  Excalidraw, restore, CaptureUpdateAction, serializeAsJSON, MainMenu } from "@excalidraw/excalidraw";
import { useCallback, useEffect, useRef, useState } from "react";
import { onEditorChange, setStorageKey, loadPersistedElements, getLatestEditedElements, setCheckpointId } from "./edit-context";
import "./global.css";
import { DiagramView } from "./mcp-diagram";
import type { ViewportRect } from "./mcp-diagram-scene";
import { ExpandIcon, ShareButton, excalidrawLogo, githubIcon, twitterIcon, discordIcon } from "./mcp-chrome";

// ============================================================
// Debug logging (routes through SDK → host log file)
// ============================================================

let _logFn: ((msg: string) => void) | null = null;
function fsLog(msg: string) {
  if (_logFn) _logFn(msg);
}

type PendingToolEvents = {
  partialInput?: any;
  input?: any;
  result?: any;
  hostContext?: any;
};

const pendingToolEvents = new WeakMap<App, PendingToolEvents>();

/**
 * Tool notifications can arrive immediately after the MCP Apps handshake.
 * Buffer them until ExcalidrawAppCore mounts and installs the React handlers.
 */
function bufferInitialToolEvents(app: App) {
  const pending: PendingToolEvents = {};
  pendingToolEvents.set(app, pending);

  app.ontoolinputpartial = (input) => {
    pending.partialInput = input;
  };
  app.ontoolinput = (input) => {
    pending.input = input;
  };
  app.ontoolresult = (result) => {
    pending.result = result;
  };
  app.onhostcontextchanged = (context) => {
    pending.hostContext = { ...pending.hostContext, ...context };
  };
}

// ============================================================
// Shared helpers
// ============================================================

async function shareToExcalidraw(data: {elements: any[], appState: any, files: any}, app: App) {
  try {
    if (!data.elements?.length) return;

    // Serialize to Excalidraw JSON
    const json = serializeAsJSON(data.elements, data.appState, data.files, "database");

    // Proxy through server tool (avoids CORS on json.excalidraw.com)
    const result = await app.callServerTool({
      name: "export_to_excalidraw",
      arguments: { json },
    });

    if (result.isError) {
      fsLog(`export failed: ${JSON.stringify(result.content)}`);
      return;
    }

    const url = (result.content[0] as any).text;
    await app.openLink({ url });
  } catch (err) {
    fsLog(`shareToExcalidraw error: ${err}`);
  }
}

// ============================================================
// Diagram component (Excalidraw SVG)
// ============================================================

// ============================================================
// Main app — Excalidraw only
// ============================================================

export function ExcalidrawAppCore({ app }: { app: App }) {
  const [toolInput, setToolInput] = useState<any>(null);
  const [inputIsFinal, setInputIsFinal] = useState(false);
  const [displayMode, setDisplayMode] = useState<"inline" | "fullscreen">(() =>
    app.getHostContext()?.displayMode === "fullscreen" ? "fullscreen" : "inline"
  );
  const [elements, setElements] = useState<any[]>([]);
  const [userEdits, setUserEdits] = useState<any[] | null>(null);
  const [containerHeight, setContainerHeight] = useState<number | null>(null);
  const [isNarrow, setIsNarrow] = useState(() => typeof window !== "undefined" && window.matchMedia("(max-width: 640px)").matches);
  const [editorReady, setEditorReady] = useState(false);
  const [excalidrawApi, setExcalidrawApi] = useState<any>(null);
  const [editorSettled, setEditorSettled] = useState(false);
  const appRef = useRef<App | null>(null);
  const svgViewportRef = useRef<ViewportRect | null>(null);
  const elementsRef = useRef<any[]>([]);
  const checkpointIdRef = useRef<string | null>(null);

  const toggleFullscreen = useCallback(async () => {
    if (!appRef.current) return;
    const newMode = displayMode === "fullscreen" ? "inline" : "fullscreen";
    fsLog(`toggle: ${displayMode}→${newMode}`);
    // Sync edited elements before leaving fullscreen
    if (newMode === "inline") {
      const edited = getLatestEditedElements();
      if (edited) {
            setElements(edited);
        setUserEdits(edited);
      }
    }
    try {
      const result = await appRef.current.requestDisplayMode({ mode: newMode });
      fsLog(`requestDisplayMode result: ${result.mode}`);
      setDisplayMode(result.mode as "inline" | "fullscreen");
    } catch (err) {
      fsLog(`requestDisplayMode FAILED: ${err}`);
    }
  }, [displayMode, elements.length, inputIsFinal]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && displayMode === "fullscreen") toggleFullscreen();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [displayMode, toggleFullscreen]);

  // Preload ALL Excalidraw fonts on first mount (inline mode) so they're
  // cached before fullscreen. Without this, Excalidraw's component init
  // downloads Assistant fonts, triggering a font recalc that corrupts
  // text dimensions measured with not-yet-loaded Excalifont.
  useEffect(() => {
    Promise.all([
      document.fonts.load('20px Excalifont'),
      document.fonts.load('400 16px Assistant'),
      document.fonts.load('500 16px Assistant'),
      document.fonts.load('700 16px Assistant'),
    ]).catch(() => {});
  }, []);

  // Set explicit height on html/body in fullscreen (position:fixed doesn't give body height in iframes)
  useEffect(() => {
    if (displayMode === "fullscreen" && containerHeight) {
      const h = `${containerHeight}px`;
      document.documentElement.style.height = h;
      document.body.style.height = h;
    } else {
      document.documentElement.style.height = "";
      document.body.style.height = "";
    }
  }, [displayMode, containerHeight]);

  // Mount editor when entering fullscreen
  useEffect(() => {
    if (displayMode !== "fullscreen") {
      setEditorReady(false);
      setExcalidrawApi(null);
      setEditorSettled(false);
      return;
    }
    (async () => {
      await document.fonts.ready;
      setTimeout(() => setEditorReady(true), 200);
    })();
  }, [displayMode]);

  // After editor mounts: refresh text dimensions, then reveal
  const mountEditor = displayMode === "fullscreen" && inputIsFinal && elements.length > 0 && editorReady;
  useEffect(() => {
    if (!mountEditor || !excalidrawApi) return;
    if (editorSettled) return; // already revealed, don't redo
    const api = excalidrawApi;

    const settle = async () => {
      try { await document.fonts.load('20px Excalifont'); } catch {}
      await document.fonts.ready;

      const sceneElements = api.getSceneElements();
      if (sceneElements?.length) {
        const { elements: fixed } = restore(
          { elements: sceneElements },
          null, null,
          { refreshDimensions: true }
        );
        api.updateScene({
          elements: fixed,
          captureUpdate: CaptureUpdateAction.NEVER,
        });
      }
      requestAnimationFrame(() => setEditorSettled(true));
    };

    const timer = setTimeout(settle, 200);
    return () => clearTimeout(timer);
  }, [mountEditor, excalidrawApi, editorSettled]);

  // Keep elementsRef in sync for ontoolresult handler (which captures closure once)
  useEffect(() => { elementsRef.current = elements; }, [elements]);

  // Set up MCP event handlers when app is provided
  useEffect(() => {
    appRef.current = app;
    _logFn = (msg) => { try { app.sendLog({ level: "info", logger: "FS", data: msg }); } catch {} };

    // Capture initial container dimensions
    const initCtx = app.getHostContext() as any;
    if (initCtx?.containerDimensions?.height) setContainerHeight(initCtx.containerDimensions.height);

    const handleHostContextChanged = (ctx: any) => {
      if (ctx.containerDimensions?.height) {
        setContainerHeight(ctx.containerDimensions.height);
      }
      if (ctx.displayMode) {
        fsLog(`hostContextChanged: displayMode=${ctx.displayMode}`);
        // Sync edited elements when host exits fullscreen
        if (ctx.displayMode === "inline") {
          const edited = getLatestEditedElements();
          if (edited) {
            setElements(edited);
            setUserEdits(edited);
          }
        }
        setDisplayMode(ctx.displayMode as "inline" | "fullscreen");
      }
    };

    const handleToolInputPartial = (input: any) => {
      const args = (input as any)?.arguments || input;
      setInputIsFinal(false);
      setToolInput(args);
    };

    const handleToolInput = (input: any) => {
      const args = (input as any)?.arguments || input;
      setInputIsFinal(true);
      setToolInput(args);
    };

    const handleToolResult = (result: any) => {
      const cpId = (result.structuredContent as { checkpointId?: string })?.checkpointId;
      if (cpId) {
        checkpointIdRef.current = cpId;
        setCheckpointId(cpId);
        // Use checkpointId as localStorage key for persisting user edits
        setStorageKey(cpId);
        // Check for persisted edits from a previous fullscreen session
        const persisted = loadPersistedElements();
        if (persisted && persisted.length > 0) {
          elementsRef.current = persisted;
          setElements(persisted);
          setUserEdits(persisted);
        }
      }
    };

    app.onhostcontextchanged = handleHostContextChanged;
    app.ontoolinputpartial = handleToolInputPartial;
    app.ontoolinput = handleToolInput;
    app.ontoolresult = handleToolResult;

    const pending = pendingToolEvents.get(app);
    pendingToolEvents.delete(app);
    if (pending) {
      if (pending.hostContext) handleHostContextChanged(pending.hostContext);
      if (pending.partialInput) handleToolInputPartial(pending.partialInput);
      if (pending.input) handleToolInput(pending.input);
      if (pending.result) handleToolResult(pending.result);
    }

    app.onteardown = async () => ({});
    app.onerror = (err) => console.error("[Excalidraw] Error:", err);
  }, [app]);

  // Track narrow viewport for mobile layout adjustments
  useEffect(() => {
    const mq = window.matchMedia("(max-width: 640px)");
    const handler = (e: MediaQueryListEvent) => setIsNarrow(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);


  return (
    <main className={`main${displayMode === "fullscreen" ? " fullscreen" : ""}`} style={displayMode === "fullscreen" && containerHeight ? { height: containerHeight } : undefined}>
      {displayMode === "inline" && (
        <div className="toolbar">
          <ShareButton
                onConfirm={async () => {
                  await shareToExcalidraw({
                    elements,
                    appState: {},
                    files: {}
                  }, app);
                }}
              />

          <button
            className="app-button"
            onClick={toggleFullscreen}
            title="Enter fullscreen"
          >
            <span>Edit</span>
            <ExpandIcon />
          </button>
        </div>
      )}
      {/* Editor: mount hidden when ready, reveal after viewport is set */}
      {mountEditor && (
        <div style={{
          width: "100%",
          height: "100%",
          visibility: editorSettled ? "visible" : "hidden",
          position: editorSettled ? undefined : "absolute",
          inset: editorSettled ? undefined : 0,
        }}>
          <Excalidraw
            excalidrawAPI={(api) => { setExcalidrawApi(api); fsLog(`excalidrawAPI set`); }}
            aiEnabled={false}
            initialData={{ elements: elements as any, scrollToContent: true }}
            theme="light"
            onChange={(els) => onEditorChange(app, els)}
            renderTopRightUI={isNarrow ? undefined : () => (
              <ShareButton
                onConfirm={async () => {
                  if (excalidrawApi) {
                    const elements = excalidrawApi.getSceneElements();
                    const appState = excalidrawApi.getAppState();
                    const files = excalidrawApi.getFiles();

                    await shareToExcalidraw({ elements, appState, files }, app);
                  }
                }}
              />
            )}
          >
            <MainMenu>
              <MainMenu.Item
                onSelect={() => {
                  app.openLink({
                    url: "https://plus.excalidraw.com?utm_source=mcp_app_menu"
                  })
                }}
                style={{minWidth: 200}}
              >
                {excalidrawLogo} Excalidraw
              </MainMenu.Item>
              <MainMenu.Item
                onSelect={() => {
                  app.openLink({
                    url: "https://github.com/excalidraw/excalidraw"
                  })
                }}
                style={{minWidth: 200}}
              >
                {githubIcon} GitHub
              </MainMenu.Item>
              <MainMenu.Item
                onSelect={() => {
                  app.openLink({
                    url: "https://x.com/excalidraw"
                  })
                }}
                style={{minWidth: 200}}
              >
                {twitterIcon} Follow us
              </MainMenu.Item>
              <MainMenu.Item
                onSelect={() => {
                  app.openLink({
                    url: "https://discord.gg/UexuTaE"
                  })
                }}
                style={{minWidth: 200}}
              >
                {discordIcon} Discord chat
              </MainMenu.Item>
            </MainMenu >
          </Excalidraw>
          {isNarrow && (
            <div className="mobile-share-slot">
              <ShareButton
                compact
                onConfirm={async () => {
                  if (excalidrawApi) {
                    const elements = excalidrawApi.getSceneElements();
                    const appState = excalidrawApi.getAppState();
                    const files = excalidrawApi.getFiles();
                    await shareToExcalidraw({ elements, appState, files }, app);
                  }
                }}
              />
            </div>
          )}
        </div>
      )}
      {/* SVG: stays visible until editor is fully settled */}
      {!editorSettled && (
        <div
          onClick={undefined}
          style={undefined}
        >
          <DiagramView toolInput={toolInput} isFinal={inputIsFinal} displayMode={displayMode} onElements={(els) => { elementsRef.current = els; setElements(els); }} editedElements={userEdits ?? undefined} onViewport={(vp) => { svgViewportRef.current = vp; }} loadCheckpoint={async (id) => {
            if (!appRef.current) return null;
            try {
              const result = await appRef.current.callServerTool({ name: "read_checkpoint", arguments: { id } });
              const text = (result.content[0] as any)?.text;
              if (!text) return null;
              return JSON.parse(text);
            } catch { return null; }
          }} />
        </div>
      )}
    </main>
  );
}

export function ExcalidrawApp() {
  const { app, error } = useApp({
    appInfo: { name: "Excalidraw", version: "1.0.0" },
    capabilities: { tools: {} },
    onAppCreated: bufferInitialToolEvents,
  });

  useHostSafeArea(app);

  if (error) return <div className="error">ERROR: {error.message}</div>;
  if (!app) return <div className="loading">Connecting...</div>;
  const canonical = canonicalSceneMarker(app);
  return canonical ? <CanonicalExcalidrawApp app={app} itemId={canonical.itemId} /> : <ExcalidrawAppCore app={app} />;
}
