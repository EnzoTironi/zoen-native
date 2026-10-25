import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import type { TimelineEntry } from '../core/generated/TimelineEntry';
import {
  EditorAgentSession,
  type EditorAgentPort,
  type EditorAgentDecision,
} from './editor-agent-session.ts';
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function request(item = 'app', space = 'space') {
  return {
    id: `review-${item}-${space}`,
    space_id: space,
    agent: { is_mine: true },
    status: 'Pending',
    tool_approval: {
      artifact_id: item,
      artifact_title: 'Document',
      version: 2,
      operation: 'format',
      expires_ms: String(Date.now() + 60_000),
      content_hash: 'ab'.repeat(32),
    },
  } as AgentRequestDto;
}
function fixture() {
  let scope = 'owner:phone:epoch1',
    ids = 0;
  const calls: unknown[][] = [],
    entry = { id: 'signed-message' } as TimelineEntry;
  const port = {
    scope: () => scope,
    id: () => `request-${++ids}`,
    flush: async () => {
      calls.push(['flush']);
    },
    head: async () => {
      calls.push(['head']);
      return { version: 2, sha256: 'cd'.repeat(32) };
    },
    requests: async () => [request(), request('other'), request('app', 'other')],
    replies: async (_sourceId: string) => [] as TimelineEntry[],
    send: async (...args: Parameters<EditorAgentPort['send']>) => {
      calls.push(['send', ...args]);
      return { entry: entry as TimelineEntry | null, conflict: null as string | null };
    },
    decide: async (command: EditorAgentDecision) => {
      calls.push(['decide', command]);
      return { request: request(), message: 'Approved', standing_grant_id: null, also_resolved: 0 };
    },
  };
  const session = new EditorAgentSession(port, scope, 'app', 'space');
  return {
    session,
    port,
    calls,
    entry,
    scope: (next: string) => {
      scope = next;
    },
  };
}
const tick = () => new Promise<void>((done) => setImmediate(done));
test('open/close only reads, retains the unsent draft and filters exact artifact reviews', async () => {
  const f = fixture();
  f.session.open();
  await tick();
  assert.equal(f.session.getSnapshot().requests.length, 1);
  f.session.setDraft('Human draft');
  f.session.close();
  f.session.open();
  await tick();
  assert.equal(f.session.getSnapshot().draft, 'Human draft');
  assert.deepEqual(f.calls, []);
});
test('send flushes without retire before reading head and publishing one explicit request', async () => {
  const f = fixture(),
    barrier = deferred<void>();
  f.port.flush = async () => {
    f.calls.push(['flush']);
    await barrier.promise;
  };
  f.session.open();
  f.session.setDraft('  Make heading bold  ');
  const sent = f.session.send();
  await tick();
  assert.deepEqual(f.calls, [['flush']]);
  barrier.resolve();
  await sent;
  assert.deepEqual(f.calls, [
    ['flush'],
    ['head'],
    ['send', 'Make heading bold', { version: 2, sha256: 'cd'.repeat(32) }, 'request-1'],
  ]);
  assert.equal(f.session.getSnapshot().draft, '');
  assert.equal(f.session.getSnapshot().sent?.id, f.entry.id);
});
test('closing or changing account during flush prevents publication and preserves only the same-owner draft', async () => {
  for (const retire of [false, true]) {
    const f = fixture(),
      barrier = deferred<void>();
    f.port.flush = () => barrier.promise;
    f.session.open();
    f.session.setDraft('Unsent');
    const sending = f.session.send();
    if (retire) {
      f.scope('owner:phone:epoch2');
      f.session.retire();
    } else f.session.close();
    barrier.resolve();
    await sending;
    assert.deepEqual(f.calls, []);
    assert.equal(f.session.getSnapshot().draft, retire ? '' : 'Unsent');
  }
});
test('unknown send outcome retries identical head/id across sheet close/reopen', async () => {
  const f = fixture();
  let fail = true;
  f.port.send = async (...args) => {
    f.calls.push(['send', ...args]);
    if (fail) throw Error('transport');
    return { entry: f.entry, conflict: null };
  };
  f.session.open();
  f.session.setDraft('Change title');
  await f.session.send();
  assert.equal(f.session.getSnapshot().uncertain, true);
  f.session.close();
  f.session.open();
  f.session.setDraft('Cannot replace an unknown intent');
  assert.equal(f.session.getSnapshot().draft, 'Change title');
  fail = false;
  await f.session.send();
  const sends = f.calls.filter(([kind]) => kind === 'send');
  assert.deepEqual(sends[0], sends[1]);
  assert.equal(f.calls.filter(([kind]) => kind === 'head').length, 1);
});
test('definitive conflict leaves an editable draft and never resends automatically', async () => {
  const f = fixture();
  f.port.send = async () => ({ entry: null, conflict: 'Current version changed' });
  f.session.open();
  f.session.setDraft('Keep prompt');
  await f.session.send();
  assert.equal(f.session.getSnapshot().uncertain, false);
  assert.equal(f.session.getSnapshot().draft, 'Keep prompt');
  f.session.setDraft('Updated intent');
  assert.equal(f.session.getSnapshot().draft, 'Updated intent');
});
test('a later terminal refusal restores the original prompt without resending or replacing newer text', async () => {
  for (const draft of ['', 'New human intent']) {
    const f = fixture();
    f.session.open();
    f.session.setDraft('Original reviewed prompt');
    await f.session.send();
    f.session.setDraft(draft);
    f.port.replies = async () =>
      [
        {
          ...f.entry,
          delivery: 'Failed',
          kind: { Message: { text: 'Original reviewed prompt', card: null } },
        },
      ] as TimelineEntry[];
    await f.session.refresh();
    await tick();
    assert.equal(f.session.getSnapshot().sent?.delivery, 'Failed');
    assert.equal(f.session.getSnapshot().draft, draft || 'Original reviewed prompt');
    assert.equal(f.session.getSnapshot().uncertain, false);
    assert.equal(f.calls.filter(([op]) => op === 'send').length, 1);
    f.session.setDraft('');
    await f.session.refresh();
    assert.equal(f.session.getSnapshot().draft, '', 'a later refresh cannot undo a human clear');
    f.session.setDraft(draft || 'Original reviewed prompt');
    f.session.close();
    f.session.open();
    await tick();
    assert.equal(f.calls.filter(([op]) => op === 'send').length, 1);
    await f.session.send();
    const sends = f.calls.filter(([op]) => op === 'send');
    assert.equal(sends.length, 2);
    assert.equal(sends[1]?.[3], 'request-2');
  }
});
test('review uses existing observed hash after flush and rejects a lateral artifact or cancelled sheet', async () => {
  const f = fixture(),
    barrier = deferred<void>();
  f.session.open();
  await tick();
  const command: EditorAgentDecision = {
    op: 'decide_tool_request',
    request_id: request().id,
    observed_content_hash: 'ab'.repeat(32),
    decision: 'ApproveOnce',
  };
  await assert.rejects(f.session.decide({ ...command, request_id: request('other').id }));
  f.port.flush = () => barrier.promise;
  const deciding = f.session.decide(command);
  f.session.close();
  barrier.resolve();
  await assert.rejects(deciding);
  assert.equal(
    f.calls.some(([kind]) => kind === 'decide'),
    false,
  );
  f.session.open();
  await tick();
  await f.session.decide(command);
  assert.deepEqual(f.calls.at(-1), ['decide', command]);
});
test('late reads cannot overwrite a new sheet activation or a retired account', async () => {
  const f = fixture(),
    read = deferred<AgentRequestDto[]>();
  let reads = 0;
  f.port.requests = () => (++reads === 1 ? read.promise : Promise.resolve([request()]));
  f.session.open();
  f.session.close();
  f.session.open();
  read.resolve([request('other')]);
  await tick();
  await tick();
  assert.equal(reads, 2);
  assert.equal(f.session.getSnapshot().requests[0]?.tool_approval?.artifact_id, 'app');
  f.scope('another-owner');
  f.session.retire();
  assert.deepEqual(f.session.getSnapshot().requests, []);
});
test('only canonical agent replies to the current source are shown and retirement clears them', async () => {
  const f = fixture();
  f.port.replies = async (sourceId) =>
    [
      { id: 'reply', reply_to: { id: sourceId }, author: { kind: 'Agent' }, kind: { Message: {} } },
      {
        id: 'lateral',
        reply_to: { id: 'other' },
        author: { kind: 'Agent' },
        kind: { Message: {} },
      },
      {
        id: 'person',
        reply_to: { id: sourceId },
        author: { kind: 'Person' },
        kind: { Message: {} },
      },
      {
        id: 'deleted',
        reply_to: { id: sourceId },
        author: { kind: 'Agent' },
        kind: { Message: {} },
        deleted: true,
      },
      { id: 'update', reply_to: { id: sourceId }, author: { kind: 'Agent' }, kind: { System: {} } },
    ] as TimelineEntry[];
  f.session.open();
  f.session.setDraft('Please edit');
  await f.session.send();
  await tick();
  await f.session.refresh();
  assert.deepEqual(
    f.session.getSnapshot().replies.map(({ id }) => id),
    ['reply'],
  );
  f.session.retire();
  assert.deepEqual(f.session.getSnapshot().replies, []);
});

test('captured prompt/send tickets cannot act after dismissal or content replacement', async () => {
  const f = fixture();
  f.session.open();
  await tick();
  f.session.setDraft('Original');
  const stale = f.session.ticket();
  assert.ok(stale);
  f.session.renewTicket();
  f.session.setDraft('Stale panel event', stale);
  await f.session.send(stale);
  assert.equal(f.session.getSnapshot().draft, 'Original');
  assert.deepEqual(f.calls, []);
  const closed = f.session.ticket();
  f.session.close();
  f.session.open();
  await tick();
  f.session.setDraft('Stale dismissed event', closed);
  await f.session.send(closed);
  assert.equal(f.session.getSnapshot().draft, 'Original');
  assert.deepEqual(f.calls, []);
  await f.session.send(f.session.ticket());
  assert.equal(f.calls.filter((call) => call[0] === 'send').length, 1);
});
