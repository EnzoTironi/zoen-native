import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MessageContextPresentation } from '../components/message-context-presentation.ts';
import type {
  MessageContextConfiguration,
  MessageContextRequest,
} from '../components/message-context-ports.ts';
const deferred = <T>() => {
  let resolve!: (value: T) => void, reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const tick = () => Promise.resolve();
const config: MessageContextConfiguration = {
  actions: [{ id: 'reply', label: 'Reply' }],
  reactions: [{ emoji: '❤️', label: 'Like', selected: false }],
  moreLabel: 'More',
  theme: {
    background: '#FFFFFF',
    text: '#000000',
    secondary: '#AAAAAA',
    separator: '#AAAAAA',
    destructive: '#FF0000',
    highlight: '#AAAAAA',
    dim: '#00000080',
    isDark: false,
  },
  reducedMotion: false,
  incoming: true,
};
function fixture() {
  const result = deferred<string | null>(),
    removal = deferred<void>();
  const selections: string[] = [],
    cancelled: string[] = [],
    presentations: unknown[][] = [];
  let current = true,
    row = true;
  const request: MessageContextRequest = {
    binding: 'row-binding',
    gestureId: 'gesture',
    kind: 'hold',
    current: () => row,
    native: {
      consumeGesture: async () => true,
      presentContext(...args) {
        presentations.push(args);
        return result.promise;
      },
      dismissContext(id) {
        cancelled.push(id);
        return removal.promise;
      },
    },
  };
  const owner = new MessageContextPresentation({
    request,
    current: () => current,
    id: () => 'exact-request',
    selected: (value) => {
      selections.push(value);
    },
  });
  return {
    owner,
    request,
    result,
    removal,
    selections,
    presentations,
    cancelled,
    revoke: () => {
      current = false;
    },
    recycle: () => {
      row = false;
    },
  };
}

test('the actual native removal promise precedes one authorized effect; overlapping open is refused', async () => {
  const f = fixture();
  const opening = f.owner.open(config);
  await tick();
  await f.owner.open(config);
  assert.equal(f.presentations.length, 1);
  assert.deepEqual(f.selections, []);
  f.result.resolve('reply');
  await opening;
  assert.deepEqual(f.selections, ['reply']);
});
for (const boundary of ['authority', 'row', 'close', 'retire'] as const)
  test(`queued native completion cannot cross ${boundary}`, async () => {
    const f = fixture(),
      opening = f.owner.open(config);
    await tick();
    if (boundary === 'authority') f.revoke();
    else if (boundary === 'row') f.recycle();
    else if (boundary === 'close') void f.owner.close();
    else void f.owner.retire();
    f.result.resolve('reply');
    f.removal.resolve();
    await opening;
    assert.deepEqual(f.selections, []);
    if (boundary === 'close' || boundary === 'retire')
      assert.deepEqual(f.cancelled, ['exact-request']);
  });
for (const unoffered of ['delete', 'reaction:🔥', 'reaction:', 'reply\n', 'unknown'])
  test(`native result ${JSON.stringify(unoffered)} cannot invent a callback`, async () => {
    const f = fixture(),
      opening = f.owner.open(config);
    await tick();
    f.result.resolve(unoffered);
    await opening;
    assert.deepEqual(f.selections, []);
  });
for (const choice of ['reaction:❤️', 'more'])
  test(`offered ${choice} is retained exactly`, async () => {
    const f = fixture(),
      opening = f.owner.open(config);
    await tick();
    f.result.resolve(choice);
    await opening;
    assert.deepEqual(f.selections, [choice]);
  });
test('retirement fences immediately even while native dismissal is still pending', async () => {
  const f = fixture(),
    opening = f.owner.open(config);
  await tick();
  let retired = false;
  const retirement = f.owner.retire().then(() => {
    retired = true;
  });
  assert.deepEqual(f.cancelled, ['exact-request']);
  await tick();
  assert.equal(retired, false);
  f.result.resolve('reply');
  await opening;
  assert.deepEqual(f.selections, []);
  f.removal.resolve();
  await retirement;
  assert.equal(retired, true);
});
test('open rejection drains the exact native presentation before safe fallback may proceed', async () => {
  const f = fixture();
  const opening = f.owner.open(config);
  const rejected = assert.rejects(opening, /native failed/);
  await tick();
  f.result.reject(new Error('native failed'));
  await tick();
  await tick();
  assert.deepEqual(f.cancelled, ['exact-request']);
  let done = false;
  void opening.catch(() => {
    done = true;
  });
  await tick();
  assert.equal(done, false);
  f.removal.resolve();
  await rejected;
  assert.equal(done, true);
});
test('configuration is captured before asynchronous admission, including offered callback set', async () => {
  const f = fixture();
  const mutable = structuredClone(config) as any;
  const opening = f.owner.open(mutable);
  mutable.actions.push({ id: 'delete', label: 'Delete' });
  await tick();
  assert.equal(JSON.parse(f.presentations[0]![3] as string).actions.length, 1);
  f.result.resolve('delete');
  await opening;
  assert.deepEqual(f.selections, []);
});
test('revocation before admission performs no native operation', async () => {
  const f = fixture();
  f.revoke();
  await f.owner.open(config);
  assert.deepEqual(f.presentations, []);
});
