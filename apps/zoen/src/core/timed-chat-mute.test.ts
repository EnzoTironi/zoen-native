import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MutedChatsSession, type MuteChoice } from '../notifications/muted-chats.ts';
import { mutedSnapshot, scope, deferred } from './muted-chats.fixtures.ts';
import type { AccountScope } from './account-scope.ts';
import type { ChatMuteMode } from './generated/ChatMuteMode';

async function fixture(single = true) {
  const snapshot = { ...mutedSnapshot(), clock_ms: '1800000000000' as string | undefined };
  let owner: AccountScope | null = scope;
  const writes: Array<{ mode: ChatMuteMode; observed: string | null }> = [];
  let read = async () => structuredClone(snapshot);
  let write = async (mode: ChatMuteMode) => {
    const until =
      mode.kind === 'for'
        ? String(BigInt(mode.from_ms) + BigInt(mode.duration_seconds) * 1000n)
        : mode.kind === 'until'
          ? mode.until_ms
          : mode.kind === 'forever'
            ? '9223372036854775807'
            : null;
    Object.assign(snapshot.conversations[0]!, { muted: until !== null, muted_until_ms: until });
  };
  const session = new MutedChatsSession(
    {
      scope: () => owner,
      read: () => read(),
      unmute: async () => assert.fail('Timed choices must use their exact canonical command'),
      mute: async (_id, mode, observed) => {
        writes.push({ mode, observed });
        await write(mode);
      },
    },
    single ? 'chat-0' : undefined,
  );
  session.activate(owner);
  await session.load(session.ticket());
  session.select('chat-0', session.ticket());
  return {
    session,
    snapshot,
    writes,
    choose: (choice: MuteChoice) => session.mute('chat-0', choice, session.ticket()),
    read: (fn: typeof read) => {
      read = fn;
    },
    write: (fn: typeof write) => {
      write = fn;
    },
    owner: (value: AccountScope | null) => {
      owner = value;
    },
  };
}

for (const seconds of [3600, 172800])
  test(`duration ${seconds} freezes the fresh Core clock and reconciles the exact canonical deadline`, async () => {
    const f = await fixture();
    f.snapshot.clock_ms = '1800000000500';
    await f.choose({ kind: 'for', duration_seconds: seconds });
    assert.deepEqual(f.writes, [
      {
        mode: { kind: 'for', duration_seconds: seconds, from_ms: '1800000000500' },
        observed: '9223372036854775807',
      },
    ]);
    assert.equal(
      f.session.getSnapshot().chats[0]!.muted_until_ms,
      String(1800000000500n + BigInt(seconds) * 1000n),
    );
    assert.equal(f.session.getSnapshot().actionFailed, false);
  });

for (const choice of [
  { kind: 'until', until_ms: '1800098765000' },
  { kind: 'forever' },
  { kind: 'off' },
] as const)
  test(`${choice.kind} sends the explicit choice without another confirmation or time source`, async () => {
    const f = await fixture();
    await f.choose(choice);
    assert.deepEqual(f.writes[0]!.mode, choice);
    assert.equal(f.session.getSnapshot().actionFailed, false);
    if (choice.kind === 'off') assert.equal(f.session.getSnapshot().chats[0]!.muted_until_ms, null);
  });

test('a missing Core clock fails visibly without sending a duration command', async () => {
  const f = await fixture();
  f.snapshot.clock_ms = undefined;
  await f.choose({ kind: 'for', duration_seconds: 3600 });
  assert.deepEqual(f.writes, []);
  assert.equal(f.session.getSnapshot().actionFailed, true);
  assert.equal(f.session.getSnapshot().pending, null);
});

test('expiry is reconciled by the canonical projection, while single-chat controls remain available', async () => {
  for (const single of [false, true]) {
    const f = await fixture(single);
    Object.assign(f.snapshot.conversations[0]!, { muted: false, muted_until_ms: null });
    await f.session.load(f.session.ticket());
    assert.equal(
      f.session.getSnapshot().chats.some((chat) => chat.id === 'chat-0'),
      single,
    );
  }
});

test('a changed observed deadline retires the choice and reloads rather than overwriting it', async () => {
  const f = await fixture();
  f.snapshot.conversations[0]!.muted_until_ms = '1800000500000';
  await f.choose({ kind: 'forever' });
  assert.deepEqual(f.writes, []);
  assert.equal(f.session.getSnapshot().chats[0]!.muted_until_ms, '1800000500000');
});

for (const applied of [false, true])
  test(`unknown ACK reconciles ${applied ? 'applied' : 'unapplied'} exact deadline without retrying`, async () => {
    const f = await fixture();
    f.write(async () => {
      if (applied) f.snapshot.conversations[0]!.muted_until_ms = '1800003600000';
      throw Error('Response lost');
    });
    await f.choose({ kind: 'for', duration_seconds: 3600 });
    assert.equal(f.writes.length, 1);
    assert.equal(f.session.getSnapshot().actionFailed, !applied);
    assert.equal(f.session.getSnapshot().pending, null);
  });

for (const transition of ['retire', 'owner'] as const)
  test(`late canonical read cannot dispatch after ${transition}`, async () => {
    const f = await fixture();
    const read = deferred<typeof f.snapshot>();
    f.read(() => read.promise);
    const action = f.choose({ kind: 'for', duration_seconds: 3600 });
    await Promise.resolve();
    if (transition === 'retire') f.session.retire();
    else f.owner({ ...scope, epoch: scope.epoch + 1 });
    read.resolve(f.snapshot);
    await action;
    assert.deepEqual(f.writes, []);
  });

test('an unknown ACK whose finite window has elapsed retires without reanchoring or claiming historical acceptance', async () => {
  const f = await fixture();
  f.write(async () => {
    f.snapshot.clock_ms = '1800003600001';
    Object.assign(f.snapshot.conversations[0]!, { muted: false, muted_until_ms: null });
    throw Error('Delayed response lost');
  });
  await f.choose({ kind: 'for', duration_seconds: 3600 });
  assert.equal(f.writes.length, 1);
  assert.equal(f.session.getSnapshot().actionFailed, false);
  assert.equal(f.session.getSnapshot().chats[0]!.muted_until_ms, null);
});
