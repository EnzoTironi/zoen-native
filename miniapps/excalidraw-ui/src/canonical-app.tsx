import { useEffect, useState } from "react";
import type { App } from "@modelcontextprotocol/ext-apps";
import { canonicalSceneTransport, type SceneDocument } from "./canonical-transfer";
import { CanonicalCanvas } from "./canonical-canvas";
export { serializeCanonicalScene } from "./canonical-serialize";
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

export type CanonicalLabels = typeof copy.en;

export function CanonicalExcalidrawApp({ app, itemId }: { app: App; itemId: string }) {
  const labels = (app.getHostContext()?.locale ?? "").toLowerCase().startsWith("pt")
    ? copy.pt
    : copy.en;
  const [attempt, retry] = useState(0);
  const [load, setLoad] = useState<{ doc?: SceneDocument; error?: string }>({});
  useEffect(() => {
    let active = true;
    const check = () => {
      if (!active) throw new Error("This drawing has closed.");
    };
    const transport = canonicalSceneTransport(app, itemId, check);
    setLoad({});
    void transport
      .readDocument()
      .then((doc) => {
        if (active) setLoad({ doc });
      })
      .catch((error) => {
        if (active) setLoad({ error: error instanceof Error ? error.message : labels.unavailable });
      });
    return () => {
      active = false;
    };
  }, [app, itemId, attempt]);
  if (!load.doc)
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
      initial={load.doc}
      labels={labels}
    />
  );
}
