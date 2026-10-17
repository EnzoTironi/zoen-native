import assert from 'node:assert/strict';
import test from 'node:test';
import { frameAuthority } from './lifetime.ts';

test('a late consent from a covered frame cannot regain authority when its replacement opens', async () => {
  let resolve!: () => void;
  const approved = new Promise<void>((done) => {
    resolve = done;
  });
  const old = frameAuthority(() => {});
  let writes = 0;
  const consent = (async () => {
    old.check();
    await approved;
    old.check();
    writes++;
  })();
  old.close();
  const next = frameAuthority(() => {});
  next.check();
  resolve();
  await assert.rejects(consent, /interface has closed/);
  assert.equal(writes, 0);
  next.check();
});

test('an owner or device switch invalidates an otherwise live frame', () => {
  let owner = 'identity:device-one';
  const initial = owner;
  const authority = frameAuthority(() => {
    if (owner !== initial) throw new Error('account changed');
  });
  owner = 'identity:device-two';
  assert.throws(authority.check, /account changed/);
});
