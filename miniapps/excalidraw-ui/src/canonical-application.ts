import { restore, CaptureUpdateAction } from "@excalidraw/excalidraw";
import type { AppState, BinaryFiles, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import { serializeCanonicalScene } from "./canonical-serialize";

/** An imperative API call is not proof that Excalidraw applied a scene. Wait for onChange. */
export function createSceneApplication(check: () => void) {
  let pending: {
    json: string;
    resolve: (json: string) => void;
    reject: (error: Error) => void;
  } | null = null;
  return {
    async apply(api: ExcalidrawImperativeAPI, json: string): Promise<string> {
      check();
      if (pending) throw new Error("A drawing update is already in progress.");
      const restored = restore(JSON.parse(json), null, null, { repairBindings: true });
      const expected = serializeCanonicalScene(
        restored.elements,
        restored.appState as AppState,
        restored.files,
      );
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => {
          pending = null;
          reject(new Error("The canvas did not acknowledge the shared drawing."));
        }, 10_000);
        pending = {
          json: expected,
          resolve: () => {
            clearTimeout(timer);
            pending = null;
            resolve();
          },
          reject: (error) => {
            clearTimeout(timer);
            pending = null;
            reject(error);
          },
        };
        try {
          api.addFiles(Object.values(restored.files));
          api.updateScene({
            elements: restored.elements,
            appState: restored.appState,
            captureUpdate: CaptureUpdateAction.NEVER,
          });
        } catch (error) {
          pending?.reject(
            error instanceof Error ? error : new Error("The shared drawing could not be applied."),
          );
        }
      });
      check();
      return expected;
    },
    observe(elements: readonly ExcalidrawElement[], state: AppState, files: BinaryFiles) {
      if (!pending) return;
      check();
      try {
        const json = serializeCanonicalScene(elements, state, files);
        if (json === pending.json) pending.resolve(json);
      } catch {
        /* A partial image/application callback is not an ACK. */
      }
    },
    dispose() {
      pending?.reject(new Error("This drawing has closed."));
      pending = null;
    },
  };
}
