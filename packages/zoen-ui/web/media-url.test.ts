import test from 'node:test';
import assert from 'node:assert/strict';
import { acquireUrl } from './media-url.ts';

test('decrypted URLs are revoked on release, including late opens', async () => {
  const original = URL.revokeObjectURL;
  const revoked: string[] = [];
  URL.revokeObjectURL = (url) => { revoked.push(url); original(url); };
  try {
    const active = acquireUrl(async () => new Blob(['secret']), () => 'application/octet-stream');
    const url = await active.url;
    assert.deepEqual(revoked, []);
    active.release(); active.release();
    assert.deepEqual(revoked, [url]);
    let finish!: (blob: Blob) => void;
    const late = acquireUrl(() => new Promise((resolve) => { finish = resolve; }), () => 'application/octet-stream');
    late.release(); finish(new Blob(['late secret']));
    assert.deepEqual(revoked, [url, await late.url]);
  } finally { URL.revokeObjectURL = original; }
});
