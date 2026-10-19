import { WorkerEntrypoint } from "cloudflare:workers";
import { ClosedSpace, verifiedPublication } from "./closed-space.mjs";
import { spaceId } from "./space-storage.mjs";

export { ClosedSpace };

/** Only a trusted service binding may supply authenticated sessions and directory facts. */
export default class ClosedRelay extends WorkerEntrypoint {
  prepareRelayPublication(input) {
    const { verified } = verifiedPublication(input);
    return this.env.CLOSED_SPACE.getByName(verified.space_id).prepareRelayPublication(input);
  }

  commitRelayPublication(input) {
    const { verified } = verifiedPublication(input);
    return this.env.CLOSED_SPACE.getByName(verified.space_id).commitRelayPublication(input);
  }

  createInvite(input) {
    spaceId(input?.spaceId);
    return this.env.CLOSED_SPACE.getByName(input.spaceId).createInvite(input);
  }

  snapshot(input) {
    spaceId(input?.spaceId);
    return this.env.CLOSED_SPACE.getByName(input.spaceId).snapshot(input);
  }

  read(input) {
    spaceId(input?.spaceId);
    return this.env.CLOSED_SPACE.getByName(input.spaceId).read(input);
  }

  visibility(input) {
    spaceId(input?.spaceId);
    return this.env.CLOSED_SPACE.getByName(input.spaceId).visibility(input);
  }

  coMembers(input) {
    spaceId(input?.spaceId);
    return this.env.CLOSED_SPACE.getByName(input.spaceId).coMembers(input);
  }

  readAuthorized(input) {
    spaceId(input?.spaceId);
    return this.env.CLOSED_SPACE.getByName(input.spaceId).readAuthorized(input);
  }

  fetch() {
    return new Response("Not found", { status: 404 });
  }
}
