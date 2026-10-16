import test from 'node:test';
import assert from 'node:assert/strict';
import { Pairing, type PairingState } from './pairing.ts';
import { parsePending, type StoredSession } from './session.ts';

const pending = () => parsePending(JSON.stringify({ device: '11'.repeat(32), relay: 'https://relay.example',
  hpke_secret: '22'.repeat(32), secret: '33'.repeat(32), code: 'fixture-code', check: '123456', box_id: '44'.repeat(32),
  expires_at_ms: Date.now() + 10_000 }));
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };

function fixture(request: () => Promise<unknown>) {
  const states: PairingState[] = [];
  const saved: StoredSession[] = [];
  let ready = async () => {};
  let stopped = 0;
  let created = 0;
  const pairing = new Pairing({ device: async () => '11'.repeat(32), sign: async () => new Uint8Array(64),
    save: async (session) => { saved.push(session); } },
  { bytes: async () => new Uint8Array(), text: async () => { created++; return JSON.stringify(pending()); } },
  (state) => states.push(state), (state, onReady) => {
    ready = onReady;
    return { start: () => { state({ kind: 'ready' }); }, stop: () => { stopped++; }, request };
  });
  return { pairing, states, saved, ready: () => ready(), stopped: () => stopped, created: () => created };
}

test('first ready request rejection retries without another ready event', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  let requests = 0;
  const f = fixture(async () => { requests++; if (requests === 1) throw new Error('offline'); return { kind: 'link', sealed: null }; });
  await f.pairing.start('https://relay.example', { kind: 'pending', pending: pending() });
  await f.ready();
  assert.equal(requests, 1);
  t.mock.timers.tick(700); await flush();
  assert.equal(requests, 2);
  assert.equal(f.states.some((s) => s.kind === 'failed'), false);
  f.pairing.stop();
});

test('expiry stops transport during a hung request and ignores its late response', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  let resolve!: (value: unknown) => void;
  const f = fixture(() => new Promise((done) => { resolve = done; }));
  await f.pairing.start('https://relay.example', { kind: 'pending', pending: pending() });
  const fetch = f.ready();
  t.mock.timers.tick(10_000); await flush();
  assert.equal(f.states.at(-1)?.kind, 'failed');
  assert.equal(f.stopped(), 1);
  resolve({ kind: 'link', sealed: [1, 2] }); await fetch;
  assert.equal(f.saved.filter((s) => s.kind === 'linked').length, 0);
});

test('cancellation clears retries and discards late responses', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  let requests = 0;
  const f = fixture(async () => { requests++; return { kind: 'link', sealed: null }; });
  await f.pairing.start('https://relay.example', { kind: 'pending', pending: pending() });
  await f.ready(); f.pairing.stop();
  const states = f.states.length;
  t.mock.timers.tick(60_000); await flush();
  assert.equal(requests, 1);
  assert.equal(f.states.length, states);
});

test('a persisted deadline is retained on reload; legacy pending codes are replaced', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  const stored = pending();
  t.mock.timers.tick(9_500);
  const f = fixture(async () => ({ kind: 'link', sealed: null }));
  await f.pairing.start(stored.relay, { kind: 'pending', pending: stored });
  assert.equal(f.created(), 0);
  t.mock.timers.tick(500); await flush();
  assert.equal(f.states.at(-1)?.kind, 'failed');
  const legacy = fixture(async () => ({ kind: 'link', sealed: null }));
  await legacy.pairing.start(stored.relay, { kind: 'pending', pending: { ...stored, expires_at_ms: 0 } });
  assert.equal(legacy.created(), 1);
  const saved = legacy.saved[0];
  assert.ok(saved?.kind === 'pending' && saved.pending.expires_at_ms > Date.now());
  legacy.pairing.stop();
});
