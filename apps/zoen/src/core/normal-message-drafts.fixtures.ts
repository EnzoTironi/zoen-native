import assert from 'node:assert/strict';
import {
  NormalDraftFailure,
  NormalMessageDrafts,
  type NormalDraftPort,
  type NormalDraftScope,
  type NormalDraftSnapshot,
} from './normal-message-drafts.ts';
import type { TimelineEntry } from './types';

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
export const main: NormalDraftScope = {
  owner: { identityId: 'qa-owner', deviceId: 'qa-device', sessionEpoch: 1, mode: 'account' },
  spaceId: 'qa-chat',
  threadRootId: null,
};
export const key = (scope: NormalDraftScope) => JSON.stringify(scope);
export const kind = (expected: NormalDraftFailure['kind']) => (cause: unknown) =>
  cause instanceof NormalDraftFailure && cause.kind === expected;
export function fixture() {
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
