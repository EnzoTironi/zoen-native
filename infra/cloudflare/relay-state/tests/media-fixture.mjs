// Local-only R2/clock/failure bridge. Never imported by production Wrangler.
// Enrollment and Unlink reuse the existing canonical Directory fixture methods.
import { DirectoryState as ExistingDirectory } from "./directory-fixture.mjs";
import PrivateDirectory from "../src/directory-index.mjs";

const bytes = (value) => Uint8Array.from(atob(value), (char) => char.charCodeAt(0));
const hex = (value) =>
  Array.from(new Uint8Array(value), (byte) => byte.toString(16).padStart(2, "0")).join("");

export class DirectoryState extends ExistingDirectory {
  #bucket;
  #control;
  #nativeControl;

  constructor(ctx, env) {
    const control = {
      failPut: false,
      failGet: false,
      readError: false,
      hold: false,
      entered: 0,
      reads: 0,
      held: [],
    };
    const bucket = env.MEDIA_BUCKET;
    super(ctx, {
      ...env,
      MEDIA_BUCKET: {
        async put(...args) {
          control.entered++;
          if (control.hold) await new Promise((resolve) => control.held.push(resolve));
          if (control.failPut) throw new Error("local-only R2 failure");
          return bucket.put(...args);
        },
        async get(...args) {
          if (control.failGet) throw new Error("local-only R2 failure");
          const object = await bucket.get(...args);
          if (object && control.readError)
            return {
              body: new ReadableStream({
                start(controller) {
                  controller.enqueue(new Uint8Array([1]));
                  controller.error(new Error("local-only late R2 read failure"));
                },
              }),
            };
          return object;
        },
      },
    });
    this.#bucket = bucket;
    this.#control = control;
  }

  media(request) {
    if (!request.body) return super.media(request);
    const reader = request.body.getReader();
    const control = this.#control;
    // No automatic initial pull: observes the actual adapter read budget rather
    // than the transport's own request-stream prefetching.
    const body = new ReadableStream(
      {
        async pull(controller) {
          control.reads++;
          const next = await reader.read();
          if (next.done) {
            reader.releaseLock();
            controller.close();
          } else controller.enqueue(next.value);
        },
        async cancel(reason) {
          await reader.cancel(reason);
          reader.releaseLock();
        },
      },
      { highWaterMark: 0 },
    );
    return super.media(new Request(request, { method: request.method, body }));
  }

  async seed(input) {
    this.#nativeControl = input.control;
    const sql = this.ctx.storage.sql;
    this.ctx.storage.transactionSync(() => {
      sql.exec(
        "INSERT INTO directory_identities VALUES (?,?,?)",
        input.identity,
        input.profile.handle,
        JSON.stringify(input.profile),
      );
      sql.exec(
        "INSERT INTO directory_devices VALUES (?,?,?,0)",
        input.device,
        input.identity,
        input.certificate,
      );
    });
    const start = this.start({ ...input.control, now_ms: input.now_ms });
    this.finish({ ...input.control, ...start, actor: "media-owner" });
    // Initialize only the media schema, never an object or public route.
    await super.media(new Request("https://private.fixture/schema-only"));
    return this.inspect();
  }

  unlink(input) {
    return this.step({
      op: "request",
      actor: "media-owner",
      now_ms: input.now_ms,
      frame_base64: this.#nativeControl.unlink_frame_base64,
    });
  }

  configure(input) {
    if (input.now_ms !== undefined) this.clock(input);
    for (const key of ["failPut", "failGet", "readError", "hold"])
      if (input[key] !== undefined) this.#control[key] = Boolean(input[key]);
    if (input.release) {
      this.#control.hold = false;
      for (const release of this.#control.held.splice(0)) release();
    }
    return {
      entered: this.#control.entered,
      reads: this.#control.reads,
      held: this.#control.held.length,
    };
  }

  quota(input) {
    this.ctx.storage.sql.exec(
      `INSERT INTO directory_blob_limits VALUES (?,?,?,?)
      ON CONFLICT(device) DO UPDATE SET tat_ns=excluded.tat_ns,last_ns=excluded.last_ns,expires_ms=excluded.expires_ms`,
      input.device,
      input.tat_ns,
      input.last_ns,
      input.expires_ms,
    );
  }

  inspectQuota() {
    return this.ctx.storage.sql
      .exec(
        "SELECT device,tat_ns,last_ns,CAST(expires_ms AS TEXT) AS expires_ms FROM directory_blob_limits ORDER BY device",
      )
      .toArray();
  }

  async objects() {
    const listed = await this.#bucket.list({ prefix: "blobs/" });
    if (listed.truncated) throw new Error("Local fixture exceeded its bounded inventory");
    return Promise.all(
      listed.objects.map(async (object) => {
        const head = await this.#bucket.head(object.key);
        return {
          key: object.key,
          sha256: head.checksums.sha256 ? hex(head.checksums.sha256) : null,
          length: String(head.size),
          metadata: head.customMetadata,
        };
      }),
    );
  }

  async reconstruct(input) {
    // A fresh production host on the same SQLite/R2 state. No forced hibernation.
    const { MediaBlobsState } = await import("../src/media-blobs.mjs");
    const host = new MediaBlobsState(
      this.ctx,
      { MEDIA_BUCKET: this.#bucket, RELAY_NAME: this.env.RELAY_NAME },
      {
        nowMs: () => Number(input.now_ms),
      },
    );
    return host.fetch(
      new Request(`https://private.fixture/v1/blobs/${input.sha}`, {
        method: "PUT",
        headers: input.headers,
        body: bytes(input.body_base64),
      }),
    );
  }

  async cancelQueued(input) {
    const controller = new AbortController();
    const pending = super.media(
      new Request(`https://private.fixture/v1/blobs/${input.sha}`, {
        method: "PUT",
        headers: input.headers,
        body: bytes(input.body_base64),
        signal: controller.signal,
      }),
    );
    controller.abort();
    return pending;
  }

  async abandonedGet(input) {
    const response = await super.media(
      new Request(`https://private.fixture/v1/blobs/${input.sha}`),
    );
    // Keep the actual production stream inside this DO without consuming it.
    // After its private idle deadline, its retained lease must become available.
    this.#control.abandoned = response;
    return { status: response.status };
  }
}

export default class MediaFixture extends PrivateDirectory {
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/facade/")) {
      url.pathname = url.pathname.slice("/facade".length);
      return super.media(new Request(url, request));
    }
    if (url.pathname.startsWith("/public/")) return super.fetch(request);
    const stub = this.env.DIRECTORY.getByName(
      url.searchParams.get("name") ?? "private-media-fixture",
    );
    if (url.pathname.startsWith("/v1/blobs/")) return stub.media(request);
    const input = request.method === "POST" ? await request.json() : {};
    switch (url.pathname) {
      case "/seed":
        return Response.json(await stub.seed(input));
      case "/configure":
        return Response.json(await stub.configure(input));
      case "/unlink":
        return Response.json(await stub.unlink(input));
      case "/objects":
        return Response.json(await stub.objects());
      case "/quota":
        await stub.quota(input);
        return Response.json({ ok: true });
      case "/inspect-quota":
        return Response.json(await stub.inspectQuota());
      case "/reconstruct":
        return stub.reconstruct(input);
      case "/cancel-queued":
        return stub.cancelQueued(input);
      case "/abandoned-get":
        return Response.json(await stub.abandonedGet(input));
      default:
        return new Response("Not found", { status: 404 });
    }
  }
}
