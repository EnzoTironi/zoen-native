import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { Command } from './generated/Command';
import type { RequestDecision } from './generated/RequestDecision';
import type { ToolApprovalDto } from './generated/ToolApprovalDto';
import { imageToolReviewValid } from './image-tool-review.ts';

export type RequestCardDecision = RequestDecision | 'ApproveOnce' | 'Cancel';
export type DecisionCommand = Extract<Command, { op: 'decide_request' | 'decide_tool_request' }>;
type DecisionRequest = Pick<
  AgentRequestDto,
  'id' | 'status' | 'tool_approval' | 'can_always_approve'
> & { agent: Pick<AgentRequestDto['agent'], 'is_mine'> };

/** Presentation checks never grant authority; the core checks the signed request again. */
export function conversationSearchReview(tool: ToolApprovalDto): boolean {
  return (
    tool?.canonical_tool === 'search_conversation_history' &&
    tool.preview_kind === 'conversation' &&
    tool.artifact_id === '' &&
    tool.version === 0
  );
}

export function toolApprovalValid(tool: ToolApprovalDto): boolean {
  return (
    typeof tool === 'object' &&
    tool !== null &&
    typeof tool.content_hash === 'string' &&
    /^[a-f0-9]{64}$/.test(tool.content_hash) &&
    typeof tool.artifact_id === 'string' &&
    (tool.artifact_id.length > 0 || conversationSearchReview(tool)) &&
    typeof tool.artifact_title === 'string' &&
    tool.artifact_title.length > 0 &&
    typeof tool.operation === 'string' &&
    tool.operation.length > 0 &&
    Number.isSafeInteger(tool.version) &&
    (tool.version > 0 || conversationSearchReview(tool)) &&
    (tool.canonical_tool !== 'search_conversation_history' || conversationSearchReview(tool)) &&
    typeof tool.expires_ms === 'string' &&
    /^[0-9]{1,19}$/.test(tool.expires_ms) &&
    BigInt(tool.expires_ms) <= 8_640_000_000_000_000n
  );
}

export function toolApprovalExpired(tool: ToolApprovalDto, nowMs = Date.now()): boolean {
  return !toolApprovalValid(tool) || BigInt(tool.expires_ms) <= BigInt(Math.trunc(nowMs));
}

/** Keep tool approvals out of generic or standing decisions, including stale UI handlers. */
export function requestDecisionCommand(
  request: DecisionRequest,
  decision: RequestCardDecision,
  nowMs = Date.now(),
): DecisionCommand | null {
  if (!request.agent.is_mine || (request.status !== 'Pending' && request.status !== 'Stale'))
    return null;
  const tool = request.tool_approval;
  if (tool != null) {
    if (!toolApprovalValid(tool)) return null;
    if (decision !== 'ApproveOnce' && decision !== 'Deny' && decision !== 'Cancel') return null;
    if (
      decision === 'ApproveOnce' &&
      (request.status !== 'Pending' ||
        toolApprovalExpired(tool, nowMs) ||
        (tool.canonical_tool === 'generate_image' && !imageToolReviewValid(tool.image)))
    )
      return null;
    return {
      op: 'decide_tool_request',
      request_id: request.id,
      observed_content_hash: tool.content_hash,
      decision,
    };
  }
  if (decision === 'ApproveOnce' || decision === 'Cancel') return null;
  if (
    (decision === 'Approve' || decision === 'AlwaysApprove') &&
    (request.status === 'Stale' || (decision === 'AlwaysApprove' && !request.can_always_approve))
  )
    return null;
  return { op: 'decide_request', request_id: request.id, decision };
}
