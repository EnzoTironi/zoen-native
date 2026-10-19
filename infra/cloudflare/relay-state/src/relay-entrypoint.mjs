import { WorkerEntrypoint } from "cloudflare:workers";
import { relayRequestRefusal, relayUnavailable } from "./relay-route.mjs";

// Named service-only fetch entrypoint. The Directory Worker's default HTTP
// handler remains 404 and its existing private RPC interface stays private.
export class RelayIngress extends WorkerEntrypoint {
  async fetch(request) {
    const refusal = relayRequestRefusal(request, this.env);
    if (refusal) return refusal;
    try {
      return await this.env.DIRECTORY.getByName(this.env.DIRECTORY_CELL).fetch(request);
    } catch {
      return relayUnavailable();
    }
  }
}

// Fixed server callback for the Space's durable reference outbox. It is never
// exposed by the ingress router or the Directory Worker's default HTTP handler.
export class RelayDispatch extends WorkerEntrypoint {
  relayDispatch(input) {
    return this.env.DIRECTORY.getByName(this.env.DIRECTORY_CELL).relayDispatch(input);
  }
  fetch() {
    return new Response("Not found", { status: 404 });
  }
}
