import { relayRequestRefusal, relayUnavailable } from "./relay-route.mjs";

export default {
  async fetch(request, env) {
    const refusal = relayRequestRefusal(request, env);
    if (refusal) return refusal;
    try {
      // The binding fixes both the destination Worker and its named entrypoint.
      // Forward the original Upgrade and Origin; no public RPC/namespace switch.
      return await env.DIRECTORY_RELAY.fetch(request);
    } catch {
      return relayUnavailable();
    }
  },
};
