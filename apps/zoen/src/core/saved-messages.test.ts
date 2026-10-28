import assert from 'node:assert/strict';
import { test } from 'node:test';
import { SavedMessagesSession } from '../saved-messages/session.ts';
import { conversationTitle } from '../saved-messages/presentation.ts';
import { unreadSnapshot } from './store.fixtures.ts';
import type { SpaceSummary } from './generated/SpaceSummary';
import type { AccountScope } from './account-scope.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const owner: AccountScope = { identity: 'me', device: 'phone', relay: 'relay', epoch: 1 };
function fixture() {
  let scope: AccountScope | null = owner;
  let summary: SpaceSummary | undefined;
  const attempts: ReturnType<typeof deferred<SpaceSummary>>[] = [];
  const opened: string[] = [];
  const saved: SpaceSummary = {
    ...unreadSnapshot(0).conversations[0]!,
    id: 'canonical',
    kind: 'saved',
  };
  const session = new SavedMessagesSession({
    scope: () => scope,
    summary: () => summary,
    ensure: (expected) => {
      assert.deepEqual(expected, scope);
      const attempt = deferred<SpaceSummary>();
      attempts.push(attempt);
      return attempt.promise;
    },
    open: (id) => opened.push(id),
  });
  return {
    session,
    attempts,
    opened,
    saved,
    setSummary: (next: SpaceSummary | undefined) => {
      summary = next;
    },
    setScope: (next: AccountScope | null) => {
      scope = next;
    },
  };
}
const settled = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

test('a cached canonical writable saved chat opens offline without another mutation', () => {
  const f = fixture();
  f.setSummary(f.saved);
  f.session.activate(owner);
  f.session.observe();
  assert.deepEqual(f.opened, ['canonical']);
  assert.equal(f.attempts.length, 0);
});

test('first open deduplicates taps, waits for canonical readiness and opens exactly once', async () => {
  const f = fixture();
  f.session.activate(owner);
  void f.session.retry(f.session.getSnapshot().revision);
  assert.equal(f.attempts.length, 1);
  f.setSummary({ ...f.saved, can_write: false });
  f.attempts[0]!.resolve(f.saved);
  await settled();
  assert.equal(f.session.getSnapshot().phase, 'waiting');
  assert.deepEqual(
    f.opened,
    [],
    'the command result cannot substitute for canonical snapshot readiness',
  );
  f.setSummary(f.saved);
  f.session.observe();
  f.session.observe();
  assert.deepEqual(f.opened, ['canonical']);
});

test('failure remains retryable without a fictitious destination or automatic retry loop', async () => {
  const f = fixture();
  f.session.activate(owner);
  f.attempts[0]!.reject(new Error('offline'));
  await settled();
  assert.equal(f.session.getSnapshot().phase, 'failed');
  assert.equal(f.attempts.length, 1);
  assert.deepEqual(f.opened, []);
  const retry = f.session.retry(f.session.getSnapshot().revision);
  f.setSummary(f.saved);
  f.attempts[1]!.resolve(f.saved);
  await retry;
  assert.deepEqual(f.opened, ['canonical']);
});

test('blur, background and preflight retirement prevent late navigation and errors', async () => {
  const f = fixture();
  f.session.activate(owner);
  f.session.retire();
  f.setSummary(f.saved);
  f.attempts[0]!.resolve(f.saved);
  await settled();
  assert.deepEqual(f.opened, []);
  assert.equal(f.session.getSnapshot().phase, 'unavailable');
  f.session.activate(owner);
  assert.deepEqual(f.opened, ['canonical']);
});

test('same-account reopen, device replacement and account replacement fence old completions', async () => {
  for (const next of [
    { ...owner, epoch: 2 },
    { ...owner, device: 'other' },
    { ...owner, identity: 'other' },
    null,
  ]) {
    const f = fixture();
    f.session.activate(owner);
    f.setScope(next);
    f.setSummary(f.saved);
    f.attempts[0]!.resolve(f.saved);
    await settled();
    f.session.observe();
    assert.deepEqual(f.opened, []);
  }
});

test('a self-DM or a chat named Saved Messages does not become the saved chat', async () => {
  const f = fixture();
  const impostor = { ...f.saved, kind: 'Direct' as const, title: 'Saved Messages' };
  f.setSummary(impostor);
  f.session.activate(owner);
  f.attempts[0]!.resolve(impostor);
  await settled();
  assert.deepEqual(f.opened, []);
  assert.equal(f.session.getSnapshot().phase, 'waiting');
  assert.equal(conversationTitle({ kind: 'Direct', title: 'Personal' }), 'Personal');
  assert.notEqual(conversationTitle({ kind: 'saved', title: 'Internal name' }), 'Internal name');
});

test('no unlocked account admits neither ensure nor navigation', () => {
  const f = fixture();
  f.setScope(null);
  f.session.activate(null);
  void f.session.retry(f.session.getSnapshot().revision);
  assert.equal(f.attempts.length, 0);
  assert.deepEqual(f.opened, []);
});

test('retry from a retired render cannot adopt a refocused lease', async () => {
  const f = fixture();
  f.session.activate(owner);
  const rendered = f.session.getSnapshot().revision;
  f.attempts[0]!.reject(new Error('offline'));
  await settled();
  const oldRetry = () => f.session.retry(rendered);
  f.session.retire();
  f.session.activate(owner);
  f.attempts[1]!.reject(new Error('still offline'));
  await settled();
  await oldRetry();
  assert.equal(f.attempts.length, 2);
  assert.equal(f.session.getSnapshot().phase, 'failed');
});
