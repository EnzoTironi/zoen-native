import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  createReviewPort,
  reviewAvailable,
  reviewMetadataValid,
  ToolReviewSession,
  type ReviewScope,
  type ReviewPort,
} from './tool-review-renewal.ts';
import type { AgentRequestDto } from './generated/AgentRequestDto';

const now = Date.now();
const request = (): AgentRequestDto =>
  ({
    id: 'old-request',
    agent: { id: 'agent', is_mine: true },
    status: 'Approved',
    tool_approval: {
      artifact_id: 'fresh-item',
      artifact_title: 'Synthetic Page',
      version: 1,
      content_hash: 'ab'.repeat(32),
      operation: 'Replace block',
      expires_ms: String(now - 1000),
      review: {
        state: 'Available',
        root_request_id: 'old-request',
        predecessor_request_id: null,
        successor_request_id: null,
      },
    },
  }) as AgentRequestDto;
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
function fixture() {
  let scope: ReviewScope | null = {
    identity: 'owner',
    device: 'phone',
    relay: 'https://relay.test',
    epoch: 1,
  };
  let rows = [request()];
  let reads: () => Promise<AgentRequestDto[]> = async () => structuredClone(rows);
  const writes: unknown[] = [];
  let effect = async (_id: string, _hash: string): Promise<unknown> => {
    rows[0]!.tool_approval!.review!.state = 'Requested';
    return {};
  };
  const port: ReviewPort = {
    scope: () => scope,
    requests: () => reads(),
    requestReview: (id, hash) => {
      writes.push([id, hash]);
      return effect(id, hash);
    },
  };
  const session = new ToolReviewSession(port, 'old-request');
  session.activate(scope);
  return {
    session,
    writes,
    port,
    scope: (next: ReviewScope | null) => {
      scope = next;
    },
    rows: (next: AgentRequestDto[]) => {
      rows = next;
    },
    reads: (next: typeof reads) => {
      reads = next;
    },
    effect: (next: typeof effect) => {
      effect = next;
    },
  };
}

test('renewal dispatches only the exact predecessor and observed hash from one explicit button intent', async () => {
  const f = fixture();
  await f.session.refresh();
  assert.equal(await f.session.renew(f.session.ticket()), true);
  assert.deepEqual(f.writes, [['old-request', 'ab'.repeat(32)]]);
  assert.equal(f.session.getSnapshot().request?.tool_approval?.review?.state, 'Requested');
  assert.equal(f.session.ticket(), null);
  await f.session.refresh();
  assert.equal(f.writes.length, 1);
});

test('preflight reads retire on owner/device/relay/epoch/focus or canonical revision changes', async () => {
  for (const retire of [
    (f: ReturnType<typeof fixture>) =>
      f.scope({ identity: 'other', device: 'phone', relay: 'https://relay.test', epoch: 1 }),
    (f: ReturnType<typeof fixture>) =>
      f.scope({ identity: 'owner', device: 'new-phone', relay: 'https://relay.test', epoch: 1 }),
    (f: ReturnType<typeof fixture>) =>
      f.scope({ identity: 'owner', device: 'phone', relay: 'https://other.test', epoch: 1 }),
    (f: ReturnType<typeof fixture>) =>
      f.scope({ identity: 'owner', device: 'phone', relay: 'https://relay.test', epoch: 2 }),
    (f: ReturnType<typeof fixture>) => f.session.retire(),
    (f: ReturnType<typeof fixture>) => f.session.refresh(),
  ]) {
    const f = fixture();
    await f.session.refresh();
    const preflight = deferred<AgentRequestDto[]>();
    const ticket = f.session.ticket();
    f.reads(() => preflight.promise);
    const attempt = f.session.renew(ticket);
    const retirement = retire(f);
    preflight.resolve([request()]);
    await retirement;
    assert.equal(await attempt, false);
    assert.deepEqual(f.writes, []);
  }
});

test('the current-state preflight read refuses changed hashes, status, owner, malformed or missing projections', async () => {
  for (const mutate of [
    (r: AgentRequestDto) => {
      r.tool_approval!.content_hash = 'cd'.repeat(32);
    },
    (r: AgentRequestDto) => {
      r.agent.is_mine = false;
    },
    (r: AgentRequestDto) => {
      r.agent.id = 'other-agent';
    },
    (r: AgentRequestDto) => {
      r.status = 'Denied';
    },
    (r: AgentRequestDto) => {
      r.tool_approval!.review!.state = 'Requested';
    },
  ]) {
    const f = fixture();
    await f.session.refresh();
    const changed = request();
    mutate(changed);
    f.rows([changed]);
    assert.equal(await f.session.renew(f.session.ticket()), false);
    assert.equal(f.writes.length, 0);
  }
  for (const rows of [[], [request(), request()]]) {
    const f = fixture();
    await f.session.refresh();
    f.rows(rows);
    assert.equal(await f.session.renew(f.session.ticket()), false);
    assert.equal(f.writes.length, 0);
  }
});

test('failed reads close admission and an unknown mutation only reconciles without resend', async () => {
  const f = fixture();
  f.reads(async () => {
    throw new Error('synthetic read failure');
  });
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().phase, 'failed');
  assert.equal(f.session.ticket(), null);
  f.reads(async () => [request()]);
  await f.session.refresh();
  f.effect(async () => {
    throw new Error('unknown bridge outcome');
  });
  assert.equal(await f.session.renew(f.session.ticket()), false);
  assert.equal(f.session.getSnapshot().unknown, true);
  assert.equal(f.session.ticket(), null);
  await f.session.refresh();
  assert.equal(f.session.ticket(), null);
  assert.equal(f.writes.length, 1);
  const accepted = request();
  accepted.tool_approval!.review!.state = 'Requested';
  f.reads(async () => [accepted]);
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().unknown, false);
  assert.equal(f.session.ticket(), null);
});

test('an in-flight request stays latched across read refreshes and cannot dispatch twice', async () => {
  const f = fixture();
  await f.session.refresh();
  const pending = deferred<unknown>();
  f.effect(() => pending.promise);
  const ticket = f.session.ticket();
  const first = f.session.renew(ticket);
  for (let n = 0; n < 8; ++n) await Promise.resolve();
  assert.equal(f.writes.length, 1);
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().busy, true);
  assert.equal(f.session.ticket(), null);
  await f.session.renew(ticket);
  assert.equal(f.writes.length, 1);
  pending.resolve({});
  await first;
  assert.equal(f.session.getSnapshot().busy, false);
});

test('closed metadata and eligible expired approval never route or renew another request', async () => {
  assert.equal(reviewAvailable(request(), now), true);
  for (const mutate of [
    (r: AgentRequestDto) => {
      r.tool_approval!.expires_ms = String(now + 60_000);
    },
    (r: AgentRequestDto) => {
      r.tool_approval!.review = undefined;
    },
    (r: AgentRequestDto) => {
      r.tool_approval!.review!.root_request_id = '';
    },
    (r: AgentRequestDto) => {
      r.tool_approval!.review!.successor_request_id = r.id;
    },
    (r: AgentRequestDto) => {
      r.tool_approval!.review!.state = 'future' as never;
    },
  ]) {
    const r = request();
    mutate(r);
    assert.equal(reviewAvailable(r, now), false);
  }
  const f = fixture();
  const r = request();
  r.tool_approval!.review!.state = 'Superseded';
  r.tool_approval!.review!.successor_request_id = 'new-pending';
  assert.equal(reviewMetadataValid(r), true);
  f.rows([r]);
  await f.session.refresh();
  assert.equal(f.session.successor(), 'new-pending');
  f.session.retire();
  assert.equal(f.session.successor(), null);
});

test('the actual generic command port has no approve, retry, source or argument authority', async () => {
  const calls: unknown[] = [];
  const port = createReviewPort({
    driver: {
      request: async (command: unknown) => {
        calls.push(command);
        return [];
      },
    },
    request: async (command: unknown) => {
      calls.push(command);
      return {};
    },
    getSnapshot: () => ({
      phase: 'ready',
      mode: 'account',
      erasePending: false,
      snapshot: {
        account: {
          registered: true,
          unlocked: true,
          setup_pending: false,
          identity_id: 'owner',
          device_id: 'phone',
          relay_url: 'https://relay.test',
        },
      },
    }),
    isSessionRetiring: false,
    sessionEpoch: 4,
  } as never);
  assert.equal(port.scope()?.epoch, 4);
  await port.requests();
  await port.requestReview('old-request', 'ab'.repeat(32));
  assert.deepEqual(calls, [
    { op: 'requests' },
    {
      op: 'decide_tool_request',
      request_id: 'old-request',
      observed_content_hash: 'ab'.repeat(32),
      decision: 'RequestReview',
    },
  ]);
});

test('failure before dispatch remains a failed read, not an unknown action or an implicit resend', async () => {
  const f = fixture();
  await f.session.refresh();
  const ticket = f.session.ticket();
  f.reads(async () => {
    throw new Error('second read failed');
  });
  assert.equal(await f.session.renew(ticket), false);
  assert.equal(f.session.getSnapshot().phase, 'failed');
  assert.equal(f.session.getSnapshot().unknown, false);
  assert.equal(f.writes.length, 0);
  f.reads(async () => [request()]);
  await f.session.refresh();
  assert.ok(f.session.ticket());
  assert.equal(f.writes.length, 0);
});
