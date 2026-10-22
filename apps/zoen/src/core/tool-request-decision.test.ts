import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  requestDecisionCommand,
  toolApprovalExpired,
  toolApprovalValid,
} from './tool-request-decision.ts';
import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { ToolApprovalDto } from './generated/ToolApprovalDto';

const now = 1_800_000_000_000;
const approval: ToolApprovalDto = {
  content_hash: 'ab'.repeat(32),
  artifact_id: 'synthetic-deck',
  artifact_title: 'Weekend plan',
  version: 7,
  operation: 'slide.notes',
  expires_ms: String(now + 60_000),
};
function request(
  status: AgentRequestDto['status'] = 'Pending',
  tool: ToolApprovalDto | null = approval,
) {
  return {
    id: 'synthetic-request',
    status,
    agent: { is_mine: true },
    can_always_approve: true,
    tool_approval: tool,
  };
}

test('tool decision binds the observed exact hash and never falls into standing or generic approval', () => {
  for (const decision of ['ApproveOnce', 'Deny', 'Cancel'] as const) {
    assert.deepEqual(requestDecisionCommand(request(), decision, now), {
      op: 'decide_tool_request',
      request_id: 'synthetic-request',
      observed_content_hash: approval.content_hash,
      decision,
    });
  }
  for (const decision of ['Approve', 'AlwaysApprove', 'AlwaysDeny'] as const) {
    assert.equal(requestDecisionCommand(request(), decision, now), null);
  }
  const changedHash = { ...approval, content_hash: 'cd'.repeat(32) };
  assert.deepEqual(requestDecisionCommand(request('Pending', changedHash), 'ApproveOnce', now), {
    op: 'decide_tool_request',
    request_id: 'synthetic-request',
    observed_content_hash: changedHash.content_hash,
    decision: 'ApproveOnce',
  });
});

test('expired or stale requests cannot approve; their owner can still refuse or cancel', () => {
  for (const value of [
    request('Stale'),
    request('Pending', { ...approval, expires_ms: String(now) }),
    request('Pending', { ...approval, expires_ms: String(now - 1) }),
  ]) {
    assert.equal(requestDecisionCommand(value, 'ApproveOnce', now), null);
    for (const decision of ['Deny', 'Cancel'] as const) {
      assert.equal(requestDecisionCommand(value, decision, now)?.op, 'decide_tool_request');
    }
  }
  assert.equal(toolApprovalExpired({ ...approval, expires_ms: String(now) }, now), true);
  assert.equal(toolApprovalExpired(approval, now), false);
});

test('resolved, non-owner and malformed requests never produce a decision command', () => {
  for (const status of ['Approved', 'Denied', 'Cancelled'] as const) {
    for (const decision of ['ApproveOnce', 'Deny', 'Cancel'] as const) {
      assert.equal(requestDecisionCommand(request(status), decision, now), null);
    }
  }
  assert.equal(
    requestDecisionCommand({ ...request(), agent: { is_mine: false } }, 'ApproveOnce', now),
    null,
  );
  for (const invalid of [
    { ...approval, content_hash: '' },
    { ...approval, content_hash: 'ab'.repeat(31) },
    { ...approval, artifact_id: '' },
    { ...approval, artifact_title: '' },
    { ...approval, operation: '' },
    { ...approval, version: 0 },
    { ...approval, version: 1.5 },
    { ...approval, expires_ms: '-1' },
    { ...approval, expires_ms: 'not-a-date' },
    { ...approval, expires_ms: '9223372036854775807' },
  ]) {
    assert.equal(toolApprovalValid(invalid), false);
    for (const decision of ['ApproveOnce', 'Deny', 'Cancel', 'AlwaysApprove'] as const) {
      assert.equal(requestDecisionCommand(request('Pending', invalid), decision, now), null);
    }
  }
});

test('ordinary requests retain their existing individual and standing decision behavior', () => {
  for (const decision of ['Approve', 'Deny', 'AlwaysApprove', 'AlwaysDeny'] as const) {
    assert.deepEqual(requestDecisionCommand(request('Pending', null), decision, now), {
      op: 'decide_request',
      request_id: 'synthetic-request',
      decision,
    });
  }
  for (const decision of ['ApproveOnce', 'Cancel'] as const) {
    assert.equal(requestDecisionCommand(request('Pending', null), decision, now), null);
  }
  assert.equal(requestDecisionCommand(request('Stale', null), 'Approve', now), null);
  assert.equal(requestDecisionCommand(request('Stale', null), 'AlwaysApprove', now), null);
  assert.equal(requestDecisionCommand(request('Stale', null), 'Deny', now)?.op, 'decide_request');
  assert.equal(
    requestDecisionCommand(
      { ...request('Pending', null), can_always_approve: false },
      'AlwaysApprove',
      now,
    ),
    null,
  );
});
