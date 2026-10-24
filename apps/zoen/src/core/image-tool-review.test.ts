import assert from 'node:assert/strict';
import { test } from 'node:test';
import { imageToolReviewValid } from './image-tool-review.ts';
import { requestDecisionCommand } from './tool-request-decision.ts';
import type { ToolApprovalDto } from './generated/ToolApprovalDto';

const image = {
  prompt: 'A quiet blue sky',
  size: '1024x1024',
  quality: 'low',
  background: 'opaque',
  definition_version: 'image-profile/1',
  price_digest: 'ab'.repeat(32),
  max_image_bytes: '12582912',
  source_id: 'original-owner-message',
  source_sha256: 'cd'.repeat(32),
};
const now = 1_800_000_000_000;
const tool: ToolApprovalDto = {
  canonical_tool: 'generate_image',
  image,
  artifact_title: 'Image',
  artifact_id: 'prospective-image',
  content_hash: 'ef'.repeat(32),
  operation: 'Create image',
  version: 1,
  expires_ms: String(now + 60_000),
};
function request(approval = tool) {
  return {
    id: 'image-request',
    status: 'Pending' as const,
    agent: { is_mine: true },
    tool_approval: approval,
    can_always_approve: false,
  };
}

test('the exact image review permits only its observed once decision', () => {
  assert.equal(imageToolReviewValid(image), true);
  assert.deepEqual(requestDecisionCommand(request(), 'ApproveOnce', now), {
    op: 'decide_tool_request',
    request_id: 'image-request',
    observed_content_hash: tool.content_hash,
    decision: 'ApproveOnce',
  });
  for (const decision of ['Approve', 'AlwaysApprove', 'AlwaysDeny'] as const)
    assert.equal(requestDecisionCommand(request(), decision, now), null);
});

test('missing or unsupported generation facts refuse approval without blocking cancel or deny', () => {
  for (const invalid of [
    undefined,
    null,
    { ...image, prompt: ' ' },
    { ...image, prompt: '\0' },
    { ...image, prompt: '😀'.repeat(32_001) },
    { ...image, size: 'auto' },
    { ...image, quality: 'automatic' },
    { ...image, background: 'automatic' },
    { ...image, max_image_bytes: '0' },
    { ...image, max_image_bytes: '12582913' },
    { ...image, max_image_bytes: 12582912 },
    { ...image, max_image_bytes: '-1' },
    { ...image, max_image_bytes: '9223372036854775807' },
    { ...image, source_id: '' },
    { ...image, source_sha256: 'invalid' },
    { ...image, price_digest: '' },
    { ...image, definition_version: '../not a version' },
  ]) {
    assert.equal(imageToolReviewValid(invalid), false);
    const changed = request({ ...tool, image: invalid } as ToolApprovalDto);
    assert.equal(requestDecisionCommand(changed, 'ApproveOnce', now), null);
    for (const decision of ['Cancel', 'Deny'] as const)
      assert.equal(requestDecisionCommand(changed, decision, now)?.decision, decision);
  }
});

test('a previously valid image cannot approve after expiry, scope change or resolution', () => {
  assert.equal(
    requestDecisionCommand(request({ ...tool, expires_ms: String(now) }), 'ApproveOnce', now),
    null,
  );
  assert.equal(
    requestDecisionCommand({ ...request(), agent: { is_mine: false } }, 'ApproveOnce', now),
    null,
  );
  for (const status of ['Stale', 'Approved', 'Denied', 'Cancelled'] as const)
    assert.equal(requestDecisionCommand({ ...request(), status }, 'ApproveOnce', now), null);
  assert.equal(imageToolReviewValid({ ...image, prompt: '😀'.repeat(32_000) }), true);
});
