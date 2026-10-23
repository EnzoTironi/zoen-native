import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  BlockedPeopleSession,
  createBlockedPeoplePort,
  type BlockedPeopleScope,
} from '../profile/blocked-people.ts';
import type { ProfileDto } from './generated/ProfileDto.ts';
import type { ClientStore } from './store.ts';

const first = 'a'.repeat(64);
const second = 'b'.repeat(64);
const owner: BlockedPeopleScope = {
  identity: 'owner',
  device: 'phone',
  relay: 'https://relay.example/',
  epoch: 1,
};
const profile = (id: string): ProfileDto => ({
  identity_id: id,
  handle: id === first ? 'nina' : 'sam',
  name: id === first ? 'Nina' : 'Sam',
  bio: null,
  photo_sha256: null,
  photo_ready: false,
  photo_mime: null,
  version: '1',
  tint_hex: '#556677',
  is_me: false,
  blocked: true,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function fixture() {
  let current: BlockedPeopleScope | null = { ...owner };
  let ids = [first, second];
  let read: () => Promise<string[]> = async () => [...ids];
  let lookup: (id: string) => Promise<ProfileDto> = async (id) => profile(id);
  let mutation: (id: string) => Promise<unknown> = async (id) => {
    ids = ids.filter((value) => value !== id);
  };
  const calls: string[] = [];
  const session = new BlockedPeopleSession({
    scope: () => current,
    blocked: () => {
      calls.push('read');
      return read();
    },
    profile: (id) => {
      calls.push(`profile:${id}`);
      return lookup(id);
    },
    unblock: (id) => {
      calls.push(`unblock:${id}`);
      return mutation(id);
    },
  });
  session.activate(current);
  return {
    session,
    calls,
    load: () => session.load(session.ticket()),
    profile: (id: string) => session.loadProfile(id, session.ticket()),
    unblock: (id: string, confirm: () => Promise<boolean>) =>
      session.unblock(id, confirm, session.ticket()),
    scope: (next: BlockedPeopleScope | null) => {
      current = next;
    },
    read: (next: typeof read) => {
      read = next;
    },
    lookup: (next: typeof lookup) => {
      lookup = next;
    },
    mutation: (next: typeof mutation) => {
      mutation = next;
    },
    ids: (next: string[]) => {
      ids = next;
    },
  };
}

test('Core list is the source of truth; profile reads are lazy and exact; confirmed unblock rereads once', async () => {
  const f = fixture();
  await f.load();
  assert.deepEqual(f.calls, ['read']);
  await f.profile(first);
  await f.profile(first);
  assert.equal(f.session.getSnapshot().people[0]!.profile?.name, 'Nina');
  assert.equal(f.session.canOpen(first, f.session.ticket()), true);
  assert.equal(f.session.canOpen(second, f.session.ticket()), false);
  await f.unblock(first, async () => true);
  assert.deepEqual(f.calls, ['read', `profile:${first}`, `unblock:${first}`, 'read']);
  assert.deepEqual(
    f.session.getSnapshot().people.map((entry) => entry.id),
    [second],
  );
  assert.equal(f.session.getSnapshot().pending, null);
});

test('cancel, duplicate taps and arbitrary identities cannot dispatch', async () => {
  const f = fixture();
  await f.load();
  const answer = deferred<boolean>();
  let dialogs = 0;
  const pending = f.unblock(first, () => {
    ++dialogs;
    return answer.promise;
  });
  await f.unblock(second, async () => {
    ++dialogs;
    return true;
  });
  assert.equal(dialogs, 1);
  assert.equal(f.session.getSnapshot().pending, first);
  answer.resolve(false);
  await pending;
  await f.unblock('foreign', async () => {
    ++dialogs;
    return true;
  });
  assert.deepEqual(f.calls, ['read']);
  assert.equal(f.session.getSnapshot().pending, null);
});

for (const [field, value] of [
  ['identity', 'another-owner'],
  ['device', 'other-phone'],
  ['relay', 'https://relay.example'],
  ['epoch', 2],
] as const) {
  test(`late dialog cannot unblock after ${field} changes, even a canonically equivalent raw relay`, async () => {
    const f = fixture();
    await f.load();
    const answer = deferred<boolean>();
    const pending = f.unblock(first, () => answer.promise);
    f.scope({ ...owner, [field]: value });
    answer.resolve(true);
    await pending;
    assert.deepEqual(f.calls, ['read']);
    assert.equal(f.session.canOpen(first, f.session.ticket()), false);
  });
}

test('retirement and refreshed revision invalidate an old dialog without clearing a newer attempt', async () => {
  const f = fixture();
  await f.load();
  const old = deferred<boolean>();
  const pending = f.unblock(first, () => old.promise);
  f.session.retire();
  f.session.activate(owner);
  await f.load();
  const newer = deferred<boolean>();
  const next = f.unblock(second, () => newer.promise);
  old.resolve(true);
  await pending;
  assert.equal(f.session.getSnapshot().pending, second);
  newer.resolve(false);
  await next;
  assert.equal(f.calls.filter((call) => call.startsWith('unblock:')).length, 0);
  const stale = deferred<boolean>();
  const last = f.unblock(first, () => stale.promise);
  await f.load();
  stale.resolve(true);
  await last;
  assert.equal(f.calls.filter((call) => call.startsWith('unblock:')).length, 0);
});

test('read failures retain only disabled prior rows, and retry restores actual empty state', async () => {
  const f = fixture();
  await f.load();
  f.read(async () => {
    throw new Error('synthetic read failure');
  });
  await f.load();
  assert.equal(f.session.getSnapshot().phase, 'failed');
  assert.equal(f.session.getSnapshot().readFailed, true);
  await f.unblock(first, async () => true);
  assert.equal(
    f.calls.some((call) => call.startsWith('unblock:')),
    false,
  );
  f.read(async () => []);
  await f.load();
  assert.equal(f.session.getSnapshot().phase, 'ready');
  assert.deepEqual(f.session.getSnapshot().people, []);
  assert.equal(f.session.getSnapshot().readFailed, false);
});

test('lost local mutation result only reconciles by read, never retries; refresh errors remain visible', async () => {
  const f = fixture();
  await f.load();
  f.mutation(async (id) => {
    f.ids([second]);
    throw new Error(`lost acknowledgement ${id}`);
  });
  await f.unblock(first, async () => true);
  assert.equal(f.calls.filter((call) => call === `unblock:${first}`).length, 1);
  assert.deepEqual(
    f.session.getSnapshot().people.map((entry) => entry.id),
    [second],
  );
  assert.equal(f.session.getSnapshot().actionFailed, true);
  assert.equal(f.session.getSnapshot().pending, null);
  f.read(async () => {
    throw new Error('read failure');
  });
  await f.unblock(second, async () => true);
  assert.equal(f.session.getSnapshot().phase, 'failed');
  assert.equal(f.session.getSnapshot().readFailed, true);
  assert.equal(f.session.getSnapshot().actionFailed, true);
});

test('late list/profile results and foreign profile data cannot enter a replacement scope', async () => {
  const f = fixture();
  const read = deferred<string[]>();
  f.read(() => read.promise);
  const pending = f.load();
  f.session.retire();
  read.resolve([first]);
  await pending;
  assert.deepEqual(f.session.getSnapshot().people, []);
  f.session.activate(owner);
  f.read(async () => [first]);
  await f.load();
  f.lookup(async () => profile(second));
  await f.profile(first);
  assert.equal(f.session.getSnapshot().people[0]!.phase, 'failed');
  assert.equal(f.session.getSnapshot().people[0]!.profile, null);
  const old = deferred<ProfileDto>();
  f.lookup(() => old.promise);
  const lookup = f.profile(first);
  f.session.retire();
  old.resolve(profile(first));
  await lookup;
  assert.deepEqual(f.session.getSnapshot().people, []);
});

test('duplicate Core identities fail closed rather than producing ambiguous selected rows', async () => {
  const f = fixture();
  f.read(async () => [first, first]);
  await f.load();
  assert.equal(f.session.getSnapshot().phase, 'failed');
  await f.unblock(first, async () => true);
  assert.deepEqual(f.calls, ['read']);
});

test('production port uses existing typed commands and requires a live unlocked registered owner', async () => {
  const calls: unknown[] = [];
  const state: any = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: owner.relay,
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
    },
  };
  const store: any = {
    getSnapshot: () => state,
    sessionEpoch: 4,
    isSessionRetiring: false,
    driver: {
      request: async (command: unknown) => {
        calls.push(command);
        return [];
      },
    },
    request: async (command: unknown) => {
      calls.push(command);
    },
  };
  const port = createBlockedPeoplePort(store as ClientStore);
  assert.deepEqual(port.scope(), { ...owner, epoch: 4 });
  await port.blocked();
  await port.profile(first);
  await port.unblock(second);
  assert.deepEqual(calls, [
    { op: 'blocked_people' },
    { op: 'profile', identity_id: first },
    { op: 'unblock_person', identity_id: second },
  ]);
  for (const key of ['registered', 'unlocked']) {
    state.snapshot.account[key] = false;
    assert.equal(port.scope(), null);
    state.snapshot.account[key] = true;
  }
  state.snapshot.account.setup_pending = true;
  assert.equal(port.scope(), null);
  state.snapshot.account.setup_pending = false;
  state.erasePending = true;
  assert.equal(port.scope(), null);
  state.erasePending = false;
  store.isSessionRetiring = true;
  assert.equal(port.scope(), null);
  store.isSessionRetiring = false;
  state.mode = 'demo';
  assert.equal(port.scope(), null);
});
