import assert from 'node:assert/strict';
import { test } from 'node:test';
import { publishObservedSite, publicSiteURL } from '../sites/publication.ts';
import { siteAuthority } from '../sites/source.ts';
import type { ClientStore } from './store';
import type { ClientState } from './client-store-state';
import type { SiteReview } from './generated/SiteReview';
import type { Receipt } from './generated/Receipt';

const scope = { identity_id: 'owner', device_id: 'device', space_id: 'chat' };
const source = {
  space_id: 'chat',
  item_id: 'item',
  version: 1,
  sha256: 'a'.repeat(64),
  byte_length: 2,
};
function fixture() {
  const state = {
    phase: 'ready',
    erasePending: false,
    mode: 'account',
    snapshot: {
      account: { identity_id: 'owner', device_id: 'device', unlocked: true, setup_pending: false },
      me: { id: 'owner' },
      conversations: [{ id: 'chat', members: [{ id: 'owner' }] }],
    },
  } as unknown as ClientState;
  let epoch = 1;
  const review: SiteReview = {
    scope,
    source,
    relay_url: 'https://relay.test',
    configuration: {
      schema: 1,
      audience: 'relay.test',
      management_origin: 'https://api.test',
      public_origin: 'https://sites.test',
      max_html_bytes: 2097152,
    },
    mutation: {
      schema: 1,
      public_origin: 'https://sites.test',
      site_id: `site_${'b'.repeat(32)}`,
      request_id: `pub_${'c'.repeat(32)}`,
      expected_head: null,
      change: { kind: 'publish', source },
    },
  };
  const receipt: Receipt = {
    schema: 1,
    site_id: review.mutation.site_id,
    request_id: review.mutation.request_id,
    owner_id: 'owner',
    head: 'd'.repeat(64),
    previous_head: null,
    source,
    active: true,
  };
  let writes = 0;
  const store = {
    getSnapshot: () => state,
    get sessionEpoch() {
      return epoch;
    },
    driver: { request: async () => review },
    request: async () => {
      writes++;
      return receipt;
    },
  } as unknown as ClientStore;
  return {
    store,
    review,
    receipt,
    writes: () => writes,
    close: () => {
      epoch++;
    },
  };
}
test('publication signs/sends only after explicit consent for the exact saved version and public destination', async () => {
  const f = fixture();
  const result = await publishObservedSite(
    f.store,
    scope,
    siteAuthority(f.store, scope)!,
    source,
    false,
    () => true,
    async (review, url) => {
      assert.equal(f.writes(), 0);
      assert.equal(review.source.version, 1);
      assert.equal(url, `https://sites.test/s/site_${'b'.repeat(32)}`);
      return true;
    },
  );
  assert.equal(f.writes(), 1);
  assert.deepEqual(result, f.receipt);
});
test('cancelled dialog, a closing account and an old selected version never send publication', async () => {
  for (const kind of ['cancel', 'close', 'version']) {
    const f = fixture();
    if (kind === 'version') f.review.source = { ...source, version: 2 };
    const operation = publishObservedSite(
      f.store,
      scope,
      siteAuthority(f.store, scope)!,
      source,
      false,
      () => true,
      async () => {
        if (kind === 'close') f.close();
        return kind !== 'cancel';
      },
    );
    if (kind === 'version') await assert.rejects(operation);
    else assert.equal(await operation, null);
    assert.equal(f.writes(), 0);
  }
});
test('changing the reviewed intent while the native dialog is pending requires new consent', async () => {
  const f = fixture();
  await assert.rejects(
    publishObservedSite(
      f.store,
      scope,
      siteAuthority(f.store, scope)!,
      source,
      false,
      () => true,
      async (review) => {
        review.mutation.expected_head = 'e'.repeat(64);
        return true;
      },
    ),
  );
  assert.equal(f.writes(), 0);
});
test('public destination is canonical, matches the signed mutation and permits HTTP only on loopback', () => {
  const f = fixture();
  for (const origin of [
    'http://remote.test',
    'https://sites.test/path',
    'https://user:pass@sites.test',
    'https://sites.test?x=1',
  ]) {
    f.review.configuration.public_origin = origin;
    f.review.mutation.public_origin = origin;
    assert.throws(() => publicSiteURL(f.review));
  }
  f.review.configuration.public_origin = 'http://127.0.0.1:7358';
  f.review.mutation.public_origin = f.review.configuration.public_origin;
  assert.equal(publicSiteURL(f.review), `http://127.0.0.1:7358/s/${f.review.mutation.site_id}`);
  f.review.mutation.public_origin = 'https://different.test';
  assert.throws(() => publicSiteURL(f.review));
});
test('a mismatched receipt is never reported as the completed publication', async () => {
  const f = fixture();
  f.receipt.request_id = 'other';
  await assert.rejects(
    publishObservedSite(
      f.store,
      scope,
      siteAuthority(f.store, scope)!,
      source,
      false,
      () => true,
      async () => true,
    ),
  );
  assert.equal(f.writes(), 1);
});
