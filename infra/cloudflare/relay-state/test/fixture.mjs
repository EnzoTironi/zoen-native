// Miniflare-only bridge. Not referenced by the production Wrangler configuration.
import { SpaceLog as ProductionSpaceLog } from '../src/index.mjs';
export class SpaceLog extends ProductionSpaceLog {
  seedHighSequence(input) {
    this.ctx.storage.sql.exec('INSERT INTO space_binding(singleton, space_id) VALUES (1, ?)', input.spaceId);
    this.ctx.storage.sql.exec('INSERT INTO entries(seq_key, author, client_id, prev, hash, bytes) VALUES (?, ?, ?, ?, ?, ?)',
      input.seq.padStart(20, '0'), 'a'.repeat(64), 'seed', '0'.repeat(64), 'f'.repeat(64), new Uint8Array([1]));
    return { seq: input.seq, hash: 'f'.repeat(64) };
  }
}
export default {
  async fetch(request, env) {
    const input = await request.json();
    const stub = env.SPACE_LOG.getByName(input.spaceId);
    if (input.event) input.event.bytes = new Uint8Array(input.event.bytes);
    if (input.admissionState) input.admissionState = new Uint8Array(input.admissionState);
    try {
      const operation = { '/read': 'read', '/snapshot': 'snapshot', '/seed-high': 'seedHighSequence' }[new URL(request.url).pathname] ?? 'appendAdmitted';
      const output = await stub[operation](input);
      return Response.json(output, { headers: { 'cache-control': 'no-store' } });
    } catch (error) {
      return Response.json({ error: error.message }, { status: 400 });
    }
  },
};
