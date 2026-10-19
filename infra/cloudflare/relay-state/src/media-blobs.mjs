// Private media transport. Rust owns signatures, digest, limits and object keys;
// this host owns bounded request I/O, current Directory rows and the R2 binding.
import {
  blobPutPolicy,
  blobBodyPolicy,
  blobGetPolicy,
  BlobDigest,
} from "../generated/wasm/admission.js";

const MAX_BODY = 25 * 1024 * 1024;
const OUTER_BODY = MAX_BODY + 1024;
const HASH_CHUNK = 64 * 1024;
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
function cors(headers, preflight = false) {
  headers.set("Access-Control-Allow-Origin", "*");
  headers.set("Vary", "origin, access-control-request-method, access-control-request-headers");
  if (preflight) {
    headers.set("Access-Control-Allow-Methods", "GET,PUT");
    headers.set("Access-Control-Allow-Headers", "content-type,x-zoen-device,x-zoen-ts,x-zoen-sig");
    headers.set("Access-Control-Max-Age", "86400");
  }
  return headers;
}
function response(body, status, headers = {}) {
  const result = new Headers(headers);
  if (body !== null && !result.has("Content-Type"))
    result.set("Content-Type", "text/plain; charset=utf-8");
  return new Response(body, { status, headers: cors(result) });
}

/** Same fixed Directory transaction domain; no public entrypoint or new account DB.
 * Only ONE media body is read at once. Up to eight pending requests retain an
 * unread stream, not another body buffer. This is an explicit private capacity
 * bound. No 25 MiB value is copied into WASM or JSON; digest updates are <=64 KiB.
 */
export class MediaBlobsState {
  #env;
  #nowMs;
  #quota;

  constructor(ctx, env, { nowMs }) {
    this.#env = env;
    this.#nowMs = nowMs;
    this.#quota = new DeviceBlobQuota(ctx);
  }

  #admit(sha, request, length) {
    const now = this.#nowMs();
    if (!Number.isSafeInteger(now) || now < 0) throw unavailable();
    const prepared = policy(
      blobPutPolicy,
      sha,
      request.headers.get("x-zoen-device") ?? "",
      request.headers.get("x-zoen-ts") ?? "",
      request.headers.get("x-zoen-sig") ?? "",
      this.#env.RELAY_NAME,
      String(now),
    );
    this.#quota.charge(
      prepared.device,
      now,
      () => policy(blobBodyPolicy, JSON.stringify({ length: String(length) })).cost_kib,
    );
    return prepared;
  }

  async #put(request, sha) {
    await acquire(request.signal);
    try {
      const body = await boundedStream(request.body, OUTER_BODY, () =>
        fail(413, "Failed to buffer the request body: length limit exceeded"),
      );
      const prepared = this.#admit(sha, request, body.length);
      // Hash failures and valid duplicates both spend native weighted rate budget.
      const digest = new BlobDigest();
      let consumed = false;
      try {
        for (const chunk of body.chunks)
          for (let offset = 0; offset < chunk.byteLength; offset += HASH_CHUNK)
            digest.update(chunk.subarray(offset, offset + HASH_CHUNK));
        consumed = true;
        try {
          digest.finish(sha);
        } catch (cause) {
          let detail;
          try {
            detail = JSON.parse(typeof cause === "string" ? cause : cause.message);
          } catch {}
          if (detail?.status === 400 && detail.body === "hash mismatch")
            throw fail(400, detail.body);
          throw unavailable();
        }
      } finally {
        if (!consumed) digest.free();
      }
      let object;
      try {
        // Conditional create is atomic. Never HEAD-shortcut auth, quota or digest.
        // No SQL fence is held across R2: native admitted PUT may finish after Unlink.
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

  async #get(sha, head, signal) {
    const { key } = policy(blobGetPolicy, sha);
    await acquire(signal);
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
      // Native buffers the R2 read before returning200: a late stream failure is
      // still503, never a successful response with an incomplete ciphertext.
      const headers = {
        "Content-Type": "application/octet-stream",
        "Cache-Control": "public, max-age=31536000, immutable",
        "Cross-Origin-Resource-Policy": "cross-origin",
      };
      if (head) return response(null, 200, headers);
      // Only fixed headers leave the bucket; hold the isolate body lease until consumed.
      const result = response(heldBodyStream(body), 200, headers);
      handedOff = true;
      return result;
    } finally {
      if (!handedOff) release();
    }
  }

  async fetch(request) {
    if (!(request instanceof Request)) return new Response("Not found", { status: 404 });
    const match = /^\/v1\/blobs\/([^/]+)$/.exec(new URL(request.url).pathname);
    if (!match) return new Response("Not found", { status: 404 });
    try {
      if (request.method === "OPTIONS")
        return new Response(null, { status: 200, headers: cors(new Headers(), true) });
      // Rust's percent decoder leaves malformed percent escapes literal, then
      // validates UTF8; ordinary hash validation remains shared Rust policy.
      let sha;
      try {
        sha = decodeURIComponent(match[1].replace(/%(?![a-fA-F0-9]{2})/g, "%25"));
      } catch {
        throw fail(400, "Invalid URL: Invalid UTF-8 in `sha`");
      }
      if (request.method === "PUT") return await this.#put(request, sha);
      if (request.method === "GET" || request.method === "HEAD")
        return await this.#get(sha, request.method === "HEAD", request.signal);
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
