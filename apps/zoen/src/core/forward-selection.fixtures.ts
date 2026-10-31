import assert from 'node:assert/strict';
import type { HomeOwner } from '../home/preferences';
import type { TimelineEntry } from './types';
import type { ForwardBatchReview } from './generated/ForwardBatchReview';
import type { ForwardBatchSource } from './generated/ForwardBatchSource';
import type { ForwardOptions } from './generated/ForwardOptions';
import { ForwardSelectionSession, type ForwardSelectionReview } from './forward-selection.ts';

export const owner: HomeOwner = { identityId: 'alice', deviceId: 'phone', mode: 'account' };
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { resolve, reject, promise };
}
export function fixture() {
  let actor: HomeOwner | null = owner,
    epoch = 1,
    writable = true;
  let spaces = new Set(['source', 'target', 'other']);
  let selected: ForwardSelectionReview | null = null,
    error: unknown = null,
    busy = false;
  let record: ForwardBatchReview | null = null,
    serial = 0,
    revision = 0;
  const calls: {
    op: string;
    source: ForwardBatchSource;
    batch?: string;
    revision?: string;
    target?: string;
    options?: ForwardOptions;
  }[] = [];
  const sent: string[] = [];
  let statusHook: (() => Promise<ForwardBatchReview | null>) | null = null;
  let confirmHook: ((value: ForwardBatchReview) => Promise<ForwardBatchReview>) | null = null;
  let finishHook: ((value: ForwardBatchReview) => Promise<ForwardBatchReview>) | null = null;
  let configureHook: ((value: ForwardBatchReview) => Promise<ForwardBatchReview>) | null = null;
  const accept = (ids = ['a', 'b', 'c']) => {
    assert.ok(record);
    record = {
      ...record,
      ready: false,
      messages: record.messages.map((item) =>
        ids.includes(item.message.id)
          ? { ...item, accepted_message_id: `accepted-${item.message.id}` }
          : item,
      ),
    };
    return record;
  };
  const session = new ForwardSelectionSession(
    { owner, spaceId: 'source', messageIds: ['a', 'b', 'c'], sessionEpoch: 1 },
    {
      owner: () => actor,
      epoch: () => epoch,
      spaces: () => spaces,
      writable: () => writable,
      status: async (source) => {
        calls.push({ op: 'status', source });
        return statusHook ? statusHook() : record;
      },
      review: async (scope) => {
        calls.push({ op: 'review', source: scope.source, target: scope.target_space });
        const existing = record && !record.completed ? record : null;
        record = {
          batch_id: existing?.batch_id ?? `batch-${++serial}`,
          review_revision: `revision-${++revision}`,
          target_space: scope.target_space,
          ready: true,
          completed: false,
          error: null,
          skipped_message_ids: [],
          messages: scope.source.message_ids.map((id) => {
            const accepted = existing?.messages.find(
              (item) => item.message.id === id && item.accepted_message_id !== null,
            );
            return (
              accepted ?? {
                target_space: scope.target_space,
                operation_id: `op-${id}-${revision}`,
                source_revision: `source-${revision}`,
                accepted_message_id: null,
                options: { hide_author: false, hide_caption: false },
                accepted: null,
                message: {
                  id,
                  kind: { Message: { text: `Source ${id}`, card: null } },
                  deleted: false,
                } as TimelineEntry,
              }
            );
          }),
        };
        return record;
      },
      confirm: async (source, batch, token) => {
        calls.push({ op: 'confirm', source, batch, revision: token });
        assert.equal(batch, record?.batch_id);
        assert.equal(token, record?.review_revision);
        return confirmHook ? confirmHook(record!) : accept();
      },
      configure: async (source, batch, token, options) => {
        calls.push({ op: 'configure', source, batch, revision: token, options });
        assert.ok(record);
        assert.equal(batch, record.batch_id);
        assert.equal(token, record.review_revision);
        record = {
          ...record,
          review_revision: `revision-${++revision}`,
          messages: record.messages.map((item) =>
            item.accepted_message_id === null
              ? {
                  ...item,
                  options: { ...options },
                  operation_id: `op-${item.message.id}-${revision}`,
                }
              : item,
          ),
        };
        return configureHook ? configureHook(record) : record;
      },
      finish: async (source, batch) => {
        calls.push({ op: 'finish', source, batch });
        assert.equal(batch, record?.batch_id);
        if (finishHook) return finishHook(record!);
        record = { ...record!, completed: true, ready: false };
        return record;
      },
      abandon: async (source, batch, token) => {
        calls.push({ op: 'abandon', source, batch, revision: token });
        assert.equal(batch, record?.batch_id);
        assert.equal(token, record?.review_revision);
        record = {
          ...record!,
          completed: true,
          ready: false,
          skipped_message_ids: record!.messages
            .filter((item) => item.accepted_message_id === null)
            .map((item) => item.message.id),
        };
        return record;
      },
    },
    {
      busy: (value) => {
        busy = value;
      },
      error: (value) => {
        error = value;
      },
      reviewed: (value) => {
        selected = value;
      },
      sent: (target) => sent.push(target),
    },
  );
  return {
    session,
    calls,
    sent,
    accept,
    selected: () => selected,
    state: () => ({ busy, error }),
    record: () => record!,
    setRecord: (value: ForwardBatchReview | null) => {
      record = value;
    },
    actor: (value: HomeOwner | null) => {
      actor = value;
    },
    epoch: (value: number) => {
      epoch = value;
    },
    spaces: (value: Set<string>) => {
      spaces = value;
    },
    writable: (value: boolean) => {
      writable = value;
    },
    status: (hook: typeof statusHook) => {
      statusHook = hook;
    },
    confirm: (hook: typeof confirmHook) => {
      confirmHook = hook;
    },
    finish: (hook: typeof finishHook) => {
      finishHook = hook;
    },
    configure: (hook: typeof configureHook) => {
      configureHook = hook;
    },
    ready: async () => {
      await session.focus();
      await session.prepare('target');
      return selected!;
    },
  };
}
export const ops = (f: ReturnType<typeof fixture>, op: string) =>
  f.calls.filter((call) => call.op === op);
