// Private HTTP adapter for the native encrypted device-history contract.
import {
  transferPutPolicy,
  transferGetPolicy,
  transferDeletePolicy,
  BlobDigest,
} from "../generated/wasm/admission.js";
import {
  policy,
  fail,
  unavailable,
  boundedStream,
  acquire,
  release,
  heldBodyStream,
  storageCall,
} from "./blob-io.mjs";
import { DeviceBlobQuota } from "./blob-quota.mjs";

const MAX_BODY = 4 * 1024 * 1024;
const OUTER_BODY = MAX_BODY + 1024;
const MAX_CHUNKS = 4096;

function response(body, status, headers = {}, preflight = false) {
  const result = new Headers(headers);
  result.set("Access-Control-Allow-Origin", "*");
  result.set("Vary", "origin, access-control-request-method, access-control-request-headers");
  if (body !== null && !result.has("Content-Type"))
    result.set("Content-Type", "text/plain; charset=utf-8");
  if (preflight) {
    result.set("Access-Control-Allow-Methods", "GET,PUT,DELETE");
    result.set("Access-Control-Allow-Headers", "content-type,x-zoen-device,x-zoen-ts,x-zoen-sig");
    result.set("Access-Control-Max-Age", "86400");
  }
  return new Response(body, { status, headers: result });
}

function decode(value, field) {
  try {
    return decodeURIComponent(value.replace(/%(?![a-fA-F0-9]{2})/g, "%25"));
  } catch {
    throw fail(400, `Invalid URL: Invalid UTF-8 in \`${field}\``);
  }
}
function chunkIndex(raw) {
  const value = decode(raw, "n");
  if (!/^\+?[0-9]+$/.test(value) || BigInt(value) > 0xffff_ffffn)
    throw fail(
      400,
      `Invalid URL: Cannot parse value at index 1 with value \`${value}\` to a \`u32\``,
    );
  return Number(value);
}
const headers = (request) =>
  ["x-zoen-device", "x-zoen-ts", "x-zoen-sig"].map((name) => request.headers.get(name) ?? "");

export class TransferBlobsState {
  #env;
  #nowMs;
  #quota;
  constructor(ctx, env, { nowMs }) {
    this.#env = env;
    this.#nowMs = nowMs;
    this.#quota = new DeviceBlobQuota(ctx);
  }

  #now() {
    const value = this.#nowMs();
    if (!Number.isSafeInteger(value) || value < 0) throw unavailable();
    return value;
  }

  async #put(request, id, chunk) {
    await acquire(request.signal);
    try {
      const body = await boundedStream(request.body, OUTER_BODY, () =>
        fail(413, "Failed to buffer the request body: length limit exceeded"),
      );
      const digest = new BlobDigest();
      let consumed = false;
      let sha;
      try {
        for (const bytes of body.chunks)
          for (let offset = 0; offset < bytes.byteLength; offset += 65536)
            digest.update(bytes.subarray(offset, offset + 65536));
        consumed = true;
        sha = digest.finishHash();
      } finally {
        if (!consumed) digest.free();
      }
      const now = this.#now();
      const prepared = policy(
        transferPutPolicy,
        id,
        chunk,
        String(body.length),
        sha,
        ...headers(request),
        String(now),
      );
      this.#quota.charge(prepared.device, now, () => prepared.cost_kib);
      let object;
      try {
        // Auth, actual-body signature and quota also run for duplicates.
        object = await storageCall(() =>
          this.#env.MEDIA_BUCKET.put(prepared.key, new Blob(body.chunks), {
            onlyIf: new Headers({ "If-None-Match": "*" }),
            sha256: sha,
            httpMetadata: { contentType: "application/octet-stream" },
          }),
        );
      } catch {
        throw unavailable();
      }
      return response(null, object === null ? 200 : 201);
    } finally {
      release();
    }
  }

  async #get(request, id, chunk) {
    const { key } = policy(transferGetPolicy, id, chunk);
    await acquire(request.signal);
    let handedOff = false;
    try {
      let object;
      try {
        object = await storageCall(() => this.#env.MEDIA_BUCKET.get(key));
      } catch {
        throw unavailable();
      }
      if (!object) throw fail(404, "missing");
      let body;
      try {
        body = await boundedStream(object.body, MAX_BODY, unavailable);
      } catch {
        throw unavailable();
      }
      const metadata = { "Content-Type": "application/octet-stream", "Cache-Control": "no-store" };
      if (request.method === "HEAD") return response(null, 200, metadata);
      const result = response(heldBodyStream(body), 200, metadata);
      handedOff = true;
      return result;
    } finally {
      if (!handedOff) release();
    }
  }

  async #delete(request, id) {
    const [device, ts, sig] = headers(request);
    const { prefix } = policy(transferDeletePolicy, id, device, ts, sig, String(this.#now()));
    this.#quota.active(device);
    // Collect before deleting so pagination cannot skip keys during mutation.
    // The native contract admits at most 4096 canonical chunk keys per transfer.
    await acquire(request.signal);
    try {
      const keys = [];
      let cursor;
      do {
        let page;
        try {
          page = await storageCall(() =>
            this.#env.MEDIA_BUCKET.list({ prefix, cursor, limit: 1000 }),
          );
        } catch {
          throw unavailable();
        }
        for (const object of page.objects) {
          if (!object.key.startsWith(prefix) || keys.length >= MAX_CHUNKS) throw unavailable();
          keys.push(object.key);
        }
        if (page.truncated && (!page.cursor || page.cursor === cursor || page.objects.length === 0))
          throw unavailable();
        cursor = page.truncated ? page.cursor : undefined;
      } while (cursor);
      for (let offset = 0; offset < keys.length; offset += 1000) {
        try {
          await storageCall(() => this.#env.MEDIA_BUCKET.delete(keys.slice(offset, offset + 1000)));
        } catch {
          throw unavailable();
        }
      }
      return response(null, 204);
    } finally {
      release();
    }
  }

  async fetch(request) {
    if (!(request instanceof Request)) return new Response("Not found", { status: 404 });
    const match = /^\/v1\/transfer\/([^/]+)(?:\/([^/]+))?$/.exec(new URL(request.url).pathname);
    if (!match) return new Response("Not found", { status: 404 });
    try {
      if (request.method === "OPTIONS") return response(null, 200, {}, true);
      const id = decode(match[1], "id");
      if (match[2] === undefined) {
        if (request.method === "DELETE") return await this.#delete(request, id);
        return response(null, 405, { Allow: "DELETE" });
      }
      const chunk = chunkIndex(match[2]);
      if (request.method === "PUT") return await this.#put(request, id, chunk);
      if (request.method === "GET" || request.method === "HEAD")
        return await this.#get(request, id, chunk);
      return response(null, 405, { Allow: "GET,HEAD,PUT" });
    } catch (cause) {
      if (request.body && !request.bodyUsed) void request.body.cancel().catch(() => {});
      const known = Number.isInteger(cause?.status) && typeof cause.body === "string";
      return response(
        known ? cause.body : "storage unavailable",
        known ? cause.status : 503,
        cause?.retryAfter ? { "Retry-After": cause.retryAfter } : {},
      );
    }
  }
}
