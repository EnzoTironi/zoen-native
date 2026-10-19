import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nativeLinkPath, parseZoenLink, validLocalNotification } from '../notifications/links.ts';

test('documented Zoen links preserve signed IDs and require review for capability invitations', () => {
  assert.equal(nativeLinkPath('zoen://app/it_signed'), '/item/it_signed');
  assert.equal(
    nativeLinkPath('https://tryzoen.com/a/agent_name'),
    '/open-link?kind=agent&target=agent_name',
  );
  assert.deepEqual(parseZoenLink('zoen://join/abcde12345'), { kind: 'invite', code: 'ABCDE12345' });
  assert.equal(
    nativeLinkPath('zoen://join/abcde12345'),
    '/open-link?kind=invite&target=ABCDE12345',
  );
});

test('link routing refuses foreign origins, encoded path escapes and action-bearing URLs', () => {
  for (const url of [
    'https://evil.test/a/agent',
    'https://tryzoen.com.evil.test/a/agent',
    'zoen://app/it%2Fprivate',
    'zoen://join/ABCDE12345?approve=true',
    'zoen://agent/a#execute',
    'javascript:alert(1)',
    'zoen://app/it%00private',
    'zoen://app/../../private',
    'https://name@tryzoen.com/a/agent',
  ])
    assert.equal(parseZoenLink(url), null, url);
  assert.equal(nativeLinkPath('malformed input'), '/open-link?invalid=1');
});

test('notification taps are account-bound local navigation hints and preserve full u64 cursors', () => {
  const data = {
    zoen_local: 1,
    device_id: 'current',
    space_id: 'sp_signed',
    event_seq: '18446744073709551615',
  };
  assert.deepEqual(validLocalNotification(data, 'current'), { spaceId: 'sp_signed' });
  for (const invalid of [
    { ...data, device_id: 'old' },
    { ...data, event_seq: 1 },
    { ...data, event_seq: '18446744073709551616' },
    { ...data, approve: true },
    { ...data, event_seq: '01' },
    { ...data, zoen_local: true },
  ])
    assert.equal(validLocalNotification(invalid, 'current'), null);
});

test('Expo SDK transport envelopes always open home and never follow their inner URL', () => {
  for (const scheme of ['zoen', 'exp+zoen']) {
    for (const transport of [
      'http://localhost:58704',
      'http://192.168.2.113:8081',
      'https://example.test/tunnel',
    ]) {
      for (const root of ['', '/']) {
        // Real Expo Router initial links include a slash before the SDK query.
        const launch = `${scheme}://${root}?__expo_url=${encodeURIComponent(transport)}`;
        assert.equal(nativeLinkPath(launch), '/', launch);
        assert.equal(
          nativeLinkPath(
            `${launch}&__expo_disable_fab=1&__expo_disable_auto_launch=1&__expo_disable_onboarding=1`,
          ),
          '/',
          launch,
        );
      }
    }
  }
});

test('only the exact SDK bootstrap shape is discarded; malformed links remain invalid', () => {
  const base = 'zoen://?__expo_url=http%3A%2F%2Flocalhost%3A58704';
  for (const value of [
    base.replace('zoen:', 'foreign:'),
    base.replace('zoen://?', 'zoen://chat/private?'),
    base.replace('zoen://?', 'zoen:///nested/path?'),
    base.replace('zoen://?', 'zoen:////?'),
    base.replace('zoen://?', 'zoen:/?'),
    base.replace('zoen://?', 'zoen:?'),
    `${base}&code=provider-proof`,
    `${base}&state=provider-state`,
    `${base}&__expo_url=http%3A%2F%2Flocalhost%3A58704`,
    `${base}&__expo_disable_fab=1`,
    `${base}&__expo_disable_fab=1&__expo_disable_auto_launch=0&__expo_disable_onboarding=1`,
    `${base}&__expo_disable_fab=1&__expo_disable_auto_launch=1&__expo_disable_onboarding=1&__expo_disable_fab=1`,
    `${base}#fragment`,
    ` ${base}`,
    `${base}\n`,
    'zoen://?__expo_url=javascript%3Aalert(1)',
    'zoen://?__expo_url=http%3A%2F%2Fuser%3Apassword%40localhost',
    'zoen://?__expo_url=not-a-url',
    'zoen:///?__expo_url=not-a-url',
    'zoen:///?__expo_url=javascript%3Aalert(1)',
    'exp+zoen:///?__expo_url=http%3A%2F%2Flocalhost&extra=value',
    `zoen://?__expo_url=${'x'.repeat(2048)}`,
  ])
    assert.equal(nativeLinkPath(value), '/open-link?invalid=1', value);
  assert.equal(nativeLinkPath('zoen://chat/sp_signed'), '/chat/sp_signed');
  assert.equal(
    nativeLinkPath('zoen://join/abcde12345'),
    '/open-link?kind=invite&target=ABCDE12345',
  );
});

test('Expo Router empty root scheme URLs open home on ordinary native cold launches', () => {
  for (const root of ['zoen:///', 'zoen://', 'exp+zoen:///', 'exp+zoen://'])
    assert.equal(nativeLinkPath(root), '/');
  for (const invalid of [
    'zoen:/',
    'foreign:///',
    'zoen://unknown',
    'zoen:///?code=proof',
    'zoen:///#fragment',
  ])
    assert.equal(nativeLinkPath(invalid), '/open-link?invalid=1');
});

test('development-client startup and redelivery open home without using the inner URL', () => {
  for (const slash of ['', '/']) {
    for (const transport of [
      'http://localhost:8089',
      'http://192.168.1.2:8081',
      'https://example.test/tunnel?state=not-a-provider-proof',
    ]) {
      const launch = `exp+zoen://expo-development-client${slash}?url=${encodeURIComponent(transport)}`;
      assert.equal(nativeLinkPath(launch), '/');
      assert.equal(parseZoenLink(launch), null);
    }
  }
});

test('development-client recognition cannot swallow product links or provider callbacks', () => {
  const launch = 'exp+zoen://expo-development-client/?url=http%3A%2F%2Flocalhost%3A8089';
  for (const invalid of [
    launch.replace('exp+zoen:', 'zoen:'),
    launch.replace('expo-development-client/', 'expo-development-client.evil.test/'),
    launch.replace('expo-development-client/', 'user@expo-development-client/'),
    launch.replace('expo-development-client/', 'expo-development-client:8089/'),
    launch.replace('expo-development-client/', 'expo-development-client/nested/'),
    launch.replace('?url=', '?__expo_url='),
    launch.replace('http%3A%2F%2Flocalhost%3A8089', 'zoen%3A%2F%2Fjoin%2FABCDE12345'),
    launch.replace('http%3A%2F%2Flocalhost%3A8089', 'javascript%3Aalert(1)'),
    launch.replace('http%3A%2F%2Flocalhost%3A8089', 'http%3A%2F%2Fuser%3Apassword%40localhost'),
    `${launch}&url=http%3A%2F%2Flocalhost%3A8081`,
    `${launch}&code=provider-proof`,
    `${launch}&state=provider-state`,
    `${launch}#fragment`,
    ` ${launch}`,
    `${launch}\n`,
    'exp+zoen://expo-development-client/?url=',
  ])
    assert.equal(nativeLinkPath(invalid), '/open-link?invalid=1', invalid);
  assert.equal(nativeLinkPath('zoen://chat/sp_signed'), '/chat/sp_signed');
  assert.equal(
    nativeLinkPath('zoen://join/abcde12345'),
    '/open-link?kind=invite&target=ABCDE12345',
  );
});
