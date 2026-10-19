import { DurableObject, WorkerEntrypoint } from 'cloudflare:workers';
import { SpaceStorage, spaceId } from './space-storage.mjs';

/** Storage only; trusted callers supply events already admitted by shared Rust. */
export class SpaceLog extends DurableObject {
  #store;

  constructor(ctx, env) {
    super(ctx, env);
    this.#store = new SpaceStorage(ctx.storage.sql);
  }

  appendAdmitted(input) {
    return this.ctx.storage.transactionSync(() => this.#store.appendAdmitted(input));
  }

  snapshot(input) {
    return this.ctx.storage.transactionSync(() => this.#store.snapshot(input?.spaceId));
  }

  read(input) {
    return this.ctx.storage.transactionSync(() => this.#store.read(input));
  }
}

/** Only trusted service bindings can invoke RPC. No public sync/auth facade. */
export default class RelayState extends WorkerEntrypoint {
  appendAdmitted(input) {
    spaceId(input?.spaceId);
    return this.env.SPACE_LOG.getByName(input.spaceId).appendAdmitted(input);
  }

  read(input) {
    spaceId(input?.spaceId);
    return this.env.SPACE_LOG.getByName(input.spaceId).read(input);
  }

  snapshot(input) {
    spaceId(input?.spaceId);
    return this.env.SPACE_LOG.getByName(input.spaceId).snapshot(input);
  }

  fetch() {
    return new Response('Not found', { status: 404 });
  }
}
