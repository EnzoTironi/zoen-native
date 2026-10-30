import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  NormalMessageDrafts,
  type NormalDraftPort,
  type NormalDraftSnapshot,
} from './normal-message-drafts.ts';
import { deferred, main, kind } from './normal-message-drafts.fixtures.ts';
import { messageDraftReactFixture } from './message-draft-react.fixtures.ts';
import type { MessageDraftEdit } from './generated/MessageDraftEdit';
import type { TimelineEntry } from './types';

const edit: MessageDraftEdit = {
  message_id: 'source',
  source_revision: 'signed-r1',
  base_text: 'Original',
  text: 'Correction 😀',
  is_caption: false,
};
function fixture() {
  let value: NormalDraftSnapshot = { revision: '0', text: '', reply_to: null };
  let projection: TimelineEntry | null | undefined;
  let serial = 0,
    mutations = 0;
  let receipt: { revision: string; entry: TimelineEntry } | null = null;
  const calls: string[] = [];
  const hooks: {
    before?: () => Promise<void>;
    after?: () => Promise<void>;
    saved?: () => Promise<void>;
  } = {};
  const copy = () => structuredClone(value);
  const port: NormalDraftPort = {
    load: async () => copy(),
    save: async (_scope, expected, text, reply_to, reply_quote, shared_reply, edit) => {
      assert.equal(expected, value.revision);
      calls.push(`save:${expected}`);
      value = {
        revision: String(++serial),
        text,
        reply_to,
        ...(edit ? { edit } : {}),
        ...(reply_quote ? { reply_quote } : {}),
        ...(shared_reply ? { shared_reply } : {}),
      };
      await hooks.saved?.();
      return copy();
    },
    send: async () => assert.fail('the edit path cannot send a normal message'),
    sendEdit: async (_scope, expected) => {
      calls.push(`commit:${expected}`);
      await hooks.before?.();
      if (receipt?.revision !== expected) {
        assert.equal(expected, value.revision);
        assert.ok(value.edit);
        receipt = {
          revision: expected,
          entry: {
            id: value.edit.message_id,
            text_revision: `accepted-r${serial + 1}`,
          } as TimelineEntry,
        };
        ++mutations;
        value = { ...value, revision: String(++serial), edit: undefined };
      }
      await hooks.after?.();
      return {
        message_id: receipt.entry.id,
        text_revision: receipt.entry.text_revision ?? null,
        entry: projection === undefined ? receipt.entry : projection,
        draft: copy(),
      };
    },
  };
  return {
    port,
    drafts: new NormalMessageDrafts(port),
    reopen: () => new NormalMessageDrafts(port),
    hooks,
    calls,
    stored: copy,
    project: (entry: TimelineEntry | null) => {
      projection = entry;
    },
    mutations: () => mutations,
  };
}

test('edit restores through the same controller and normal record without an automatic send', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Independent message');
  f.drafts.setReply(main, 'normal-quote');
  f.drafts.setEdit(main, edit);
  await f.drafts.retire({ flush: true });
  assert.equal(f.drafts.setEditText(main, 'Retired control'), false);
  const next = f.reopen();
  await next.activate(main);
  assert.deepEqual(next.getSnapshot(main).edit, edit);
  assert.equal(next.getSnapshot(main).text, 'Independent message');
  assert.equal(next.getSnapshot(main).replyTo, 'normal-quote');
  assert.equal(f.mutations(), 0);
  await next.sendEdit(main);
  assert.equal(next.getSnapshot(main).edit, null);
  assert.equal(next.getSnapshot(main).text, 'Independent message');
  assert.equal(next.getSnapshot(main).replyTo, 'normal-quote');
  assert.equal(f.mutations(), 1);
});

test('late edit acceptance preserves newer typing, ABA, another editor and the normal draft', async () => {
  for (const context of ['typing', 'ABA', 'new editor', 'cancel'] as const) {
    const f = fixture(),
      entered = deferred<void>(),
      release = deferred<void>();
    await f.drafts.activate(main);
    f.drafts.setText(main, 'Normal');
    f.drafts.setEdit(main, edit);
    f.hooks.before = async () => {
      entered.resolve();
      await release.promise;
    };
    const sent = f.drafts.sendEdit(main);
    await entered.promise;
    f.drafts.setText(main, 'New normal text');
    f.drafts.setReply(main, 'New normal quote');
    if (context === 'new editor')
      f.drafts.setEdit(main, { ...edit, message_id: 'other', text: 'Other correction' });
    if (context === 'typing' || context === 'ABA') {
      f.drafts.setEdit(main, edit);
      f.drafts.setEditText(main, 'New correction');
      if (context === 'ABA') f.drafts.setEditText(main, edit.text);
    }
    release.resolve();
    await sent;
    await f.drafts.flush(main);
    assert.equal(f.drafts.getSnapshot(main).text, 'New normal text');
    assert.equal(f.drafts.getSnapshot(main).replyTo, 'New normal quote');
    assert.equal(
      f.drafts.getSnapshot(main).edit?.text ?? null,
      context === 'cancel'
        ? null
        : context === 'new editor'
          ? 'Other correction'
          : context === 'ABA'
            ? edit.text
            : 'New correction',
    );
    assert.equal(f.mutations(), 1);
  }
});

test('an unknown edit acknowledgement retries its captured revision before saving newer typing', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setEdit(main, edit);
  f.hooks.after = async () => {
    throw new Error('lost acknowledgement');
  };
  await assert.rejects(f.drafts.sendEdit(main), kind('send'));
  const sent = f.calls.find((c) => c.startsWith('commit:'));
  assert.ok(sent);
  f.drafts.setEditText(main, 'Newer correction');
  f.drafts.setText(main, 'Independent text');
  f.hooks.after = undefined;
  await f.drafts.retry(main);
  assert.deepEqual(
    f.calls.filter((c) => c.startsWith('commit:')),
    [sent, sent],
  );
  assert.equal(f.mutations(), 1);
  assert.equal(f.stored().edit?.text, 'Newer correction');
  assert.equal(f.stored().text, 'Independent text');
});

test('a definitive source refusal permits explicit review and retry while retaining correction', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setEdit(main, edit);
  f.hooks.before = async () => {
    throw Object.assign(new Error('source changed'), { code: 'ERR_ZOEN_DRAFT_REJECTED' });
  };
  await assert.rejects(f.drafts.sendEdit(main), kind('send'));
  assert.equal(f.drafts.getSnapshot(main).edit?.text, edit.text);
  assert.equal(f.drafts.hasPendingSend(main), false);
  f.drafts.setEdit(main, {
    ...edit,
    source_revision: 'fresh-review-r2',
    base_text: 'Concurrent edit',
  });
  f.hooks.before = undefined;
  await f.drafts.sendEdit(main);
  assert.equal(f.mutations(), 1);
});

test('lost private-save acknowledgement reconciles before clearing and never sends cancelled text', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.hooks.saved = async () => {
    throw new Error('lost save response');
  };
  f.drafts.setEdit(main, edit);
  await new Promise<void>((done) => setImmediate(done));
  assert.equal(f.drafts.getSnapshot(main).error?.kind, 'save');
  assert.equal(f.stored().edit?.text, edit.text);
  f.drafts.setEdit(main, null);
  f.hooks.saved = undefined;
  await f.drafts.retry(main);
  assert.equal(f.stored().edit, undefined);
  assert.equal(f.mutations(), 0);
});

test('compiled composer recovers the reviewed correction and its normal text after session reopen', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    await f.act(() =>
      f.current().updateDraft({
        type: 'edit',
        entry: {
          id: edit.message_id,
          text_revision: edit.source_revision,
          kind: { Message: { text: edit.base_text, card: null } },
          voice: null,
        } as TimelineEntry,
        text: edit.base_text,
      }),
    );
    await f.act(() => f.current().updateDraft({ type: 'text', value: edit.text }));
    await f.reopen(undefined, true);
    assert.equal(f.current().capture()?.text, edit.text);
    assert.equal(f.current().capture()?.editingRevision, edit.source_revision);
    assert.equal(f.current().messageDraft.text, 'Owner A private unsent text');
    assert.equal(f.calls.filter((c) => c.op === 'send_message_edit_draft').length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('unmount retires every captured editing control even while the same account remains open', async () => {
  const f = await messageDraftReactFixture();
  try {
    await f.render();
    await f.act(() =>
      f.current().updateDraft({
        type: 'edit',
        entry: {
          id: edit.message_id,
          text_revision: edit.source_revision,
          kind: { Message: { text: edit.base_text, card: null } },
          voice: null,
        } as TimelineEntry,
        text: edit.base_text,
      }),
    );
    const old = f.current(),
      captured = old.capture();
    assert.ok(captured);
    await f.unmount();
    assert.equal(old.capture(), null);
    assert.equal(old.isCurrent(captured), false);
    const before = f.calls.length;
    old.updateDraft({ type: 'text', value: 'Retired correction' });
    old.retry();
    old.flush();
    await assert.rejects(old.sendEdit(captured), { code: 'ERR_ZOEN_DRAFT_SCOPE_CHANGED' });
    await assert.rejects(old.send(), { code: 'ERR_ZOEN_DRAFT_SCOPE_CHANGED' });
    assert.equal(f.calls.length, before);
    assert.equal(f.store.getSnapshot().phase, 'ready');
  } finally {
    await f.close();
  }
});

test('newer typing in the same editor adopts only its own acknowledged revision', async () => {
  const f = fixture(),
    entered = deferred<void>(),
    release = deferred<void>();
  await f.drafts.activate(main);
  f.drafts.setEdit(main, edit);
  f.hooks.before = async () => {
    entered.resolve();
    await release.promise;
  };
  const sent = f.drafts.sendEdit(main);
  await entered.promise;
  f.drafts.setEditText(main, 'Continue while saving');
  release.resolve();
  const accepted = await sent;
  await f.drafts.flush(main);
  assert.equal(f.stored().edit?.text, 'Continue while saving');
  assert.equal(f.stored().edit?.base_text, edit.text);
  assert.ok(Object.isFrozen(f.drafts.getSnapshot(main).edit));
  assert.equal(f.stored().edit?.source_revision, accepted.text_revision);
  assert.notEqual(f.stored().edit?.source_revision, edit.source_revision);
  assert.equal(f.mutations(), 1);
});

for (const hidden of [false, true])
  test(`a reconciled edit uses its accepted stamp, independently of ${hidden ? 'a hidden message' : 'a later observed edit'}`, async () => {
    const f = fixture();
    await f.drafts.activate(main);
    f.drafts.setEdit(main, edit);
    f.hooks.after = async () => {
      throw new Error('lost acknowledgement');
    };
    await assert.rejects(f.drafts.sendEdit(main), kind('send'));
    f.drafts.setEditText(main, 'Continued private typing');
    f.project(
      hidden
        ? null
        : ({ id: edit.message_id, text_revision: 'later-observed-r3' } as TimelineEntry),
    );
    f.hooks.after = undefined;
    await f.drafts.retry(main);
    assert.equal(f.stored().edit?.source_revision, 'accepted-r2');
    assert.equal(f.stored().edit?.base_text, edit.text);
    assert.equal(f.stored().edit?.text, 'Continued private typing');
    assert.equal(f.mutations(), 1);
  });

test('a legacy unstamped receipt cannot borrow a visible revision for continued typing', async () => {
  const f = fixture();
  const send = f.port.sendEdit;
  assert.ok(send);
  f.port.sendEdit = async (...args) => ({ ...(await send(...args)), text_revision: null });
  await f.drafts.activate(main);
  f.drafts.setEdit(main, edit);
  f.hooks.after = async () => {
    throw new Error('lost acknowledgement');
  };
  await assert.rejects(f.drafts.sendEdit(main), kind('send'));
  f.drafts.setEditText(main, 'Keep without guessing a revision');
  f.project({ id: edit.message_id, text_revision: 'later-visible-r3' } as TimelineEntry);
  f.hooks.after = undefined;
  await f.drafts.retry(main);
  assert.equal(f.stored().edit?.source_revision, edit.source_revision);
  assert.equal(f.stored().edit?.base_text, edit.base_text);
  assert.equal(f.stored().edit?.text, 'Keep without guessing a revision');
  assert.equal(f.mutations(), 1);
});

test('a substituted acceptance cannot consume the editor and its original retry stays exact once', async () => {
  const f = fixture();
  const send = f.port.sendEdit;
  assert.ok(send);
  f.port.sendEdit = async (...args) => ({
    ...(await send(...args)),
    message_id: 'another-message',
  });
  await f.drafts.activate(main);
  f.drafts.setEdit(main, edit);
  await assert.rejects(f.drafts.sendEdit(main), /Invalid draft acceptance/);
  assert.equal(f.drafts.getSnapshot(main).edit?.text, edit.text);
  f.port.sendEdit = send;
  await f.drafts.retry(main);
  assert.equal(f.drafts.getSnapshot(main).edit, null);
  assert.equal(f.mutations(), 1);
});
