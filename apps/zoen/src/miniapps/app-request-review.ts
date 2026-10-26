import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import type { ClientStore } from '../core/store';
import { accountScope, accountScopeKey } from '../core/account-scope.ts';
import { homeSpacesFor } from '../home/access.ts';
import { toolApprovalExpired, toolApprovalValid } from '../core/tool-request-decision.ts';
import { isOwnedAppResource } from './canonical-resource.ts';

/** A navigation intent, never an approval or an execution capability. */
export type AppRequestReviewTarget = Readonly<{
  requestId: string;
  contentHash: string;
  scope: string;
}>;
const id = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
const digest = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);

export function parseAppRequestReview(params: {
  reviewRequest?: unknown;
  reviewHash?: unknown;
  reviewScope?: unknown;
}): AppRequestReviewTarget | null {
  return id(params.reviewRequest) &&
    digest(params.reviewHash) &&
    typeof params.reviewScope === 'string' &&
    params.reviewScope.length > 0 &&
    params.reviewScope.length <= 2048 &&
    [...params.reviewScope].every((character) => character.charCodeAt(0) >= 32)
    ? { requestId: params.reviewRequest, contentHash: params.reviewHash, scope: params.reviewScope }
    : null;
}

export function appRequestReviewTarget(
  request: AgentRequestDto,
  scope: string,
): AppRequestReviewTarget | null {
  const tool = request.tool_approval;
  return scope &&
    id(request.id) &&
    request.agent.is_mine &&
    request.status === 'Pending' &&
    tool &&
    toolApprovalValid(tool) &&
    !toolApprovalExpired(tool) &&
    (tool.preview_kind === 'scene' || tool.canonical_tool === 'replace_scene')
    ? { requestId: request.id, contentHash: tool.content_hash, scope }
    : null;
}

export function matchesAppRequestReview(
  request: AgentRequestDto,
  target: AppRequestReviewTarget,
  itemId: string,
  spaceId: string,
): boolean {
  const current = appRequestReviewTarget(request, target.scope);
  return (
    !!current &&
    current.requestId === target.requestId &&
    current.contentHash === target.contentHash &&
    request.space_id === spaceId &&
    request.tool_approval?.artifact_id === itemId
  );
}

/** Re-read normal Core views before routing; an old/foreign link stays inert. */
export async function resolveAppRequestReview(
  store: ClientStore,
  target: AppRequestReviewTarget,
  itemId: string,
  check: () => void,
) {
  const current = () => {
    check();
    if (!target.scope || accountScopeKey(accountScope(store)) !== target.scope)
      throw Error('app_review_scope_changed');
  };
  current();
  const requests = await store.driver.request({ op: 'requests' });
  current();
  const request = requests.find((value) => value.id === target.requestId);
  if (!request || !matchesAppRequestReview(request, target, itemId, request.space_id))
    throw Error('app_review_request_changed');
  const item = await store.driver.request({ op: 'item', item_id: itemId });
  current();
  if (
    item.id !== itemId ||
    item.space_id !== request.space_id ||
    !homeSpacesFor(store.getSnapshot()).has(item.space_id) ||
    !item.app ||
    !isOwnedAppResource(item.app.resource_uri) ||
    item.version !== request.tool_approval?.version
  )
    throw Error('app_review_item_changed');
  return { request, item };
}
