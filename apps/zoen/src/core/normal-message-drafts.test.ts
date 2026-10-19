import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { TimelineEntry } from './types';
import {
  NormalMessageDrafts,
  NormalDraftFailure,
  type NormalDraftPort,
  type NormalDraftScope,
  type NormalDraftSnapshot,
} from './normal-message-drafts.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const main: NormalDraftScope = {
  owner: { identityId: 'qa-owner', deviceId: 'qa-device', sessionEpoch: 1, mode: 'account' },
  spaceId: 'qa-chat',
  threadRootId: null,
};
const key = (scope: NormalDraftScope) => JSON.stringify(scope);
const kind = (expected: NormalDraftFailure['kind']) => (cause: unknown) =>
  cause instanceof NormalDraftFailure && cause.kind === expected;
function fixture() {
  const stored = new Map<string, NormalDraftSnapshot>();
  const receipts = new Map<string, { revision: string; entry: TimelineEntry }>();
  const calls: { op: 'load' | 'save' | 'send'; scope: NormalDraftScope; revision?: string }[] = [];
  const posts: { text: string; replyTo: string | null; scope: NormalDraftScope }[] = [];
  let revision = 0;
  const hooks: {
    load?: () => Promise<void>;
    save?: (text: string) => Promise<void>;
    saved?: () => Promise<void>;
    send?: () => Promise<void>;
    sent?: () => Promise<void>;
  } = {};
  const current = (scope: NormalDraftScope) =>
    stored.get(key(scope)) ?? { revision: '0', text: '', reply_to: null };
  const port: NormalDraftPort = {
    async load(scope) {
      calls.push({ op: 'load', scope });
      await hooks.load?.();
      return { ...current(scope) };
    },
    async save(scope, expected, text, replyTo) {
      calls.push({ op: 'save', scope, revision: expected });
      await hooks.save?.(text);
      assert.equal(expected, current(scope).revision, 'the port enforces the actual CAS');
      const value = { revision: `opaque-${++revision}`, text, reply_to: replyTo };
      stored.set(key(scope), value);
      await hooks.saved?.();
      return { ...value };
    },
    async send(scope, expected) {
      calls.push({ op: 'send', scope, revision: expected });
      await hooks.send?.();
      const receipt = receipts.get(key(scope));
      if (receipt?.revision === expected)
        return { entry: receipt.entry, draft: { ...current(scope) } };
      assert.equal(expected, current(scope).revision, 'send consumes the exact stored intent');
      const value = current(scope);
      posts.push({ text: value.text, replyTo: value.reply_to, scope });
      const entry = { id: `qa-message-${posts.length}` } as TimelineEntry;
      stored.set(key(scope), { revision: `opaque-${++revision}`, text: '', reply_to: null });
      receipts.set(key(scope), { revision: expected, entry });
      await hooks.sent?.();
      return { entry, draft: { ...current(scope) } };
    },
  };
  return { drafts: new NormalMessageDrafts(port), port, hooks, stored, current, calls, posts };
}

test('cold restore gates input, caches a loaded scope, and keeps opaque revisions and immutable scope', async () => {
  const f = fixture();
  const gate = deferred<void>();
  f.hooks.load = () => gate.promise;
  f.stored.set(key(main), {
    revision: 'native_token-A',
    text: 'Saved 🌤️\ntext',
    reply_to: 'quote',
  });
  const scope = { ...main, owner: { ...main.owner } };
  const loading = f.drafts.activate(scope);
  assert.equal(f.drafts.getSnapshot(scope).loading, true);
  assert.equal(f.drafts.setText(scope, 'Too early'), false);
  assert.equal(f.drafts.setReply(scope, 'Too early'), false);
  scope.owner.identityId = 'changed outside';
  gate.resolve();
  await loading;
  const state = f.drafts.getSnapshot(main);
  assert.equal(state.text, 'Saved 🌤️\ntext');
  assert.equal(state.replyTo, 'quote');
  assert.equal(state.persistedRevision, 'native_token-A');
  assert.equal(state.scope.owner.identityId, main.owner.identityId);
  assert.ok(Object.isFrozen(state.scope));
  assert.ok(Object.isFrozen(state.scope.owner));
  f.drafts.setText(main, 'New local text');
  await f.drafts.activate(main);
  await f.drafts.flush(main);
  assert.equal(f.drafts.getSnapshot(main).text, 'New local text');
  assert.equal(f.calls.filter((call) => call.op === 'load').length, 1);
});

test('account, device, session, demo, chat and two thread scopes cannot share drafts', async () => {
  const f = fixture();
  const scopes: NormalDraftScope[] = [
    main,
    { ...main, owner: { ...main.owner, identityId: 'another' } },
    { ...main, owner: { ...main.owner, deviceId: 'another' } },
    { ...main, owner: { ...main.owner, sessionEpoch: 2 } },
    { ...main, owner: { ...main.owner, mode: 'demo' } },
    { ...main, spaceId: 'another' },
    { ...main, threadRootId: 'thread-A' },
    { ...main, threadRootId: 'thread-B' },
  ];
  for (const [index, scope] of scopes.entries()) {
    await f.drafts.activate(scope);
    f.drafts.setText(scope, `Draft ${index}`);
    await f.drafts.flush(scope);
  }
  for (const [index, scope] of scopes.entries())
    assert.equal(f.drafts.getSnapshot(scope).text, `Draft ${index}`);
  assert.equal(f.stored.size, scopes.length);
});

test('same-turn typing coalesces and an admitted old save never overwrites newer text', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  const entered = deferred<void>();
  const gate = deferred<void>();
  let saves = 0;
  f.hooks.save = async () => {
    if (++saves !== 1) return;
    entered.resolve();
    await gate.promise;
  };
  f.drafts.setText(main, 'A');
  f.drafts.setText(main, 'AB');
  await entered.promise;
  f.drafts.setText(main, 'ABC');
  assert.equal(f.drafts.getSnapshot(main).text, 'ABC');
  gate.resolve();
  await f.drafts.flush(main);
  assert.equal(f.current(main).text, 'ABC');
  assert.equal(f.drafts.getSnapshot(main).text, 'ABC');
  assert.equal(saves, 2);
});

test('send saves its exact captured intent before later typing can advance the port', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Send this');
  f.drafts.setReply(main, 'quote-A');
  const sending = f.drafts.send(main);
  f.drafts.setText(main, 'Keep this later');
  f.drafts.setReply(main, 'quote-B');
  await sending;
  await f.drafts.flush(main);
  assert.deepEqual(
    f.posts.map(({ text, replyTo }) => ({ text, replyTo })),
    [{ text: 'Send this', replyTo: 'quote-A' }],
  );
  assert.equal(f.current(main).text, 'Keep this later');
  assert.equal(f.current(main).reply_to, 'quote-B');
  assert.deepEqual(
    f.calls.map((call) => call.op),
    ['load', 'save', 'send', 'save'],
  );
});

test('late send completion preserves text ABA and a newly selected identical quote', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Same text');
  f.drafts.setReply(main, 'quote');
  await f.drafts.flush(main);
  const entered = deferred<void>();
  const gate = deferred<void>();
  f.hooks.send = async () => {
    entered.resolve();
    await gate.promise;
  };
  const sending = f.drafts.send(main);
  await entered.promise;
  f.drafts.setText(main, '');
  f.drafts.setText(main, 'Same text');
  f.drafts.setReply(main, 'quote');
  gate.resolve();
  await sending;
  await f.drafts.flush(main);
  assert.equal(f.posts.length, 1);
  assert.equal(f.drafts.getSnapshot(main).text, 'Same text');
  assert.equal(f.drafts.getSnapshot(main).replyTo, 'quote');
  assert.equal(f.current(main).text, 'Same text');
});

test('send clears text and reply independently, preserving only the part changed since capture', async () => {
  for (const changed of ['text', 'reply'] as const) {
    const f = fixture();
    await f.drafts.activate(main);
    f.drafts.setText(main, 'Original');
    f.drafts.setReply(main, 'original-quote');
    const entered = deferred<void>();
    const gate = deferred<void>();
    f.hooks.send = async () => {
      entered.resolve();
      await gate.promise;
    };
    const sending = f.drafts.send(main);
    await entered.promise;
    if (changed === 'text') f.drafts.setText(main, 'New text');
    else f.drafts.setReply(main, 'new-quote');
    gate.resolve();
    await sending;
    await f.drafts.flush(main);
    assert.equal(f.current(main).text, changed === 'text' ? 'New text' : '');
    assert.equal(f.current(main).reply_to, changed === 'reply' ? 'new-quote' : null);
  }
});

test('lost send response freezes its revision, retries one receipt, then saves newer typing', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Accepted once');
  let first = true;
  f.hooks.sent = async () => {
    if (first) {
      first = false;
      throw new Error('Synthetic lost response after commit');
    }
  };
  await assert.rejects(f.drafts.send(main), kind('send'));
  const revision = f.calls.find((call) => call.op === 'send')!.revision;
  f.drafts.setText(main, 'New local draft');
  f.drafts.setReply(main, 'new-quote');
  assert.equal(f.current(main).text, '');
  assert.equal(f.drafts.getSnapshot(main).text, 'New local draft');
  await f.drafts.retry(main);
  assert.equal(f.posts.length, 1);
  assert.deepEqual(
    f.calls.filter((call) => call.op === 'send').map((call) => call.revision),
    [revision, revision],
  );
  assert.equal(f.current(main).text, 'New local draft');
  assert.equal(f.current(main).reply_to, 'new-quote');
});

test('duplicate send taps share the admitted request rather than posting twice', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Once');
  const entered = deferred<void>();
  const gate = deferred<void>();
  f.hooks.send = async () => {
    entered.resolve();
    await gate.promise;
  };
  const first = f.drafts.send(main);
  await entered.promise;
  const second = f.drafts.send(main);
  assert.equal(first, second);
  gate.resolve();
  assert.equal((await first).id, (await second).id);
  assert.equal(f.posts.length, 1);
  assert.equal(f.drafts.getSnapshot(main).text, '');
});

test('a rejected prepare save permits a corrected new intent without ever sending the invalid one', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.hooks.save = async (text) => {
    if (text.length > 10) throw new Error('Synthetic text limit');
  };
  f.drafts.setText(main, 'Too long to save');
  f.drafts.setReply(main, 'wrong-quote');
  await assert.rejects(f.drafts.send(main), kind('save'));
  assert.equal(f.calls.filter((call) => call.op === 'send').length, 0);
  f.drafts.setText(main, 'Correct');
  f.drafts.setReply(main, null);
  await f.drafts.send(main);
  assert.deepEqual(
    f.posts.map(({ text, replyTo }) => ({ text, replyTo })),
    [{ text: 'Correct', replyTo: null }],
  );
});

test('an explicitly definitive first send rejection allows cancelling an invalid quote', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Keep text');
  f.drafts.setReply(main, 'deleted-quote');
  f.hooks.send = async () => {
    f.hooks.send = undefined;
    throw Object.assign(new Error('Synthetic canonical target missing'), {
      code: 'ERR_ZOEN_DRAFT_REJECTED',
    });
  };
  await assert.rejects(f.drafts.send(main), kind('send'));
  assert.equal(f.posts.length, 0);
  f.drafts.setReply(main, null);
  await f.drafts.send(main);
  assert.equal(f.posts.length, 1);
  assert.equal(f.posts[0].text, 'Keep text');
  assert.equal(f.posts[0].replyTo, null);
});

test('a generic rejection after an ambiguous commit never unlocks a different send', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Original accepted intent');
  f.hooks.sent = async () => {
    f.hooks.sent = undefined;
    throw new Error('Synthetic response lost after commit');
  };
  await assert.rejects(f.drafts.send(main), kind('send'));
  const revision = f.calls.find((call) => call.op === 'send')!.revision;
  f.drafts.setText(main, 'New text is not a retry');
  f.hooks.send = async () => {
    throw Object.assign(new Error('Synthetic access changed during retry'), {
      code: 'ERR_FORBIDDEN',
    });
  };
  await assert.rejects(f.drafts.send(main), kind('send'));
  await assert.rejects(f.drafts.send(main), kind('send'));
  assert.equal(f.posts.length, 1);
  assert.deepEqual(
    f.calls.filter((call) => call.op === 'send').map((call) => call.revision),
    [revision, revision, revision],
  );
  assert.equal(f.current(main).text, '');
  assert.equal(f.drafts.getSnapshot(main).text, 'New text is not a retry');
  f.hooks.send = undefined;
  await f.drafts.retry(main);
  assert.equal(f.current(main).text, 'New text is not a retry');
  assert.equal(f.posts.length, 1);
});

test('exclusive receipt-absence proof after an unknown failure permits a corrected new send', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Original draft');
  f.drafts.setReply(main, 'deleted-quote');
  f.hooks.send = async () => {
    throw new Error('Synthetic unknown precommit failure');
  };
  await assert.rejects(f.drafts.send(main), kind('send'));
  const oldRevision = f.current(main).revision;
  f.drafts.setText(main, 'Corrected draft');
  f.drafts.setReply(main, null);
  f.hooks.send = async () => {
    assert.equal(f.posts.length, 0);
    assert.equal(f.current(main).revision, oldRevision);
    throw Object.assign(new Error('Synthetic native proof: never accepted, target deleted'), {
      code: 'ERR_ZOEN_DRAFT_REJECTED',
    });
  };
  await assert.rejects(f.drafts.send(main), kind('send'));
  f.hooks.send = undefined;
  await f.drafts.send(main);
  assert.deepEqual(
    f.posts.map(({ text, replyTo }) => ({ text, replyTo })),
    [{ text: 'Corrected draft', replyTo: null }],
  );
});

test('failed load cannot autosave over ciphertext, and explicit retry restores the same draft', async () => {
  const f = fixture();
  f.stored.set(key(main), { revision: 'saved-token', text: 'Recover this', reply_to: 'quote' });
  f.hooks.load = async () => {
    throw new Error('Synthetic encrypted read failure');
  };
  await assert.rejects(f.drafts.activate(main), kind('load'));
  assert.equal(f.drafts.setText(main, 'Overwrite'), false);
  assert.equal(f.drafts.getSnapshot(main).loaded, false);
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 0);
  f.hooks.load = undefined;
  await f.drafts.retry(main);
  assert.equal(f.drafts.getSnapshot(main).text, 'Recover this');
  assert.equal(f.drafts.getSnapshot(main).replyTo, 'quote');
});

test('lost save response reconciles its exact accepted payload before saving newer local text', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.hooks.saved = async () => {
    f.hooks.saved = undefined;
    throw new Error('Synthetic save response lost');
  };
  const failed = deferred<void>();
  const unsubscribe = f.drafts.subscribe(() => {
    if (f.drafts.getSnapshot(main).error?.kind === 'save') failed.resolve();
  });
  f.drafts.setText(main, 'First payload');
  await failed.promise;
  unsubscribe();
  f.drafts.setText(main, 'Newer payload');
  await f.drafts.retry(main);
  assert.equal(f.current(main).text, 'Newer payload');
  assert.equal(f.drafts.getSnapshot(main).text, 'Newer payload');
  assert.equal(f.drafts.getSnapshot(main).error, null);
});

test('a foreign CAS conflict is surfaced without overwriting either local or remote content', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  const entered = deferred<void>();
  f.hooks.save = async () => {
    entered.resolve();
    throw new Error('Synthetic save unavailable');
  };
  f.drafts.setText(main, 'Retain local');
  await entered.promise;
  await assert.rejects(f.drafts.flush(main), kind('save'));
  f.hooks.save = undefined;
  f.stored.set(key(main), { revision: 'foreign', text: 'Other writer', reply_to: null });
  const saves = f.calls.filter((call) => call.op === 'save').length;
  await assert.rejects(f.drafts.retry(main), kind('conflict'));
  assert.equal(f.current(main).text, 'Other writer');
  assert.equal(f.drafts.getSnapshot(main).text, 'Retain local');
  assert.equal(f.calls.filter((call) => call.op === 'save').length, saves);
});

test('preserving retirement closes admission synchronously, drains latest text, and can cold restore', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Keep on close');
  f.drafts.setReply(main, 'quote');
  const closing = f.drafts.retire({ flush: true });
  assert.equal(f.drafts.setText(main, 'Stale callback'), false);
  await assert.rejects(f.drafts.send(main), kind('closed'));
  await closing;
  assert.equal(f.current(main).text, 'Keep on close');
  const next = new NormalMessageDrafts(f.port);
  await next.activate(main);
  assert.equal(next.getSnapshot(main).text, 'Keep on close');
  assert.equal(next.getSnapshot(main).replyTo, 'quote');
});

test('failed preserving close retains text and permits a later retire retry without reopening setters', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  let attempts = 0;
  f.hooks.save = async () => {
    if (++attempts <= 2) throw new Error('Synthetic disk unavailable');
  };
  f.drafts.setText(main, 'Unsaved on close');
  await assert.rejects(f.drafts.retire({ flush: true }), kind('save'));
  assert.equal(f.drafts.getSnapshot(main).text, 'Unsaved on close');
  assert.equal(f.drafts.setText(main, 'Old callback'), false);
  await f.drafts.retire({ flush: true });
  assert.equal(f.current(main).text, 'Unsaved on close');
  assert.equal(f.drafts.getSnapshot(main).error, null);
});

test('discard escalates failed preserving close and cancels all queued work', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.hooks.save = async () => {
    throw new Error('Synthetic disk unavailable');
  };
  f.drafts.setText(main, 'Erase this');
  await assert.rejects(f.drafts.retire({ flush: true }), kind('save'));
  const calls = f.calls.length;
  await f.drafts.retire({ flush: false });
  assert.equal(f.drafts.getSnapshot(main).text, '');
  assert.equal(f.drafts.getSnapshot(main).loaded, false);
  await f.drafts.retire({ flush: true });
  assert.equal(f.calls.length, calls);
});

test('discard drains an admitted save, cancels another queued scope, and never retains late plaintext', async () => {
  const f = fixture();
  const thread = { ...main, threadRootId: 'thread' };
  await f.drafts.activate(main);
  await f.drafts.activate(thread);
  const entered = deferred<void>();
  const gate = deferred<void>();
  f.hooks.save = async () => {
    entered.resolve();
    await gate.promise;
  };
  f.drafts.setText(main, 'SYNTHETIC SECRET TO ERASE');
  await entered.promise;
  f.drafts.setText(thread, 'QUEUED SECRET TO ERASE');
  let drained = false;
  const erasing = f.drafts.retire({ flush: false }).then(() => {
    drained = true;
  });
  assert.equal(drained, false);
  assert.equal(f.drafts.getSnapshot(main).text, '');
  assert.equal(f.drafts.setText(thread, 'Stale callback'), false);
  gate.resolve();
  await erasing;
  assert.equal(drained, true);
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 1);
  // Public redaction alone would miss private retry buffers recreated by a late
  // save failure. Erasure must also release those controller-owned buffers.
  const records = Reflect.get(f.drafts, 'records') as Map<string, unknown>;
  assert.equal(JSON.stringify([...records.values()]).includes('SECRET TO ERASE'), false);
});
