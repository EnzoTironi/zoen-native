import assert from 'node:assert/strict';
import test from 'node:test';
import { PageInputConflict, PageInputEditor } from './page-input.ts';
import type {
  PageInputOpen,
  PageInputPending,
  PageInputPort,
  PageInputStatus,
} from './page-input.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function fixture(pending: Partial<PageInputPending> = {}, supportsWholeDocument = false) {
  let listener: ((status: PageInputStatus) => void) | undefined;
  let current: PageInputStatus = {
    session: 'native-session',
    id: 'native-minted-id',
    token: 'native-token',
    itemId: 'page',
    blockId: 'block',
    mode: 'rich',
    resuming: false,
    revision: '9007199254740994',
    statusVersion: '1',
    sequence: null,
    hasRetainedCapture: false,
    retentionPending: false,
    failed: false,
    sealed: false,
    completed: false,
    operation: null,
    initialMarkdown: 'canonical seed',
  };
  const requests: PageInputOpen[] = [];
  const closes: string[][] = [];
  let reserves = 0;
  let detached = 0;
  const port: PageInputPort = {
    supportsWholeDocument,
    async session() {
      return current.session;
    },
    async load() {
      return {
        revision: '9007199254740993',
        hasRetainedCapture: false,
        blockId: null,
        mode: null,
        ...pending,
      };
    },
    async reserve() {
      reserves++;
      return current.id;
    },
    async open(request) {
      requests.push(request);
      return current;
    },
    async status() {
      return current;
    },
    async seal() {
      current = {
        ...current,
        statusVersion: '2',
        sealed: true,
        failed: false,
        retentionPending: false,
      };
      return current;
    },
    async apply() {
      current = { ...current, statusVersion: '3', completed: true, operation: 'apply' };
      return current;
    },
    async discard() {
      current = { ...current, statusVersion: '3', completed: true, operation: 'discard' };
      return current;
    },
    async close(session, id) {
      closes.push([session, id]);
    },
    subscribe(next) {
      listener = next;
      return () => {
        listener = undefined;
        detached++;
      };
    },
  };
  return {
    port,
    requests,
    closes,
    reserves: () => reserves,
    detached: () => detached,
    set: (value: Partial<PageInputStatus>) => {
      current = { ...current, ...value };
    },
    emit: (value: Partial<PageInputStatus>) => listener?.({ ...current, ...value }),
    current: () => current,
  };
}
const options = {
  itemId: 'page',
  blockId: 'block',
  editContext: 'frozen-context',
  mode: 'rich' as const,
};

test('whole-document opens an explicit native target without sending document text through JS', async () => {
  const f = fixture({}, true);
  const editor = new PageInputEditor(f.port, {
    ...options,
    blockId: null,
    mode: 'markdown',
    target: 'document',
  });
  await editor.open();
  assert.equal(f.requests[0]?.target, 'document');
  assert.equal(f.requests[0]?.blockId, null);
  assert.equal(f.requests[0]?.mode, 'markdown');
  assert.equal(f.requests[0]?.editContext, options.editContext);
  assert.equal('initialMarkdown' in f.requests[0]!, false);
  await editor.close();
});

test('a retained whole-document capture is distinct from append despite the same null block id', async () => {
  const f = fixture(
    { hasRetainedCapture: true, blockId: null, mode: 'markdown', target: 'document' },
    true,
  );
  const append = new PageInputEditor(f.port, { ...options, blockId: null, mode: 'markdown' });
  await assert.rejects(append.open(), PageInputConflict);
  assert.equal(f.reserves(), 0);
  await append.close();
  const recovery = new PageInputEditor(f.port, { ...options, resume: true });
  await recovery.open();
  assert.equal(f.requests[0]?.target, 'document');
  assert.equal(f.requests[0]?.blockId, null);
  assert.equal(f.requests[0]?.mode, 'markdown');
  assert.equal(f.requests[0]?.resume, true);
  await recovery.close();
});

test('switching the retained mode on the same block requires explicit recovery', async () => {
  const f = fixture({ hasRetainedCapture: true, blockId: 'block', mode: 'markdown' });
  const editor = new PageInputEditor(f.port, options);
  await assert.rejects(editor.open(), PageInputConflict);
  assert.equal(f.reserves(), 0);
  await editor.close();
});

test('legacy native ports cannot accidentally turn a whole-document request into AppendNew', async () => {
  const f = fixture();
  const editor = new PageInputEditor(f.port, {
    ...options,
    blockId: null,
    mode: 'markdown',
    target: 'document',
  });
  await assert.rejects(editor.open(), /Update the app/);
  assert.equal(f.reserves(), 0);
  assert.equal(f.requests.length, 0);
  await editor.close();
});

test('invalid explicit target shapes never reserve a native writer', async () => {
  for (const override of [
    { target: 'document' as const, blockId: 'block', mode: 'markdown' as const },
    { target: 'document' as const, blockId: null, mode: 'rich' as const },
    { target: 'block' as const, blockId: null },
    { target: 'append' as const, blockId: 'block' },
  ]) {
    const f = fixture({}, true);
    const editor = new PageInputEditor(f.port, { ...options, ...override });
    await assert.rejects(editor.open(), /incompatible/);
    assert.equal(f.reserves(), 0);
    await editor.close();
  }
});

test('recovery uses the retained target/mode and exact u64 revision; another target needs explicit recovery', async () => {
  const f = fixture({ hasRetainedCapture: true, blockId: 'other', mode: 'lossless-text' });
  const conflict = new PageInputEditor(f.port, options);
  await assert.rejects(conflict.open(), PageInputConflict);
  assert.equal(f.reserves(), 0, 'must not take over the retained native writer');
  await conflict.close();
  const recovery = new PageInputEditor(f.port, { ...options, resume: true });
  await recovery.open();
  assert.deepEqual(f.requests, [
    {
      session: 'native-session',
      id: 'native-minted-id',
      itemId: 'page',
      blockId: 'other',
      resume: true,
      revision: '9007199254740993',
      editContext: 'frozen-context',
      mode: 'lossless-text',
    },
  ]);
  await recovery.close();
});

test('an ambiguous open retries the exact native reservation/request rather than minting a writer in JS', async () => {
  const f = fixture();
  const realOpen = f.port.open;
  let fail = true;
  f.port.open = async (request) => {
    const status = await realOpen(request);
    if (fail) {
      fail = false;
      throw new Error('lost open response');
    }
    return status;
  };
  const editor = new PageInputEditor(f.port, options);
  await assert.rejects(editor.open(), /lost open response/);
  await editor.open();
  assert.equal(f.reserves(), 1);
  assert.equal(f.requests.length, 2);
  assert.strictEqual(f.requests[0], f.requests[1]);
  assert.deepEqual(Object.keys(f.requests[0]!).sort(), [
    'blockId',
    'editContext',
    'id',
    'itemId',
    'mode',
    'resume',
    'revision',
    'session',
  ]);
  await editor.close();
});

test('leave closes the known native handle while the open response is held; late response cannot remount it', async () => {
  const f = fixture();
  const entered = deferred<void>(),
    held = deferred<PageInputStatus>();
  f.port.open = async (request) => {
    f.requests.push(request);
    entered.resolve();
    return held.promise;
  };
  const editor = new PageInputEditor(f.port, options);
  const opening = assert.rejects(editor.open(), /closing/);
  await entered.promise;
  await editor.close();
  assert.deepEqual(f.closes, [['native-session', 'native-minted-id']]);
  assert.equal(editor.getSnapshot(), null);
  held.resolve(f.current());
  await opening;
  assert.equal(editor.getSnapshot(), null);
  assert.equal(f.detached(), 1);
});

test('leave during native reservation never queues a later open', async () => {
  const f = fixture();
  const entered = deferred<void>(),
    held = deferred<string>();
  f.port.reserve = async () => {
    entered.resolve();
    return held.promise;
  };
  const editor = new PageInputEditor(f.port, options);
  const opening = assert.rejects(editor.open(), /closing/);
  await entered.promise;
  const closing = editor.close();
  held.resolve('native-minted-id');
  await closing;
  await opening;
  assert.equal(f.requests.length, 0);
  assert.deepEqual(f.closes, [['native-session', 'native-minted-id']]);
});

test('a failed preserve keeps its native owner and subscription, and retry closes the same handle', async () => {
  const f = fixture();
  let failures = 1;
  f.port.close = async (session, id) => {
    f.closes.push([session, id]);
    if (failures--) throw new Error('checkpoint not retained');
  };
  const editor = new PageInputEditor(f.port, options);
  await editor.open();
  await assert.rejects(editor.close(), /not retained/);
  assert.equal(f.detached(), 0);
  f.emit({ statusVersion: '2', failed: true, retentionPending: true });
  assert.equal(editor.getSnapshot()?.retentionPending, true);
  await editor.close();
  assert.deepEqual(f.closes, [
    ['native-session', 'native-minted-id'],
    ['native-session', 'native-minted-id'],
  ]);
  assert.equal(f.reserves(), 1);
  assert.equal(f.detached(), 1);
});

test('lost Apply response keeps the native pending operation; Discard cannot replace it and exact retry succeeds', async () => {
  const f = fixture();
  const entered = deferred<void>(),
    held = deferred<PageInputStatus>();
  let calls = 0;
  f.port.apply = async () => {
    calls++;
    if (calls === 1) {
      entered.resolve();
      return held.promise;
    }
    return { ...f.current(), statusVersion: '3', completed: true, operation: 'apply' };
  };
  const editor = new PageInputEditor(f.port, options);
  await editor.open();
  const applying = editor.apply();
  assert.strictEqual(editor.apply(), applying);
  await entered.promise;
  await assert.rejects(editor.discard(), /pending editor operation/);
  f.set({ statusVersion: '2', operation: 'apply', sealed: true });
  const rejected = assert.rejects(applying, /lost Apply/);
  held.reject(new Error('lost Apply response'));
  await rejected;
  await assert.rejects(editor.discard(), /pending editor operation/);
  assert.equal((await editor.apply()).completed, true);
  assert.equal(calls, 2);
  await editor.close();
});

test('metadata ignores old-owner and out-of-order notifications while preserving only the canonical seed', async () => {
  const f = fixture();
  const editor = new PageInputEditor(f.port, options);
  await editor.open();
  f.emit({ statusVersion: '9007199254740995', failed: true, retentionPending: true });
  f.emit({ statusVersion: '9007199254740994', failed: false, retentionPending: false });
  f.emit({ session: 'replacement-owner', statusVersion: '9007199254740996', failed: false });
  assert.equal(editor.getSnapshot()?.failed, true);
  assert.equal(editor.getSnapshot()?.initialMarkdown, 'canonical seed');
  editor.retire();
  f.emit({ statusVersion: '9007199254740997', failed: false });
  assert.equal(editor.getSnapshot()?.failed, true);
  assert.equal(f.detached(), 1);
});

test('retrying retention seals the same native producer and never passes text, ranges or receipts through JS', async () => {
  const f = fixture();
  const calls: string[][] = [];
  f.port.seal = async (...identity) => {
    calls.push(identity);
    return { ...f.current(), statusVersion: '2', sealed: true };
  };
  const editor = new PageInputEditor(f.port, options);
  await editor.open();
  await editor.retryRetention();
  assert.deepEqual(calls, [['native-session', 'native-minted-id']]);
  assert.equal(editor.getSnapshot()?.sealed, true);
  await editor.close();
});
