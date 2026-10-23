import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import { Driver, deferred, draftSession, tick } from './store.fixtures.ts';
import type { Snapshot } from './types.ts';
import { createOwnerPolicyPort, policyAvailability } from '../owner-policy/core-port.ts';
const target = {
  spaceId: 'private-dm',
  agentId: 'agent',
  requestId: 'request',
  expectedHash: 'review-hash',
};
function snapshot(): Snapshot {
  const original = draftSession(),
    owner = original.conversations[0]?.members[0];
  assert.ok(owner);
  const agent = {
    ...owner,
    id: 'agent',
    kind: 'Agent' as const,
    name: 'Agent',
    owner_id: owner.id,
    is_mine: true,
    is_me: false,
  };
  return {
    ...original,
    conversations: [
      {
        ...original.conversations[0]!,
        id: 'private-dm',
        kind: 'Direct',
        counterpart: agent,
        members: [owner, agent],
      },
    ],
    requests: [
      {
        id: 'request',
        agent,
        space_id: 'private-dm',
        space_title: 'Agent',
        title: 'Create image',
        detail: 'Sky',
        audience: 'This chat',
        action_label: 'Edit',
        cost_cents: null,
        reason: '',
        status: 'Pending',
        opened_ms: '1',
        resolved_ms: null,
        item_id: null,
        line_id: null,
        action_key: '',
        can_always_approve: false,
        by_standing: false,
        tool_approval: {
          content_hash: 'review-hash',
          artifact_id: 'image',
          artifact_title: 'Image',
          version: 1,
          operation: 'generate_image',
          expires_ms: '200000',
          image: {
            prompt: 'Sky',
            size: '1024x1024',
            quality: 'low',
            background: 'opaque',
            definition_version: 'image-v1',
            price_digest: 'a'.repeat(64),
            max_image_bytes: '100000',
            source_id: 'message',
            source_sha256: 'b'.repeat(64),
          },
        },
      },
    ],
  };
}
async function fixture() {
  let current = snapshot();
  const driver = new Driver(),
    calls: string[] = [];
  driver.opening = async () => current;
  driver.operation = async (command) => {
    calls.push(command.op);
    return command.op === 'snapshot' ? current : null;
  };
  const store = new ClientStore(driver);
  await store.open('account');
  return {
    store,
    driver,
    calls,
    replace(value: Snapshot) {
      current = value;
    },
    snapshot: () => current,
    port: createOwnerPolicyPort(store, target),
  };
}

test('production port uses exact private DM source and refuses group without rerouting', async () => {
  const f = await fixture(),
    scope = f.port.scope();
  assert.ok(scope);
  assert.equal(scope.requestId, 'request');
  const grouped = {
    ...f.snapshot(),
    conversations: f
      .snapshot()
      .conversations.map((space) => ({ ...space, kind: 'Group' as const, counterpart: null })),
  };
  f.replace(grouped);
  await f.store.refresh();
  assert.equal(policyAvailability(f.store, target).reason, 'private-conversation-required');
  const before = f.calls.length;
  await assert.rejects(f.port.inquire(scope));
  assert.equal(f.calls.length, before, 'no query in this or another conversation');
  await f.store.close();
});
test('native read completing after request hash change is discarded by the real store port', async () => {
  const f = await fixture(),
    scope = f.port.scope();
  assert.ok(scope);
  const gate = deferred<unknown>();
  f.driver.operation = async (command) => (command.op === 'snapshot' ? f.snapshot() : gate.promise);
  const pending = f.port.inquire(scope);
  const changed = {
    ...f.snapshot(),
    requests: f
      .snapshot()
      .requests.map((request) => ({
        ...request,
        tool_approval: request.tool_approval
          ? { ...request.tool_approval, content_hash: 'replacement' }
          : null,
      })),
  };
  f.replace(changed);
  await f.store.refresh();
  gate.resolve({ source: 'old-query' });
  await assert.rejects(pending);
  assert.equal(policyAvailability(f.store, target).reason, 'request-changed');
  await f.store.close();
});
test('store preflight/epoch replacement fences held native work before another account can adopt it', async () => {
  const f = await fixture(),
    scope = f.port.scope();
  assert.ok(scope);
  const gate = deferred<unknown>();
  f.driver.operation = async (command) => (command.op === 'snapshot' ? f.snapshot() : gate.promise);
  const pending = f.port.find(scope, 'c'.repeat(64));
  await f.store.open('demo');
  await tick();
  gate.resolve(null);
  await assert.rejects(pending);
  assert.equal(f.port.scope(), null);
  await f.store.close();
});
