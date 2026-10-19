import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { unstable_dev } from 'wrangler';

if (process.env.ZOEN_CF_REMOTE_STATE_QA !== '1') throw new Error('Set ZOEN_CF_REMOTE_STATE_QA=1 to write two small synthetic records to the deployed private QA storage.');
const spaceId = `qa-cloudflare-state-${randomUUID()}`;
const fixture = fileURLToPath(new URL('./remote-fixture.mjs', import.meta.url));
const config = fileURLToPath(new URL('./remote.wrangler.jsonc', import.meta.url));
let worker;
async function open() {
  return unstable_dev(fixture, {
    config,
    ip: '127.0.0.1', port: 0, local: false, logLevel: 'error',
    services: [{ binding: 'RELAY_STATE', service: 'zoen-relay-state', remote: true }],
    experimental: { disableExperimentalWarning: true, disableDevRegistry: true, watch: false, showInteractiveDevSession: false },
  });
}
async function call(path, input) {
  const response = await worker.fetch(`http://qa.test${path}`, { method: 'POST', body: JSON.stringify(input) });
  assert.equal(response.status, 200, await response.clone().text());
  return response.json();
}
const first = {
  spaceId, author: 'a'.repeat(64), clientId: 'first', expectedHead: null,
  event: { seq: '0', prev: '0'.repeat(64), hash: '1'.repeat(64), bytes: [10, 20, 30] },
  admissionState: [1, 2],
};
try {
  worker = await open();
  assert.equal((await call('/append', first)).kind, 'new');
  const next = {
    ...first, clientId: 'next', expectedHead: { seq: '0', hash: first.event.hash },
    event: { seq: '1', prev: first.event.hash, hash: '2'.repeat(64), bytes: [40, 50] },
    admissionState: [3, 4],
  };
  assert.equal((await call('/append', next)).kind, 'new');
  await worker.stop();
  worker = await open();
  assert.equal((await call('/append', { ...first, event: { ...first.event, bytes: [99] } })).kind, 'duplicate');
  const entries = await call('/read', { spaceId, from: '0', limit: 2 });
  assert.deepEqual(entries.map(event => event.seq), ['0', '1']);
  assert.deepEqual(Object.values(entries[0].bytes), [10, 20, 30]);
  const snapshot = await call('/snapshot', { spaceId });
  assert.deepEqual(snapshot.head, { seq: '1', hash: next.event.hash });
  assert.deepEqual(Object.values(snapshot.admissionState), [3, 4]);
  console.log(JSON.stringify({ result: 'PASS', spaceId, records: 2, eventBytes: 5, scope: 'deployed SQLite DO via authenticated remote service binding; second client session replay' }));
} finally { await worker?.stop(); }
