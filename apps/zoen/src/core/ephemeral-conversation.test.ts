import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EphemeralConversation } from '../assistant/conversation.ts';
import type {
  AssistantOpen,
  AssistantPort,
  AssistantUnavailableReason,
} from '../assistant/types.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const available = (id = 'volatile-lease'): AssistantOpen => ({
  available: true,
  id,
  contextMessages: 3,
  limited: true,
});

function fixture() {
  const opened: { spaceId: string; locale: string }[] = [];
  const asked: { id: string; question: string }[] = [];
  const closed: string[] = [];
  const port: AssistantPort = {
    async open(spaceId, locale) {
      opened.push({ spaceId, locale });
      return available();
    },
    async ask(id, question) {
      asked.push({ id, question });
      return { text: 'Actual port response' };
    },
    async close(id) {
      closed.push(id);
    },
  };
  const conversation = new EphemeralConversation(port);
  return { port, conversation, opened, asked, closed };
}

test('opening supplies only the exact context scope and locale, and does not implicitly ask', async () => {
  const f = fixture();
  await f.conversation.open('canonical-space', 'pt-BR');
  assert.deepEqual(f.opened, [{ spaceId: 'canonical-space', locale: 'pt-BR' }]);
  assert.deepEqual(f.asked, []);
  assert.deepEqual(f.conversation.getSnapshot(), {
    phase: 'ready',
    turns: [],
    pending: null,
    failed: false,
    contextMessages: 3,
    limited: true,
  });
  await f.conversation.close();
});

test('closing during open owns the late native lease and never publishes its availability', async () => {
  const f = fixture();
  const opening = deferred<AssistantOpen>();
  const closeStarted = deferred<string>();
  const drain = deferred<void>();
  f.port.open = async () => opening.promise;
  f.port.close = async (id) => {
    f.closed.push(id);
    closeStarted.resolve(id);
    await drain.promise;
  };
  const phases: string[] = [];
  f.conversation.subscribe(() => phases.push(f.conversation.getSnapshot().phase));
  const open = f.conversation.open('canonical-space', 'en');
  const close = f.conversation.close();
  assert.strictEqual(f.conversation.close(), close);
  assert.equal(f.conversation.getSnapshot().phase, 'closed');
  const publishedAtClose = phases.length;
  opening.resolve(available('late-lease'));
  await open;
  assert.equal(await closeStarted.promise, 'late-lease');
  assert.equal(phases.length, publishedAtClose);
  assert.deepEqual(f.closed, ['late-lease']);
  assert.equal(await f.conversation.ask('Never submit this'), false);
  drain.resolve();
  await close;
  assert.deepEqual(f.asked, []);
  assert.deepEqual(f.conversation.getSnapshot().turns, []);
});

test('closing an active answer waits for native cancellation/drain and discards the late answer', async () => {
  const f = fixture();
  await f.conversation.open('canonical-space', 'pt-BR');
  const answer = deferred<{ text: string }>();
  const closeStarted = deferred<void>();
  const nativeDrain = deferred<void>();
  f.port.ask = async (id, question) => {
    f.asked.push({ id, question });
    return answer.promise;
  };
  f.port.close = async (id) => {
    f.closed.push(id);
    closeStarted.resolve();
    await nativeDrain.promise;
  };
  const ask = f.conversation.ask('Question still generating');
  assert.equal(f.conversation.getSnapshot().pending, 'Question still generating');
  let finished = false;
  const close = f.conversation.close().then(() => {
    finished = true;
  });
  await closeStarted.promise;
  const closedState = f.conversation.getSnapshot();
  answer.resolve({ text: 'Late private answer' });
  assert.equal(await ask, false);
  assert.equal(finished, false);
  assert.strictEqual(f.conversation.getSnapshot(), closedState);
  assert.deepEqual(closedState.turns, []);
  assert.equal(closedState.pending, null);
  nativeDrain.resolve();
  await close;
  assert.equal(finished, true);
  assert.deepEqual(f.closed, ['volatile-lease']);
});

test('a native close acknowledgement alone cannot finish before the owned ask settles', async () => {
  const f = fixture();
  await f.conversation.open('canonical-space', 'en');
  const answer = deferred<{ text: string }>();
  const closeStarted = deferred<void>();
  f.port.ask = async () => answer.promise;
  f.port.close = async () => closeStarted.resolve();
  const ask = f.conversation.ask('Pending');
  let finished = false;
  const close = f.conversation.close().then(() => {
    finished = true;
  });
  await closeStarted.promise;
  assert.equal(finished, false);
  answer.reject(new Error('Native generation cancelled'));
  assert.equal(await ask, false);
  await close;
  assert.equal(finished, true);
  assert.equal(f.conversation.getSnapshot().phase, 'closed');
});

test('a failed close retains the exact lease for retry while refusing all further work', async () => {
  const f = fixture();
  await f.conversation.open('canonical-space', 'en');
  let attempts = 0;
  f.port.close = async (id) => {
    f.closed.push(id);
    if (++attempts === 1) throw new Error('Native cleanup pending');
  };
  await assert.rejects(f.conversation.close(), /cleanup pending/);
  assert.equal(await f.conversation.ask('Do not retry generation'), false);
  await f.conversation.open('other-space', 'pt-BR');
  assert.equal(f.opened.length, 1);
  await f.conversation.close();
  await f.conversation.close();
  assert.deepEqual(f.closed, ['volatile-lease', 'volatile-lease']);
  assert.deepEqual(f.asked, []);
  assert.deepEqual(f.conversation.getSnapshot().turns, []);
});

test('failed asks preserve the accepted history and let the caller retry its unchanged draft', async () => {
  const f = fixture();
  await f.conversation.open('canonical-space', 'en');
  assert.equal(await f.conversation.ask('First question'), true);
  const history = f.conversation.getSnapshot().turns;
  const draft = '  Retry this exact question  ';
  let attempt = 0;
  f.port.ask = async (id, question) => {
    f.asked.push({ id, question });
    if (++attempt === 1) throw new Error('Model failed');
    return { text: 'Actual retry response' };
  };
  assert.equal(await f.conversation.ask(draft), false);
  assert.strictEqual(f.conversation.getSnapshot().turns, history);
  assert.equal(f.conversation.getSnapshot().phase, 'ready');
  assert.equal(f.conversation.getSnapshot().failed, true);
  assert.equal(f.conversation.getSnapshot().pending, null);
  assert.equal(draft, '  Retry this exact question  ');
  assert.equal(await f.conversation.ask(draft), true);
  assert.deepEqual(f.conversation.getSnapshot().turns, [
    { role: 'user', text: 'First question' },
    { role: 'assistant', text: 'Actual port response' },
    { role: 'user', text: 'Retry this exact question' },
    { role: 'assistant', text: 'Actual retry response' },
  ]);
  assert.equal(f.conversation.getSnapshot().failed, false);
  await f.conversation.close();
});

test('an empty model result is a failure and never fabricates a user/assistant pair', async () => {
  const f = fixture();
  await f.conversation.open('canonical-space', 'en');
  f.port.ask = async () => ({ text: ' \n ' });
  assert.equal(await f.conversation.ask('Real question'), false);
  assert.deepEqual(f.conversation.getSnapshot().turns, []);
  assert.equal(f.conversation.getSnapshot().failed, true);
  await f.conversation.close();
});

test('all unavailable reasons remain truthful and admit neither asks nor a fabricated lease', async () => {
  const reasons: AssistantUnavailableReason[] = [
    'not_eligible',
    'disabled',
    'not_ready',
    'unsupported_locale',
    'unsupported',
  ];
  for (const reason of reasons) {
    const f = fixture();
    f.port.open = async () => ({ available: false, reason });
    await f.conversation.open('canonical-space', 'pt-BR');
    assert.equal(f.conversation.getSnapshot().phase, 'unavailable');
    assert.equal(f.conversation.getSnapshot().reason, reason);
    assert.equal(await f.conversation.ask('Must not execute'), false);
    await f.conversation.close();
    assert.deepEqual(f.asked, []);
    assert.deepEqual(f.closed, []);
  }
});

test('concurrent asks admit one native generation and retain only the accepted question', async () => {
  const f = fixture();
  await f.conversation.open('canonical-space', 'en');
  const answer = deferred<{ text: string }>();
  f.port.ask = async (id, question) => {
    f.asked.push({ id, question });
    return answer.promise;
  };
  const first = f.conversation.ask('Accepted question');
  assert.equal(await f.conversation.ask('Duplicate question'), false);
  assert.deepEqual(f.asked, [{ id: 'volatile-lease', question: 'Accepted question' }]);
  assert.equal(f.conversation.getSnapshot().pending, 'Accepted question');
  answer.resolve({ text: 'One actual answer' });
  assert.equal(await first, true);
  assert.deepEqual(f.conversation.getSnapshot().turns, [
    { role: 'user', text: 'Accepted question' },
    { role: 'assistant', text: 'One actual answer' },
  ]);
  await f.conversation.close();
});
