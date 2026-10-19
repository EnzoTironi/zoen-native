import assert from "node:assert/strict";
import test from "node:test";
import { boundedStream, fail, acquire, release, storageCall } from "../src/blob-io.mjs";

test("tiny and empty fragments retain a bounded number of owned ciphertext buffers", async () => {
  let index = 0;
  const length = 150_003;
  const body = new ReadableStream(
    {
      pull(controller) {
        if (index === length) return controller.close();
        if (index % 7 === 0) controller.enqueue(new Uint8Array());
        controller.enqueue(new Uint8Array([index++ % 251]));
      },
    },
    { highWaterMark: 0 },
  );
  const actual = await boundedStream(body, length, () => fail(413, "too large"));
  assert.equal(actual.length, length);
  assert.equal(actual.chunks.length, Math.ceil(length / 65536));
  const bytes = Buffer.concat(actual.chunks);
  for (let i = 0; i < length; i++) assert.equal(bytes[i], i % 251);
  assert.ok(actual.chunks.every((chunk) => chunk.buffer.byteLength === 65536));
});

test("oversized bodies are cancelled and preserve the original refusal", async () => {
  let cancelled = false;
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(new Uint8Array(1025));
    },
    cancel() {
      cancelled = true;
    },
  });
  await assert.rejects(
    boundedStream(stream, 1024, () => fail(413, "too large")),
    (error) => error.status === 413 && error.body === "too large",
  );
  assert.equal(cancelled, true);
});

test("an uncancellable storage timeout refuses new buffers until the original operation settles", async () => {
  await acquire();
  let finish;
  const pending = storageCall(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const queued = acquire().then(
    () => "admitted",
    () => "refused",
  );
  await assert.rejects(pending, (error) => error.status === 503);
  release();
  assert.equal(await queued, "refused");
  await assert.rejects(acquire(), (error) => error.status === 503);
  finish(null);
  await new Promise((resolve) => setImmediate(resolve));
  await acquire();
  release();
});
