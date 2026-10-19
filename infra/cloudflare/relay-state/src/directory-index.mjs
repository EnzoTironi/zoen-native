import { WorkerEntrypoint } from 'cloudflare:workers';
export { DirectoryState } from './directory-state.mjs';

/** Private service binding only; callers cannot choose a new transaction shard. */
export default class PrivateDirectory extends WorkerEntrypoint {
  #directory() { return this.env.DIRECTORY.getByName(this.env.DIRECTORY_CELL); }
  hello(input) { return this.#directory().hello(input); }
  authenticate(input) { return this.#directory().authenticate(input); }
  request(input) { return this.#directory().request(input); }
  authorize(input) { return this.#directory().authorize(input); }
  deliver(input) { return this.#directory().deliver(input); }
  // Request/Response RPC is private; the original Origin header reaches the DO.
  provider(request) { return this.#directory().provider(request); }
  media(request) { return this.#directory().media(request); }
  fetch() { return new Response('Not found', { status: 404 }); }
}
