// One bounded ciphertext buffer across all media and transfer hosts in an isolate.
const HASH_CHUNK = 64 * 1024;
const MAX_WAITING = 8;
const IO_IDLE_MS = 10_000;
// The 128 MiB limit belongs to the isolate, including every Directory instance.
// Keep one shared body reader even when local fixtures create several cells.
let bodyInUse = false;
let pendingStorage = 0;
const waiting = [];
export const fail = (status, body) => Object.assign(new Error(body), { status, body });
export const unavailable = () => fail(503, "storage unavailable");

export function policy(fn, ...args) {
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

export async function boundedStream(stream, maximum, tooLarge) {
  const chunks = [];
  const reader = stream?.getReader();
  let length = 0;
  let buffer;
  let used = 0;
  let progress = performance.now();
  if (!reader) return { chunks, length };
  try {
    for (;;) {
      const remaining = IO_IDLE_MS - (performance.now() - progress);
      if (remaining <= 0) throw unavailable();
      let timer;
      let next;
      try {
        next = await Promise.race([
          reader.read(),
          new Promise((_, reject) => {
            timer = setTimeout(() => reject(unavailable()), remaining);
          }),
        ]);
      } finally {
        clearTimeout(timer);
      }
      const { done, value } = next;
      if (done) {
        if (used) chunks.push(buffer.subarray(0, used));
        return { chunks, length };
      }
      // Empty fragments neither allocate nor reset the progress deadline.
      if (value.byteLength === 0) continue;
      progress = performance.now();
      length += value.byteLength;
      if (length > maximum) throw tooLarge();
      // Input chunk count and backing allocations are untrusted. Copy into
      // <=ceil(maximum/64KiB) owned buffers rather than retaining tiny slices.
      for (let offset = 0; offset < value.byteLength;) {
        buffer ??= new Uint8Array(HASH_CHUNK);
        const count = Math.min(HASH_CHUNK - used, value.byteLength - offset);
        buffer.set(value.subarray(offset, offset + count), used);
        used += count;
        offset += count;
        if (used === HASH_CHUNK) {
          chunks.push(buffer);
          buffer = undefined;
          used = 0;
        }
      }
    }
  } catch (cause) {
    void reader.cancel().catch(() => {});
    throw cause;
  } finally {
    reader.releaseLock();
  }
}

// R2 calls cannot be cancelled. A timed-out call still fences new body readers
// until it settles, so a timeout never releases unlimited retained upload bodies.
export async function storageCall(start) {
  pendingStorage++;
  let timedOut = false;
  let timer;
  const operation = Promise.resolve().then(start);
  operation.then(
    (value) => {
      pendingStorage--;
      if (timedOut && value?.body) void value.body.cancel().catch(() => {});
    },
    () => {
      pendingStorage--;
    },
  );
  try {
    return await Promise.race([
      operation,
      new Promise((_, reject) => {
        timer = setTimeout(() => {
          timedOut = true;
          reject(unavailable());
        }, 15_000);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

export async function acquire(signal) {
  if (signal?.aborted) throw unavailable();
  if (!bodyInUse) {
    if (pendingStorage) throw unavailable();
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
    const accept = (admitted = true) => {
      remove();
      if (admitted) resolve();
      else reject(unavailable());
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

export function release() {
  if (pendingStorage) {
    bodyInUse = false;
    for (const next of waiting.splice(0)) next(false);
    return;
  }
  const next = waiting.shift();
  if (next) next();
  else bodyInUse = false;
}

// Caller transfers its lease only after creating the response successfully.
export function heldBodyStream(body) {
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
  return stream;
}
