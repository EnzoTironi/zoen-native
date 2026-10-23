import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, fixture, kind, main } from './normal-message-drafts.fixtures.ts';

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
