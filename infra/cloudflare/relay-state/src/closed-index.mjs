import { WorkerEntrypoint } from 'cloudflare:workers';
import { ClosedSpace, verifiedPublication } from './closed-space.mjs';
import { spaceId } from './space-storage.mjs';

export { ClosedSpace };

/** Only a trusted service binding may supply authenticated sessions and directory facts. */
export default class ClosedRelay extends WorkerEntrypoint {
  publish(input) {
    const { verified } = verifiedPublication(input);
    return this.env.CLOSED_SPACE.getByName(verified.space_id).publish(input);
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

  fetch() { return new Response('Not found', { status: 404 }); }
}
