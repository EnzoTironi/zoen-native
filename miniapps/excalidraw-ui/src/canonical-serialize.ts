import { serializeAsJSON } from "@excalidraw/excalidraw";
import type { AppState, BinaryFiles } from "@excalidraw/excalidraw/types";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
export class PendingSceneImages extends Error {}

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
