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

test('local Metro development launches open the entry screen without becoming invalid invitations', () => {
  const launch = 'exp+zoen://?__expo_url=http%3A%2F%2Flocalhost%3A58704';
  assert.equal(nativeLinkPath(launch, true), '/');
  assert.equal(nativeLinkPath(launch, false), '/open-link?invalid=1');
  for (const value of [
    'foreign://?__expo_url=http%3A%2F%2Flocalhost%3A58704',
    'exp+zoen://?__expo_url=https%3A%2F%2Fforeign.test',
    'exp+zoen://chat/private?__expo_url=http%3A%2F%2Flocalhost%3A58704',
  ])
    assert.equal(nativeLinkPath(value, true), '/open-link?invalid=1');
});
