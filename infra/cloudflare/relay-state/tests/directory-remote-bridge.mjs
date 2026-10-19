// Temporary authenticated remote preview only. These vars are generated from ONE
// native synthetic client; request input cannot select a cell, target or frame.
async function emptyBody(request) {
  if (!request.body) return true;
  const reader = request.body.getReader();
  try {
    // Remote preview may represent an empty network POST as an empty stream.
    // Refuse the first actual byte without buffering a caller-supplied payload.
    for (;;) {
      const { done, value } = await reader.read();
      if (done) return true;
      if (value.byteLength) return false;
    }
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
}
export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (
      path === "/socket" &&
      request.method === "GET" &&
      request.headers.get("Upgrade")?.toLowerCase() === "websocket"
    ) {
      return env.DIRECTORY.getByName(env.DIRECTORY_CELL).fetch(
        new Request("https://private-directory.test/v1/sync", request),
      );
    }
    if (path === "/deliver" && request.method === "POST" && (await emptyBody(request))) {
      const frame = Uint8Array.from(atob(env.QA_PONG), (char) => char.charCodeAt(0));
      return Response.json(
        await env.DIRECTORY.getByName(env.DIRECTORY_CELL).deliver({
          identity: env.QA_IDENTITY,
          device: env.QA_DEVICE,
          frame,
        }),
      );
    }
    if (path === "/public" && request.method === "GET") {
      return env.PRIVATE.fetch(new Request("https://private-directory.test/v1/sync"));
    }
    return new Response("Not found", { status: 404 });
  },
};
