import assert from 'node:assert/strict';
import test from 'node:test';
import { authorizedNetworkHosts } from './network.ts';

test('a displayed grant from another member cannot open the current owner network sandbox', async () => {
  const grantsInSharedLog = [
    { capability: 'net:tiles.example.org', grantor: 'another-member' },
    { capability: 'net:api.example.org', grantor: 'current-owner' },
  ];
  const checked: string[] = [];
  const hosts = await authorizedNetworkHosts(
    ['tiles.example.org', 'api.example.org'],
    async (capability) => {
      checked.push(capability);
      return grantsInSharedLog.some(
        (grant) => grant.capability === capability && grant.grantor === 'current-owner',
      );
    },
    () => {},
  );
  assert.deepEqual(hosts, ['api.example.org']);
  assert.deepEqual(checked, ['net:tiles.example.org', 'net:api.example.org']);
});

test('unavailable or obsolete authorization never produces an open CSP allowlist', async () => {
  await assert.rejects(
    authorizedNetworkHosts(
      ['tiles.example.org'],
      async () => {
        throw new Error('offline');
      },
      () => {},
    ),
    /offline/,
  );
  let active = true;
  await assert.rejects(
    authorizedNetworkHosts(
      ['tiles.example.org'],
      async () => {
        active = false;
        return true;
      },
      () => {
        if (!active) throw new Error('closed');
      },
    ),
    /closed/,
  );
});
