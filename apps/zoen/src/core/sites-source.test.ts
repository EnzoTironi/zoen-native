import assert from 'node:assert/strict';
import { test } from 'node:test';
import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import { saveObservedSite } from '../sites/actions.ts';
import {
  emptySite,
  sameSource,
  siteAuthority,
  siteFilename,
  siteScope,
  validSiteText,
} from '../sites/source.ts';
import type { ClientStore } from './store';
import type { ClientState } from './client-store-state';
import type { SiteSource } from './generated/SiteSource';

const scope = { identity_id: 'owner', device_id: 'device', space_id: 'chat' };
const source = { space_id: 'chat', item_id: 'item', version: 1, sha256: 'oldhash', byte_length: 2 };
function fixture() {
  let state = {
    phase: 'ready',
    erasePending: false,
    mode: 'account',
    snapshot: {
      account: { identity_id: 'owner', device_id: 'device', unlocked: true, setup_pending: false },
      me: { id: 'owner' },
      conversations: [{ id: 'chat', members: [{ id: 'owner' }] }],
    },
  } as unknown as ClientState;
  let epoch = 1,
    calls = 0;
  const html = '<h1>Saved 🐦</h1>';
  const bytes = new TextEncoder().encode(html);
  const result: SiteSource = {
    source: { ...source, version: 2, sha256: bytesToHex(sha256(bytes)), byte_length: bytes.length },
    current_version: 2,
    editable: true,
    html,
    name: 'site.html',
  };
  let resolve!: (value: SiteSource) => void;
  const pending = new Promise<SiteSource>((r) => {
    resolve = r;
  });
  const store = {
    getSnapshot: () => state,
    get sessionEpoch() {
      return epoch;
    },
    request: () => {
      calls++;
      return pending;
    },
    driver: {},
  } as unknown as ClientStore;
  return {
    store,
    html,
    result,
    resolve,
    calls: () => calls,
    close: () => {
      epoch++;
    },
    revoke: () => {
      state = { ...state, snapshot: { ...state.snapshot!, conversations: [] } };
    },
  };
}
test('site source scope requires the unlocked current device and canonical chat membership', () => {
  const f = fixture();
  assert.deepEqual(siteScope(f.store, 'chat'), scope);
  assert.equal(siteScope(f.store, 'foreign'), null);
  assert.equal(siteAuthority(f.store, { ...scope, device_id: 'other' }), null);
  f.revoke();
  assert.equal(siteAuthority(f.store, scope), null);
});
test('site drafts preserve exact Unicode and do not normalize the saved source descriptor', () => {
  assert(validSiteText('😀'.repeat(524288)));
  assert(!validSiteText('😀'.repeat(524289)));
  assert(!validSiteText('') && !validSiteText('x\0y'));
  assert(sameSource(source, { ...source }));
  for (const patch of [
    { version: 2 },
    { byte_length: 3 },
    { sha256: 'other' },
    { item_id: 'other' },
    { space_id: 'other' },
  ])
    assert(!sameSource(source, { ...source, ...patch }));
  assert.equal(siteFilename('../A\\B.html'), '.. A B.html');
  assert(emptySite('<script>"&').includes('&lt;script&gt;&quot;&amp;'));
  assert(!emptySite('<script>').includes('<script>'));
});
test('checked source save delivers only the exact next canonical HTML/version/hash', async () => {
  const f = fixture();
  const pending = saveObservedSite(
    f.store,
    scope,
    siteAuthority(f.store, scope)!,
    source,
    f.html,
    () => true,
  );
  assert.equal(f.calls(), 1);
  f.resolve(f.result);
  assert.deepEqual(await pending, f.result);
});
test('late save result cannot replace the editor after account epoch, revoked membership or focus loss', async () => {
  for (const kind of ['close', 'revoke', 'blur']) {
    const f = fixture();
    let active = true;
    const pending = saveObservedSite(
      f.store,
      scope,
      siteAuthority(f.store, scope)!,
      source,
      f.html,
      () => active,
    );
    if (kind === 'close') f.close();
    else if (kind === 'revoke') f.revoke();
    else active = false;
    f.resolve(f.result);
    assert.equal(await pending, null, kind);
  }
});
test('corrupt saved descriptor and out-of-bound drafts never become a successful source save', async () => {
  const f = fixture();
  await assert.rejects(
    saveObservedSite(f.store, scope, siteAuthority(f.store, scope)!, source, '\0', () => true),
  );
  assert.equal(f.calls(), 0);
  const pending = saveObservedSite(
    f.store,
    scope,
    siteAuthority(f.store, scope)!,
    source,
    f.html,
    () => true,
  );
  f.resolve({ ...f.result, source: { ...f.result.source, sha256: 'wrong' } });
  await assert.rejects(pending);
});
