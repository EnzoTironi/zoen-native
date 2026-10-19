// Private media transport. Rust owns signatures, digest, limits and object keys;
// this host owns bounded request I/O, current Directory rows and the R2 binding.
import {
  blobPutPolicy,
  blobBodyPolicy,
  blobGetPolicy,
  blobRateConfig,
  blobRateDecision,
  BlobDigest,
} from "../generated/wasm/admission.js";

const MAX_BODY = 25 * 1024 * 1024;
const OUTER_BODY = MAX_BODY + 1024;
const HASH_CHUNK = 64 * 1024;
const MAX_WAITING = 8;
const GC_BATCH = 256;
const IO_IDLE_MS = 10_000;
// The 128 MiB limit belongs to the isolate, including every Directory instance.
// Keep one shared body reader even when local fixtures create several cells.
let bodyInUse = false;
const waiting = [];
const fail = (status, body) => Object.assign(new Error(body), { status, body });
const unavailable = () => fail(503, "storage unavailable");

function policy(fn, ...args) {
  try {
    return JSON.parse(fn(...args));
  } catch (cause) {
    let detail;
    try {
      detail = JSON.parse(typeof cause === "string" ? cause : cause.message);
    } catch {}
    if (Number.isInteger(detail?.status) && typeof detail.body === "string")
      throw fail(detail.status, detail.body);
    throw unavailable();
  }
}

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

async function boundedStream(stream, maximum, tooLarge) {
  const chunks = [];
  const reader = stream?.getReader();
  let length = 0;
  if (!reader) return { chunks, length };
  try {
    for (;;) {
      let timer;
      let next;
      try {
        next = await Promise.race([
          reader.read(),
          new Promise((_, reject) => {
            timer = setTimeout(() => reject(unavailable()), IO_IDLE_MS);
          }),
        ]);
      } finally {
        clearTimeout(timer);
      }
      const { done, value } = next;
      if (done) return { chunks, length };
      length += value.byteLength;
      if (length > maximum) throw tooLarge();
      chunks.push(value);
    }
  } catch (cause) {
    // A hostile source's cancel callback must not keep the shared lease forever.
    void reader.cancel().catch(() => {});
    throw cause;
  } finally {
    reader.releaseLock();
  }
}

async function acquire(signal) {
  if (signal?.aborted) throw unavailable();
  if (!bodyInUse) {
    bodyInUse = true;
    return;
  }
  if (waiting.length >= MAX_WAITING) throw unavailable();
  await new Promise((resolve, reject) => {
    let timer;
    const remove = () => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
    };
    const accept = () => {
      remove();
      resolve();
    };
    const abort = () => {
      const index = waiting.indexOf(accept);
      if (index < 0) return;
      waiting.splice(index, 1);
      remove();
      reject(unavailable());
    };
    waiting.push(accept);
    timer = setTimeout(abort, IO_IDLE_MS);
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) abort();
  });
}

function release() {
  const next = waiting.shift();
  if (next) next();
  else bodyInUse = false;
}

/** Same fixed Directory transaction domain; no public entrypoint or new account DB.
 * Only ONE media body is read at once. Up to eight pending requests retain an
 * unread stream, not another body buffer. This is an explicit private capacity
 * bound. No 25 MiB value is copied into WASM or JSON; digest updates are <=64 KiB.
 */
export class MediaBlobsState {
  #ctx;
  #env;
  #nowMs;

  constructor(ctx, env, { nowMs }) {
    this.#ctx = ctx;
    this.#env = env;
    this.#nowMs = nowMs;
    ctx.storage.sql.exec(`CREATE TABLE IF NOT EXISTS directory_blob_limits (
      device TEXT PRIMARY KEY, tat_ns TEXT NOT NULL, last_ns TEXT NOT NULL,
      expires_ms INTEGER NOT NULL)`);
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
    return this.#ctx.storage.transactionSync(() => {
      const sql = this.#ctx.storage.sql;
      let active;
      try {
        active = sql
          .exec(
            "SELECT device FROM directory_devices WHERE device=? AND revoked=0",
            prepared.device,
          )
          .toArray()[0];
      } catch {
        throw fail(503, "database unavailable");
      }
      if (!active) throw fail(403, "unlinked device");
      const body = policy(blobBodyPolicy, JSON.stringify({ length: String(length) }));
      const previous = sql
        .exec("SELECT tat_ns,last_ns FROM directory_blob_limits WHERE device=?", prepared.device)
        .toArray()[0];
      // Native uses process-monotonic Instant. Across DO reconstruction we retain
      // the last admitted wall-clock basis and never grant time on a backward step.
      const wall = BigInt(now) * 1_000_000n;
      const clock = previous && BigInt(previous.last_ns) > wall ? BigInt(previous.last_ns) : wall;
      const quota = policy(blobRateConfig);
      const rate = policy(
        blobRateDecision,
        JSON.stringify({
          now_ns: String(clock),
          tat_ns: previous?.tat_ns ?? null,
          cost: body.cost_kib,
          interval_ns: quota.interval_ns,
          burst: quota.burst,
        }),
      );
      if (!rate.accepted)
        throw Object.assign(fail(429, rate.body), { retryAfter: rate.retry_after });
      // Expired theoretical arrivals are equivalent to missing native map entries.
      // Physical maintenance is bounded; no cleanup on a refused admission.
      sql.exec(
        "DELETE FROM directory_blob_limits WHERE device IN (SELECT device FROM directory_blob_limits WHERE expires_ms <= ? LIMIT ?)",
        now,
        GC_BATCH,
      );
      const expires = Number((BigInt(rate.next_ns) + 999_999n) / 1_000_000n);
      if (!Number.isSafeInteger(expires)) throw unavailable();
      sql.exec(
        `INSERT INTO directory_blob_limits VALUES (?,?,?,?)
        ON CONFLICT(device) DO UPDATE SET tat_ns=excluded.tat_ns,last_ns=excluded.last_ns,expires_ms=excluded.expires_ms`,
        prepared.device,
        rate.next_ns,
        String(clock),
        expires,
      );
      return prepared;
    });
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
        object = await this.#env.MEDIA_BUCKET.put(prepared.key, new Blob(body.chunks), {
          onlyIf: new Headers({ "If-None-Match": "*" }),
          sha256: sha,
          httpMetadata: { contentType: "application/octet-stream" },
        });
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
        object = await this.#env.MEDIA_BUCKET.get(key);
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
      let index = 0;
      let offset = 0;
      let released = false;
      let timer;
      let streamController;
      const complete = () => {
        if (released) return;
        released = true;
        clearTimeout(timer);
        body.chunks.length = 0;
        release();
      };
      const arm = () => {
        clearTimeout(timer);
        timer = setTimeout(() => {
          try {
            streamController.error(new Error("private media read idle timeout"));
          } catch {
          } finally {
            complete();
          }
        }, IO_IDLE_MS);
      };
      // Keep the isolate lease until these bytes are consumed/cancelled. No
      // second Blob/ArrayBuffer copy is needed for the outgoing response.
      const stream = new ReadableStream(
        {
          start(controller) {
            streamController = controller;
            arm();
          },
          pull(controller) {
            arm();
            const chunk = body.chunks[index];
            if (!chunk) {
              controller.close();
              complete();
              return;
            }
            const end = Math.min(offset + HASH_CHUNK, chunk.byteLength);
            controller.enqueue(chunk.subarray(offset, end));
            offset = end;
            if (offset === chunk.byteLength) {
              index++;
              offset = 0;
            }
          },
          cancel() {
            complete();
          },
        },
        { highWaterMark: 0 },
      );
      // Only fixed media headers leave the bucket, never uploader metadata.
      const result = response(stream, 200, headers);
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
