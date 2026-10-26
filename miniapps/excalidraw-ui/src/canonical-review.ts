import { exportToCanvas, restore } from "@excalidraw/excalidraw";
import type { OrderedExcalidrawElement, NonDeleted } from "@excalidraw/excalidraw/element/types";
import { sceneBytes, sceneHash } from "./canonical-transfer";

export const REVIEW_SCENE_BYTES = 32 * 1024;
export const REVIEW_PNG_BYTES = 128 * 1024;
const token = (v: unknown): v is string =>
  typeof v === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(v);
const digest = (v: unknown): v is string => typeof v === "string" && /^[a-f0-9]{64}$/.test(v);
const record = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === "object" && !Array.isArray(v);
function scene(json: unknown, sha: unknown) {
  if (typeof json !== "string" || json.length > REVIEW_SCENE_BYTES || !digest(sha))
    throw Error("Invalid drawing review.");
  const bytes = sceneBytes(json);
  if (bytes.length > REVIEW_SCENE_BYTES || sceneHash(bytes) !== sha)
    throw Error("The reviewed scene checksum does not match.");
  const value = JSON.parse(json);
  for (const file of Object.values(value.files))
    if (
      !record(file) ||
      typeof file.dataURL !== "string" ||
      !file.dataURL.startsWith("data:image/")
    )
      throw Error("The reviewed drawing requires an unavailable image.");
  return restore(value, null, null, { repairBindings: true });
}
const base64 = (bytes: Uint8Array) => {
  let value = "";
  for (let at = 0; at < bytes.length; at += 16_384)
    value += String.fromCharCode(...bytes.subarray(at, at + 16_384));
  return btoa(value);
};

/** Offscreen upstream export only. This path never calls the open canvas API or saves. */
export function createSceneReviewExport(itemId: string, checkOwner: () => void) {
  let active = true,
    busy = false;
  const check = () => {
    if (!active) throw Error("This drawing has closed.");
    checkOwner();
  };
  const render = async (restored: ReturnType<typeof scene>, sha: unknown) => {
    check();
    const elements = restored.elements.filter(
      (e): e is NonDeleted<OrderedExcalidrawElement> => !e.isDeleted,
    );
    for (const maximum of [896, 640, 384]) {
      check();
      const canvas: HTMLCanvasElement = await exportToCanvas({
        elements,
        appState: { ...restored.appState, exportBackground: true },
        files: restored.files,
        maxWidthOrHeight: maximum,
        exportPadding: 20,
      });
      try {
        check();
        if (
          !Number.isSafeInteger(canvas.width) ||
          !Number.isSafeInteger(canvas.height) ||
          canvas.width < 1 ||
          canvas.height < 1 ||
          canvas.width > maximum ||
          canvas.height > maximum
        )
          throw Error("Invalid drawing preview dimensions.");
        const blob = await new Promise<Blob>((resolve, reject) => {
          canvas.toBlob(
            (value) =>
              value ? resolve(value) : reject(Error("The drawing preview could not be created.")),
            "image/png",
          );
        });
        check();
        if (blob.type !== "image/png") throw Error("Invalid drawing preview format.");
        if (!blob.size || blob.size > REVIEW_PNG_BYTES) continue;
        const bytes = new Uint8Array(await blob.arrayBuffer());
        check();
        return {
          sceneSha256: sha,
          imageSha256: sceneHash(bytes),
          mimeType: "image/png",
          width: canvas.width,
          height: canvas.height,
          dataBase64: base64(bytes),
        };
      } finally {
        canvas.width = 0;
        canvas.height = 0;
      }
    }
    throw Error("The drawing preview exceeds its image limit.");
  };
  return {
    async render(value: unknown) {
      check();
      if (busy) throw Error("Another drawing review is still opening.");
      if (
        !record(value) ||
        Object.keys(value).length !== 10 ||
        !token(value.nonce) ||
        value.artifactId !== itemId ||
        !token(value.requestId) ||
        !digest(value.contentHash) ||
        typeof value.resourceUri !== "string" ||
        !value.resourceUri.startsWith("ui://zoen/package/") ||
        value.resourceUri.length > 256 ||
        !Number.isSafeInteger(value.baseVersion) ||
        Number(value.baseVersion) < 1
      )
        throw Error("Invalid drawing review binding.");
      busy = true;
      try {
        const beforeScene = scene(value.beforeSceneJson, value.beforeSha256);
        const afterScene = scene(value.afterSceneJson, value.afterSha256);
        const before = await render(beforeScene, value.beforeSha256);
        const after = await render(afterScene, value.afterSha256);
        check();
        return {
          content: [],
          structuredContent: {
            nonce: value.nonce,
            artifactId: itemId,
            requestId: value.requestId,
            contentHash: value.contentHash,
            resourceUri: value.resourceUri,
            baseVersion: value.baseVersion,
            renderer: "excalidraw@0.18.0",
            before,
            after,
          },
        };
      } finally {
        busy = false;
      }
    },
    dispose() {
      active = false;
    },
  };
}
