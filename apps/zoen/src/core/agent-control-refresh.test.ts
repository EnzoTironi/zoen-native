import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AgentControlSession, type AgentControlPort } from '../agents/agent-control-session.ts';
import type { AgentControlDto } from './generated/AgentControlDto.ts';
import { deferred } from './store.fixtures.ts';

const record = (
  revision: string,
  status: AgentControlDto['status'] = 'Active',
): AgentControlDto => ({
  revision,
  status,
  pending: null,
  operation_id: null,
  outcome: 'Observed',
});
const settle = () => new Promise<void>((resolve) => setImmediate(resolve));
function fixture() {
  let scope = { identity: 'owner', device: 'device', relay: 'relay', epoch: 1 };
  const requests: ReturnType<typeof deferred<AgentControlDto>>[] = [];
  const writes: string[] = [];
  let active = 0,
    peak = 0;
  const port: AgentControlPort = {
    scope: () => scope,
    read: () => {
      const request = deferred<AgentControlDto>();
      requests.push(request);
      peak = Math.max(peak, ++active);
      return request.promise.finally(() => --active);
    },
    control: async () => {
      writes.push('control');
    },
    retry: async () => {
      writes.push('retry');
    },
  };
  const session = new AgentControlSession(port, 'agent');
  session.activate(scope);
  return {
    session,
    requests,
    writes,
    get peak() {
      return peak;
    },
    activate: () => {
      scope = { ...scope, epoch: scope.epoch + 1 };
      session.activate(scope);
    },
  };
}

test('canonical bursts serialize status reads, fence actions immediately and adopt only the latest observation', async () => {
  const f = fixture();
  const initial = f.session.refresh();
  await settle();
  f.requests[0]!.resolve(record('1'));
  await initial;
  const oldTicket = f.session.ticket()!;
  const refreshing = f.session.refresh();
  assert.equal(f.session.ticket(), null);
  await f.session.change(oldTicket);
  assert.deepEqual(f.writes, []);
  await settle();
  const burst = Array.from({ length: 100 }, () => f.session.refresh());
  assert.equal(f.requests.length, 2);
  f.requests[1]!.resolve(record('2'));
  await settle();
  assert.equal(f.requests.length, 3);
  assert.equal(f.session.getSnapshot().phase, 'loading');
  assert.equal(f.session.ticket(), null);
  f.requests[2]!.resolve(record('3', 'Paused'));
  await Promise.all([refreshing, ...burst]);
  assert.equal(f.peak, 1);
  assert.equal(f.session.ticket()!.revision, '3');
  assert.equal(f.session.ticket()!.action, 'Resume');
  assert.deepEqual(f.writes, []);
});

test('a superseded failure cannot publish failure or clear a new activation’s pending read', async () => {
  const f = fixture();
  const old = f.session.refresh();
  await settle();
  const oldQueued = f.session.refresh();
  f.session.suspend();
  f.activate();
  const fresh = f.session.refresh();
  await settle();
  f.requests[0]!.reject(Error('old account read failed'));
  await Promise.all([old, oldQueued]);
  assert.equal(f.session.getSnapshot().phase, 'loading');
  const freshQueued = f.session.refresh();
  assert.equal(f.requests.length, 2);
  f.requests[1]!.reject(Error('invalidated current observation failed'));
  await settle();
  assert.equal(f.requests.length, 3);
  assert.equal(f.session.getSnapshot().phase, 'loading');
  f.requests[2]!.resolve(record('7', 'Paused'));
  await Promise.all([fresh, freshQueued]);
  assert.equal(f.session.getSnapshot().phase, 'ready');
  assert.equal(f.session.ticket()!.revision, '7');
  assert.deepEqual(f.writes, []);
});

test('suspending before an admitted read starts cannot issue RPCs or revive its queued refresh', async () => {
  const f = fixture();
  const reads = [f.session.refresh(), f.session.refresh()];
  f.session.suspend();
  await Promise.all(reads);
  assert.equal(f.requests.length, 0);
  assert.equal(f.session.getSnapshot().phase, 'inactive');
  assert.equal(f.session.ticket(), null);
});

test('rendered lifecycle state cannot borrow action or retry tickets from a later canonical observation', async () => {
  const f = fixture();
  const first = f.session.refresh();
  await settle();
  f.requests[0]!.resolve(record('1'));
  await first;
  const rendered = f.session.getSnapshot();
  assert.ok(f.session.ticket(rendered));
  const next = f.session.refresh();
  assert.equal(f.session.ticket(rendered), null);
  await settle();
  f.requests[1]!.resolve({
    ...record('2'),
    pending: 'Pause',
    operation_id: '11'.repeat(32),
    outcome: 'Unknown',
  });
  await next;
  assert.equal(f.session.ticket(rendered), null);
  assert.equal(f.session.retryTicket(rendered), null);
  assert.equal(f.session.retryTicket(f.session.getSnapshot())?.action, 'Pause');
  assert.deepEqual(f.writes, []);
});
