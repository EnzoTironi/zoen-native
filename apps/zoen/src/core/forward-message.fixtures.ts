import assert from 'node:assert/strict';
import type { HomeOwner } from '../home/preferences';
import type { TimelineEntry } from './types';
import type { ForwardReview as Canonical } from './generated/ForwardReview';
import type { ForwardOptions } from './generated/ForwardOptions';
import { ForwardMessageSession, type ForwardMessageReview } from './forward-message.ts';
export const owner: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone' };
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export const message = {
  id: 'message',
  kind: { Message: { text: 'Exact source', card: null } },
  deleted: false,
} as TimelineEntry;
export function fixture() {
  let actor: HomeOwner | null = owner,
    epoch = 1,
    writable = true,
    spaces = new Set(['source', 'target', 'other']);
  let record: Canonical | null = null,
    next = 0,
    revision = 'r1',
    selected: ForwardMessageReview | null = null,
    error: unknown = null,
    busy = false;
  const sends: string[] = [],
    configurations: { id: string; revision: string; options: ForwardOptions }[] = [],
    confirms: string[] = [],
    dismisses: (string | null)[] = [],
    reviews: string[] = [];
  let statusHook: (() => Promise<Canonical | null>) | null = null,
    configureHook: ((value: Canonical) => Promise<Canonical>) | null = null,
    confirmHook: ((value: Canonical) => Promise<Canonical>) | null = null,
    dismissHook: (() => Promise<void>) | null = null;
  const accept = (value: Canonical) =>
    (record = { ...value, accepted_message_id: 'accepted', accepted: message });
  const session = new ForwardMessageSession(
    { owner, spaceId: 'source', messageId: 'message', sessionEpoch: 1 },
    {
      owner: () => actor,
      epoch: () => epoch,
      writable: () => writable,
      spaces: () => spaces,
      status: async (scope) => {
        assert.equal(scope.source_space, 'source');
        return statusHook ? statusHook() : record;
      },
      review: async (scope) => {
        reviews.push(scope.target_space);
        return (record ??= {
          target_space: scope.target_space,
          operation_id: `op${++next}`,
          source_revision: revision,
          message,
          accepted: null,
          accepted_message_id: null,
          options: { hide_author: false, hide_caption: false },
        });
      },
      configure: async (scope, id, rev, options) => {
        configurations.push({ id, revision: rev, options });
        assert.equal(scope.target_space, record?.target_space);
        assert.equal(id, record?.operation_id);
        assert.equal(rev, record?.source_revision);
        assert.ok(record);
        record = { ...record, operation_id: `op${++next}`, options: { ...options } };
        return configureHook ? configureHook(record) : record;
      },
      confirm: async (scope, id, rev) => {
        confirms.push(id);
        assert.equal(scope.expected_identity_id, owner.identityId);
        assert.equal(id, record?.operation_id);
        assert.equal(scope.target_space, record?.target_space);
        if (!record) throw Error('Missing');
        if (confirmHook) return confirmHook(record);
        if (record.accepted_message_id) return record;
        if (rev !== revision) throw Error('Changed');
        return accept(record);
      },
      dismiss: async (scope, id, accepted) => {
        dismisses.push(accepted);
        if (dismissHook) return dismissHook();
        if (!record) return;
        if (
          scope.target_space !== record.target_space ||
          id !== record.operation_id ||
          accepted !== record.accepted_message_id
        )
          throw Error('CAS lost');
        record = null;
      },
    },
    {
      busy: (v) => {
        busy = v;
      },
      error: (v) => {
        error = v;
      },
      reviewed: (v) => {
        selected = v;
      },
      sent: (v) => {
        sends.push(v);
      },
    },
  );
  return {
    session,
    ready: () => session.focus(),
    selected: () => selected,
    record: () => record,
    state: () => ({ error, busy }),
    sends,
    confirms,
    dismisses,
    reviews,
    configurations,
    accept,
    status: (hook: typeof statusHook) => {
      statusHook = hook;
    },
    confirm: (hook: typeof confirmHook) => {
      confirmHook = hook;
    },
    configure: (hook: typeof configureHook) => {
      configureHook = hook;
    },
    dismiss: (hook: typeof dismissHook) => {
      dismissHook = hook;
    },
    revision: (v: string) => {
      revision = v;
    },
    revoke: (kind: string) => {
      if (kind === 'owner') actor = { ...owner, identityId: 'foreign' };
      if (kind === 'epoch') epoch = 2;
      if (kind === 'membership') spaces = new Set(['source']);
      if (kind === 'writer') writable = false;
    },
  };
}
