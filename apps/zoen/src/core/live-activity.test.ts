import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import {
  LiveActivity,
  TypingSender,
  emptyActivity,
  chatActivityLabel,
  type ActivityClock,
} from './live-activity.ts';
import type { CoreDriver, CoreEvent, Snapshot } from './types.ts';

type Timer = ReturnType<typeof setTimeout>;
class Clock implements ActivityClock {
  time = 0;
  sequence = 0;
  timers = new Map<number, { at: number; run: () => void }>();
  now = () => this.time;
  setTimeout = (run: () => void, delay: number): Timer => {
    const id = ++this.sequence;
    this.timers.set(id, { at: this.time + delay, run });
    return id as unknown as Timer;
  };
  clearTimeout = (id: Timer) => {
    this.timers.delete(id as unknown as number);
  };
  advance(ms: number) {
    const end = this.time + ms;
    for (;;) {
      const pending = [...this.timers]
        .filter(([, timer]) => timer.at <= end)
        .sort((a, b) => a[1].at - b[1].at)[0];
      if (!pending) break;
      this.time = pending[1].at;
      this.timers.delete(pending[0]);
      pending[1].run();
    }
    this.time = end;
  }
}
function snapshot(): Snapshot {
  return {
    account: null,
    me: { id: 'me' } as Snapshot['me'],
    connection: { state: 'online', synced: true, pending: '0', error: null },
    conversations: [
      {
        id: 'room',
        kind: 'Group',
        members: [
          { id: 'me', name: 'Eu' },
          { id: 'ana', name: 'Ana' },
          { id: 'bia', name: 'Bia' },
        ],
        counterpart: null,
      } as Snapshot['conversations'][number],
    ],
    agents: [],
    integrity_warnings: {},
    has_unreadable_local_history: false,
    requests: [],
  };
}
const typing = (fromId: string, type = 'typing', spaceId = 'room'): CoreEvent => ({
  kind: 'ephemeral',
  fromId,
  spaceId,
  type,
  detail: '',
});
async function flush() {
  for (let i = 0; i < 12; i++) await Promise.resolve();
}

test('concurrent typing deadlines are independent and only current other members can signal', () => {
  const clock = new Clock();
  const scope = snapshot();
  let state = emptyActivity();
  const live = new LiveActivity(
    () => scope,
    (next) => {
      state = next;
    },
    clock,
  );
  live.synchronize(scope);
  for (const event of [
    typing('me'),
    typing('stranger'),
    typing('ana', 'typing', 'unknown'),
    typing('ana', 'status'),
  ])
    live.receive(event as Extract<CoreEvent, { kind: 'ephemeral' }>);
  assert.deepEqual(state.typing, {});
  live.receive(typing('ana') as Extract<CoreEvent, { kind: 'ephemeral' }>);
  clock.advance(3000);
  live.receive(typing('bia') as Extract<CoreEvent, { kind: 'ephemeral' }>);
  assert.equal(chatActivityLabel(scope, state, 'room'), 'Ana and Bia are typing…');
  clock.advance(3000);
  assert.deepEqual(Object.keys(state.typing.room!), ['bia']);
  assert.equal(chatActivityLabel(scope, state, 'room'), 'Bia is typing…');
  live.receive(typing('bia', 'stopped') as Extract<CoreEvent, { kind: 'ephemeral' }>);
  assert.deepEqual(state.typing, {});
  assert.equal(clock.timers.size, 0);
});

test('presence never invents last seen and membership removal/disconnect clear hints', () => {
  const clock = new Clock();
  let scope = snapshot();
  let state = emptyActivity();
  const live = new LiveActivity(
    () => scope,
    (next) => {
      state = next;
    },
    clock,
  );
  live.synchronize(scope);
  live.receive({ kind: 'presence', identityId: 'ana', online: true });
  live.receive({ kind: 'presence', identityId: 'me', online: true });
  live.receive({ kind: 'presence', identityId: 'stranger', online: true });
  assert.deepEqual(state.online, { ana: true });
  live.receive({ kind: 'presence', identityId: 'ana', online: false });
  assert.deepEqual(state.online, {});
  live.receive(typing('bia') as Extract<CoreEvent, { kind: 'ephemeral' }>);
  scope = {
    ...scope,
    conversations: [
      {
        ...scope.conversations[0]!,
        members: scope.conversations[0]!.members.filter((p) => p.id !== 'bia'),
      },
    ],
  };
  live.synchronize(scope);
  assert.deepEqual(live.value().typing, {});
  scope = { ...scope, connection: { ...scope.connection, state: 'offline', synced: false } };
  live.synchronize(scope);
  assert.deepEqual(live.value(), emptyActivity());
  live.receive({ kind: 'presence', identityId: 'ana', online: true });
  clock.advance(10000);
  assert.deepEqual(live.value(), emptyActivity());
});

test('outbound typing refreshes on a bounded cadence, stops when idle or cleared, and never sends text', async () => {
  const clock = new Clock();
  const frames: unknown[] = [];
  const sender = new TypingSender(
    (space) => space === 'room',
    async (space, typing) => {
      frames.push({ space, typing });
    },
    clock,
  );
  sender.update('room', true);
  sender.update('room', true);
  sender.update('foreign', true);
  await flush();
  assert.deepEqual(frames, [{ space: 'room', typing: true }]);
  clock.advance(1999);
  sender.update('room', true);
  await flush();
  assert.equal(frames.length, 1);
  clock.advance(1);
  sender.update('room', true);
  await flush();
  assert.equal(frames.length, 2);
  clock.advance(4000);
  await flush();
  assert.deepEqual(frames.at(-1), { space: 'room', typing: false });
  sender.update('room', false);
  await flush();
  assert.equal(frames.length, 3);
  sender.update('room', true);
  sender.update('room', false);
  await flush();
  assert.deepEqual(frames.slice(-2), [
    { space: 'room', typing: true },
    { space: 'room', typing: false },
  ]);
});

test('generation reset discards queued typing and cancels idle timers without replay after reconnect', async () => {
  const clock = new Clock();
  const frames: boolean[] = [];
  const sender = new TypingSender(
    () => true,
    async (_space, typing) => {
      frames.push(typing);
    },
    clock,
  );
  sender.update('room', true);
  sender.reset();
  clock.advance(10000);
  await flush();
  assert.deepEqual(frames, []);
  sender.update('room', true);
  await flush();
  assert.deepEqual(frames, [true]);
  sender.reset();
  clock.advance(10000);
  await flush();
  assert.deepEqual(frames, [true]);
});

test('ClientStore routes live hints without durable refresh and ignores old account listeners', async () => {
  const clock = new Clock();
  const scope = snapshot();
  let listener: ((event: CoreEvent) => void) | undefined;
  const requests: string[] = [];
  const driver = {
    async open() {
      return scope;
    },
    async close() {},
    subscribe(next: (event: CoreEvent) => void) {
      listener = next;
      return () => {};
    },
    async request(command: { op: string }) {
      requests.push(command.op);
      return scope;
    },
  } as unknown as CoreDriver;
  const store = new ClientStore(driver, clock);
  await store.open('demo');
  listener!({ kind: 'presence', identityId: 'ana', online: true });
  listener!(typing('ana'));
  assert.deepEqual(store.getSnapshot().activity.online, { ana: true });
  assert.equal(requests.length, 0);
  store.setTyping('room', true);
  await flush();
  assert.deepEqual(requests, ['set_typing']);
  const old = listener!;
  await store.open('demo');
  old(typing('ana'));
  assert.deepEqual(store.getSnapshot().activity, emptyActivity());
  listener!(typing('bia'));
  clock.advance(6000);
  assert.deepEqual(store.getSnapshot().activity.typing, {});
  listener!({ kind: 'presence', identityId: 'ana', online: true });
  listener!({ kind: 'connection' });
  assert.deepEqual(store.getSnapshot().activity, emptyActivity());
  await flush();
  await store.close();
  listener!(typing('ana'));
  assert.deepEqual(store.getSnapshot().activity, emptyActivity());
});

test('initial connection hints wait for the fresh scope and never extend old typing deadlines', () => {
  const clock = new Clock();
  const scope = snapshot();
  const live = new LiveActivity(
    () => scope,
    () => {},
    clock,
  );
  live.beginConnection();
  live.receive({ kind: 'presence', identityId: 'ana', online: true });
  live.receive(typing('bia') as Extract<CoreEvent, { kind: 'ephemeral' }>);
  assert.deepEqual(live.value(), emptyActivity());
  clock.advance(6000);
  live.synchronize(scope);
  assert.deepEqual(live.value().online, { ana: true });
  assert.deepEqual(live.value().typing, {});
  live.beginConnection();
  live.receive({ kind: 'presence', identityId: 'bia', online: true });
  live.synchronize({
    ...scope,
    connection: { ...scope.connection, state: 'offline', synced: false },
  });
  assert.deepEqual(live.value(), emptyActivity());
});

test('recognized processing/building/call hints expire, ignore strangers and clear on explicit offline', () => {
  const clock = new Clock();
  const scope = snapshot();
  let state = emptyActivity();
  const live = new LiveActivity(
    () => scope,
    (next) => {
      state = next;
    },
    clock,
  );
  const status = (fromId: string, detail: string) =>
    ({ kind: 'ephemeral', spaceId: 'room', fromId, type: 'status', detail }) as const;
  live.synchronize(scope);
  live.receive(status('stranger', 'processing'));
  assert.deepEqual(state.statuses, {});
  live.receive(status('ana', 'processing'));
  assert.equal(chatActivityLabel(scope, state, 'room'), 'Ana is processing…');
  clock.advance(60_000);
  assert.deepEqual(state.statuses, {});
  live.receive(status('ana', 'building'));
  clock.advance(119_999);
  assert.equal(state.statuses.room?.ana?.kind, 'building');
  assert.equal(chatActivityLabel(scope, state, 'room'), 'Ana is creating an app…');
  clock.advance(1);
  assert.deepEqual(state.statuses, {});
  live.receive(status('bia', 'in_call'));
  assert.equal(state.statuses.room?.bia?.kind, 'call');
  assert.equal(chatActivityLabel(scope, state, 'room'), 'Bia is in a call');
  live.receive({ kind: 'presence', identityId: 'bia', online: false });
  assert.deepEqual(state.statuses, {});
  live.receive(status('ana', 'processing'));
  live.receive(status('ana', ''));
  assert.deepEqual(state.statuses, {});
});
