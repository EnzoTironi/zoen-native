// Shared edge/service gate. Authentication remains the binary device handshake
// owned by Directory; no HTTP parameter can supply a session or target shard.
function refusal(status) {
  return new Response(status === 404 ? "Not found" : "Relay unavailable", {
    status,
    headers: { "Cache-Control": "no-store" },
  });
}

export function relayRequestRefusal(request, env) {
  const url = new URL(request.url);
  if (
    request.method !== "GET" ||
    url.pathname !== "/v1/sync" ||
    url.search !== "" ||
    url.hash !== "" ||
    request.headers.get("Upgrade")?.toLowerCase() !== "websocket"
  )
    return refusal(404);
  // Activation follows complete discovery and actual wire/fault qualification.
  if (env.RELAY_PROTOCOL_ENABLED !== "true") return refusal(503);
  const configured = env.RELAY_ALLOWED_ORIGINS;
  let origins;
  try {
    if (typeof configured !== "string" || configured.length > 2048) return refusal(503);
    origins = JSON.parse(configured);
    if (!Array.isArray(origins) || origins.length < 1 || origins.length > 8) return refusal(503);
    const unique = new Set();
    for (const value of origins) {
      if (typeof value !== "string") return refusal(503);
      const origin = new URL(value);
      if (origin.protocol !== "https:" || origin.origin !== value || unique.has(value))
        return refusal(503);
      unique.add(value);
    }
  } catch {
    return refusal(503);
  }
  const origin = request.headers.get("Origin");
  if (origin !== null && !origins.includes(origin)) return refusal(403);
  // Native clients have no Origin and must complete the same certified-device
  // challenge. Origin is browser request protection, never an identity proof.
  return null;
}

export function relayUnavailable() {
  return refusal(503);
}
