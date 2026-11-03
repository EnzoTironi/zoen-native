import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MessageInfoSession } from './message-info-session.ts';
import { messageInfoDate, messageReadDate } from './message-info-presentation.ts';
import { draftSession, deferred } from './store.fixtures.ts';
import type { MessageInfo } from './generated/MessageInfo';
const source = {
  expected_identity_id: 'author',
  expected_device_id: 'device',
  space_id: 'chat',
  message_id: 'message',
};
function value(): MessageInfo {
  return {
    scope: source,
    author: draftSession().conversations[0]!.members[0]!,
    sent_at_ms: '1000',
    forwarded: false,
    edited: true,
    edited_at_ms: '2000',
    delivery: 'Read',
    read_details: 'available',
    participants: [
      {
        person: { ...draftSession().conversations[0]!.members[0]!, id: 'peer' },
        delivered: true,
        read: true,
        read_reported_at_ms: '3000',
        read_observed_at_ms: '9000',
      },
    ],
  };
}
function fixture() {
  let current = true;
  const requests: ReturnType<typeof deferred<MessageInfo>>[] = [];
  const seen: Array<{ value: MessageInfo | null; loading: boolean; error: boolean }> = [];
  const session = new MessageInfoSession(
    source,
    {
      current: () => current,
      read: async (scope) => {
        assert.deepEqual(scope, source);
        const request = deferred<MessageInfo>();
        requests.push(request);
        return request.promise;
      },
    },
    (value, loading, error) => seen.push({ value, loading, error }),
  );
  return {
    session,
    requests,
    seen,
    current: (next: boolean) => {
      current = next;
      session.refresh();
    },
  };
}
const tick = async () => {
  await new Promise((resolve) => setImmediate(resolve));
};
test('Info reads exact canonical scope and never substitutes receipt arrival for peer read date', async () => {
  const f = fixture();
  f.session.focus();
  f.requests[0]!.resolve(value());
  await tick();
  assert.equal(f.seen.at(-1)!.value!.edited_at_ms, '2000');
  const participant = f.seen.at(-1)!.value!.participants[0]!;
  assert.equal(messageReadDate(participant), '3000');
  assert.equal(messageReadDate({ ...participant, read_reported_at_ms: null }), null);
  assert.equal(messageReadDate({ ...participant, read: false }), null);
  assert.equal(messageInfoDate(null, 'en'), null);
  assert.equal(messageInfoDate('not-a-date', 'en'), null);
});
test('access loss and same-owner ABA retire held reads; only a new current read can publish', async () => {
  const f = fixture();
  f.session.focus();
  f.current(false);
  f.current(true);
  f.requests[0]!.resolve(value());
  await tick();
  assert.equal(f.requests.length, 2);
  assert.equal(
    f.seen.some((state) => state.value !== null),
    false,
  );
  f.requests[1]!.resolve(value());
  await tick();
  assert.equal(f.seen.at(-1)!.value!.scope.message_id, 'message');
  f.session.blur();
  assert.equal(f.seen.at(-1)!.value, null);
});
test('blur, wrong source and storage/query failure cannot masquerade as missing timestamps', async () => {
  const f = fixture();
  f.session.focus();
  f.session.blur();
  f.requests[0]!.resolve(value());
  await tick();
  assert.equal(f.seen.at(-1)!.value, null);
  f.session.focus();
  f.requests[1]!.resolve({ ...value(), scope: { ...source, message_id: 'foreign' } });
  await tick();
  assert.equal(f.seen.at(-1)!.error, true);
  f.session.refresh();
  f.requests[2]!.reject(new Error('canonical read failed'));
  await tick();
  assert.equal(f.seen.at(-1)!.error, true);
  assert.equal(f.seen.at(-1)!.value, null);
});
