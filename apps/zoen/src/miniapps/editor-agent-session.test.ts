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
