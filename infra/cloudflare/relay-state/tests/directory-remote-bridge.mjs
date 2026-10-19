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
    if (env.QA_MEDIA === "1") {
      const match = /^\/media\/([a-f0-9]{64})$/.exec(path);
      if (
        match &&
        [env.QA_MEDIA_ONE, env.QA_MEDIA_TWO].includes(match[1]) &&
        ["GET", "HEAD", "PUT"].includes(request.method) &&
        (request.method !== "PUT" || request.headers.get("x-zoen-device") === env.QA_DEVICE)
      ) {
        // One hardcoded OWN cell and exactly two native-generated hashes. No
        // arbitrary bucket key, cell, identity or verified-device injection.
        return env.DIRECTORY.getByName(env.DIRECTORY_CELL).media(
          new Request(`https://private-directory.test/v1/blobs/${match[1]}`, request),
        );
      }
    }
    if (
      env.QA_MEDIA !== "1" &&
      path === "/deliver" &&
      request.method === "POST" &&
      (await emptyBody(request))
    ) {
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
