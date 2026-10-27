import { sha256 } from "@noble/hashes/sha2.js";
import type { App } from "@modelcontextprotocol/ext-apps";

export const MAX_SCENE_BYTES = 16 * 1024 * 1024;
export const CHUNK_BYTES = 196_608;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
export const sceneHash = (bytes: Uint8Array) =>
  Array.from(sha256(bytes), (n) => n.toString(16).padStart(2, "0")).join("");
export function sceneBytes(json: string): Uint8Array {
  const bytes = encoder.encode(json);
  if (!bytes.length || bytes.length > MAX_SCENE_BYTES)
    throw new Error("The drawing exceeds the 16 MiB limit.");
  const value = JSON.parse(json);
  if (
    !record(value) ||
    !Array.isArray(value.elements) ||
    !record(value.appState) ||
    !record(value.files)
  )
    throw new Error("Invalid drawing document.");
  return bytes;
}
function base64(bytes: Uint8Array): string {
  let text = "";
  for (let at = 0; at < bytes.length; at += 16_384)
    text += String.fromCharCode(...bytes.subarray(at, at + 16_384));
  return btoa(text);
}
function unbase64(text: unknown, maximum: number): Uint8Array {
  if (
    typeof text !== "string" ||
    text.length > Math.ceil(maximum / 3) * 4 ||
    !/^[A-Za-z0-9+/]*={0,2}$/.test(text) ||
    text.length % 4
  )
    throw new Error("Invalid drawing chunk.");
  const raw = atob(text);
  if (raw.length > maximum) throw new Error("Invalid drawing chunk.");
  return Uint8Array.from(raw, (letter) => letter.charCodeAt(0));
}
export function sceneMutationId(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (n) =>
    n.toString(16).padStart(2, "0"),
  ).join("");
}
export type SceneDocument = {
  json: string;
  version: number;
  currentVersion: number;
  sha256: string;
};
export type SceneAck = {
  checkpointId: string;
  version: number;
  currentVersion: number;
  sha256: string;
};
export function canonicalSceneTransport(
  app: Pick<App, "callServerTool">,
  id: string,
  check: () => void,
) {
  let failedUpload: string | null = null;
  let copyAttempt: { json: string; mutation: string } | null = null;
  const call = async (name: string, args: Record<string, unknown>) => {
    check();
    const response = await app.callServerTool({ name, arguments: args });
    check();
    if (response.isError) {
      const text = response.content.find((part) => part.type === "text");
      throw new Error(text?.type === "text" ? text.text : "The drawing could not be saved.");
    }
    if (!record(response.structuredContent)) throw new Error("Invalid drawing response.");
    return response.structuredContent;
  };
  const upload = async (json: string, mutationId: string) => {
    const bytes = sceneBytes(json),
      digest = sceneHash(bytes);
    const begin = await call("app_scene_begin", {
      id,
      mutationId,
      bytes: bytes.length,
      sha256: digest,
    });
    if (typeof begin.uploadId !== "string" || begin.chunkBytes !== CHUNK_BYTES)
      throw new Error("Invalid drawing upload.");
    failedUpload = begin.uploadId;
    for (let at = 0, index = 0; at < bytes.length; at += CHUNK_BYTES, index++) {
      const reply = await call("app_scene_append", {
        id,
        uploadId: begin.uploadId,
        index,
        data: base64(bytes.subarray(at, at + CHUNK_BYTES)),
      });
      if (reply.nextIndex !== index + 1) throw new Error("Invalid drawing upload order.");
    }
    return { uploadId: begin.uploadId, bytes: bytes.length, sha256: digest };
  };
  const readDocument = async (refreshId?: string): Promise<SceneDocument> => {
    const meta = await call("app_scene_read", { id, ...(refreshId ? { refreshId } : {}) });
    if (
      typeof meta.transferId !== "string" ||
      typeof meta.bytes !== "number" ||
      !Number.isSafeInteger(meta.version) ||
      Number(meta.version) < 1 ||
      meta.currentVersion !== meta.version ||
      typeof meta.sha256 !== "string" ||
      !Number.isSafeInteger(meta.bytes) ||
      meta.bytes <= 0 ||
      meta.bytes > MAX_SCENE_BYTES ||
      meta.chunkBytes !== CHUNK_BYTES ||
      !/^[a-f0-9]{64}$/.test(meta.sha256)
    )
      throw new Error("Invalid drawing manifest.");
    const bytes = new Uint8Array(meta.bytes);
    for (let index = 0, at = 0; at < bytes.length; index++) {
      const chunk = await call("app_scene_read_chunk", {
        id,
        transferId: meta.transferId,
        index,
      });
      const expected = Math.min(CHUNK_BYTES, bytes.length - at);
      const next = unbase64(chunk.data, expected);
      if (chunk.index !== index || next.length !== expected)
        throw new Error("Invalid drawing chunk order.");
      bytes.set(next, at);
      at += next.length;
    }
    if (sceneHash(bytes) !== meta.sha256) throw new Error("The drawing checksum does not match.");
    const text = decoder.decode(bytes);
    sceneBytes(text);
    return {
      json: text,
      version: Number(meta.version),
      currentVersion: Number(meta.currentVersion),
      sha256: meta.sha256,
    };
  };
  return {
    readDocument,
    async read(): Promise<string> {
      return (await readDocument()).json;
    },
    async save(json: string, mutationId: string): Promise<SceneAck> {
      const staged = await upload(json, mutationId);
      const digest = staged.sha256;
      const accepted = await call("app_scene_commit", { id, uploadId: staged.uploadId });
      if (
        accepted.checkpointId !== id ||
        accepted.sha256 !== digest ||
        typeof accepted.version !== "number" ||
        typeof accepted.currentVersion !== "number" ||
        !Number.isSafeInteger(accepted.version) ||
        !Number.isSafeInteger(accepted.currentVersion) ||
        accepted.version <= 0 ||
        accepted.currentVersion < accepted.version
      )
        throw new Error("Invalid drawing save acknowledgment.");
      if (accepted.currentVersion !== accepted.version)
        throw new Error("The shared drawing has a newer version. Your changes remain open.");
      failedUpload = null;
      return accepted as SceneAck;
    },
    async prepareCopy(json: string) {
      check();
      if (failedUpload) {
        await call("app_scene_abort", { id, uploadId: failedUpload });
        failedUpload = null;
      }
      if (!copyAttempt || copyAttempt.json !== json)
        copyAttempt = { json, mutation: sceneMutationId() };
      const staged = await upload(copyAttempt.json, copyAttempt.mutation);
      failedUpload = null;
      return staged;
    },
  };
}
