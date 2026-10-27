import assert from 'node:assert/strict';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { drawingReply, drawingReview } from './drawing-review.fixtures.ts';
import { drawingReviewPort } from '../miniapps/drawing-review-port.ts';
import { CanonicalSceneHost } from '../miniapps/canonical-scene.ts';
import { canonicalRefresh } from '../miniapps/canonical-refresh.ts';
import { editorAgentMembers } from './editor-agent-members.fixtures.ts';
import { editorAgentTargets } from './editor-agent-targets.fixtures.ts';
import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { ItemDetail } from './types';
import type { MiniappDocumentController } from '../miniapps/document-controller';

/** Explicit Core and renderer boundaries. They reject a wrong request/hash,
 * artifact, pin or base instead of promoting arbitrary input into a preview. */
export async function appRequestReviewState(options: { canonicalFlush?: boolean } = {}) {
  const driver = new Driver(),
    snapshot = draftSession();
  const room = snapshot.conversations[0]!,
    space = room.id,
    itemId = 'review-item';
  const members = editorAgentMembers(snapshot, space);
  const targets = editorAgentTargets(snapshot, space, members, () => changed());
  const owner = room.members[0]!;
  const agent = {
    ...owner,
    id: members.id,
    name: 'Review Agent',
    kind: 'Agent' as const,
    is_me: false,
    is_mine: true,
    owner_id: snapshot.account!.identity_id,
  };
  const review = drawingReview(space, itemId);
  const item: ItemDetail = {
    id: itemId,
    space_id: space,
    space_title: room.title,
    title: 'Owned scene',
    kind_id: 'app',
    kind_label: 'App',
    origin: '',
    created_by: owner,
    plan: null,
    text: null,
    version: 1,
    versions: [],
    linked_requests: [],
    path: '',
    file: null,
    app: {
      app_id: 'independent-scene-editor',
      name: 'Scene App',
      resource_uri: review.resource_uri,
      view_json: '{}',
      headline: '',
      metrics: [],
      last_action: null,
      trust: 'Act',
      snapshot_json: '',
    },
  };
  const request: AgentRequestDto = {
    id: review.request_id,
    agent,
    space_id: space,
    space_title: room.title,
    title: 'Edit existing scene',
    detail: 'A frozen proposed change',
    audience: 'This chat',
    action_label: 'Edit scene',
    cost_cents: null,
    reason: '',
    opened_ms: '1',
    resolved_ms: null,
    status: 'Pending',
    item_id: itemId,
    line_id: null,
    action_key: '',
    can_always_approve: false,
    by_standing: false,
    tool_approval: {
      artifact_id: itemId,
      artifact_title: item.title,
      version: 1,
      operation: 'Replace scene',
      content_hash: review.content_hash,
      expires_ms: String(Date.now() + 600_000),
      preview_kind: 'scene',
      canonical_tool: 'replace_scene',
      review: {
        state: 'Unavailable',
        root_request_id: review.request_id,
        predecessor_request_id: null,
        successor_request_id: null,
      },
    },
  };
  const sibling: AgentRequestDto = structuredClone(request);
  sibling.id = 'sibling-request';
  sibling.title = 'Another existing change';
  sibling.tool_approval!.content_hash = 'aa'.repeat(32);
  sibling.tool_approval!.review!.root_request_id = sibling.id;
  let requests = [request, sibling],
    allowed = true,
    ready = true;
  let requestsGate: Promise<void> | null = null,
    itemGate: Promise<void> | null = null;
  let headGate: Promise<void> | null = null,
    renderGate: Promise<void> | null = null;
  let flushGate: Promise<void> | null = null;
  const calls: any[] = [],
    renders: any[] = [],
    listeners = new Set<() => void>();
  const current = (id: string, hash: string) => {
    const matches = requests.filter((r) => r.id === id);
    assert.equal(matches.length, 1, 'one existing request');
    const value = matches[0]!;
    assert.equal(value.tool_approval?.content_hash, hash);
    assert.equal(value.space_id, space);
    assert.equal(value.tool_approval?.artifact_id, itemId);
    assert.equal(value.agent.owner_id, snapshot.account!.identity_id);
    assert.equal(value.agent.is_mine, true);
    assert.ok(allowed && snapshot.conversations.some((c) => c.id === space));
    return value;
  };
  const checkPin = (pin: unknown) => {
    assert.ok(allowed && item.app);
    assert.equal(pin, review.resource_uri);
    assert.equal(item.app.resource_uri, review.resource_uri);
  };
  const changed = () => driver.changed?.({ kind: 'change', spaceIds: [space] });
  driver.opening = async () => snapshot;
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (
      [
        'request_app_edit',
        'add_member_reviewed',
        'set_trust',
        'agent_enrollment_approve',
        'agent_enrollment_dismiss',
      ].includes(command.op)
    )
      assert.fail(`Review cannot send, enroll or change a grant: ${command.op}`);
    if (members.handles(command.op)) return members.handle(command);
    if (targets.handles(command.op))
      return targets.handle(command, item.version, review.before_sha256, review.resource_uri);
    switch (command.op) {
      case 'snapshot':
        return snapshot;
      case 'requests':
        if (requestsGate) await requestsGate;
        return structuredClone(requests);
      case 'item':
        if (itemGate) await itemGate;
        assert.equal(command.item_id, itemId);
        assert.ok(allowed && snapshot.conversations.some((c) => c.id === space));
        return structuredClone(item);
      case 'app_specs':
        return [
          { id: item.app!.app_id, has_view: true, tools: [], resource_uri: review.resource_uri },
        ];
      case 'app_document_read_pinned':
        if (headGate) await headGate;
        assert.equal(command.item_id, itemId);
        checkPin(command.expected_resource_uri);
        return {
          item_id: itemId,
          version: item.version,
          current_version: item.version,
          sha256: review.before_sha256,
          scene_json: review.before_scene_json,
        };
      case 'scene_tool_review': {
        const q = command.input;
        assert.equal(q.item_id, itemId);
        assert.equal(q.space_id, space);
        assert.equal(q.expected_identity_id, snapshot.account!.identity_id);
        assert.equal(q.expected_device_id, snapshot.account!.device_id);
        checkPin(q.expected_resource_uri);
        const value = current(q.request_id, q.expected_hash);
        assert.equal(value.status, 'Pending');
        assert.ok(Number(value.tool_approval!.expires_ms) > Date.now());
        assert.equal(value.tool_approval!.version, item.version);
        return { ...review, request_id: q.request_id, content_hash: q.expected_hash };
      }
      case 'decide_tool_request': {
        const value = current(command.request_id, command.observed_content_hash);
        if (command.decision === 'RequestReview') {
          assert.ok(Number(value.tool_approval!.expires_ms) <= Date.now());
          assert.equal(value.tool_approval!.review!.state, 'Available');
          value.tool_approval!.review!.state = 'Requested';
        } else if (command.decision === 'ApproveOnce') {
          assert.equal(value.status, 'Pending');
          assert.ok(Number(value.tool_approval!.expires_ms) > Date.now());
          value.status = 'Approved';
        } else assert.ok(['Deny', 'Cancel'].includes(command.decision));
        return { message: 'Recorded separate decision', also_resolved: 0 };
      }
      case 'timeline':
        return { entries: [], before: null, has_more: false };
      case 'thread':
        return [];
      case 'request_app_edit':
        assert.fail('Reviewing an existing request cannot Send');
      default:
        assert.fail(`Unexpected review Core operation ${command.op}`);
    }
  };
  const store = new ClientStore(driver);
  await store.open('account');
  calls.length = 0;
  let ids = 0;
  const doc = () => ({
    item_id: itemId,
    version: item.version,
    current_version: item.version,
    sha256: review.before_sha256,
    scene_json: review.before_scene_json,
  });
  const scene = new CanonicalSceneHost({
    initialDoc: doc(),
    check: () => checkPin(review.resource_uri),
    id: () => `scene-${++ids}`,
    save: async () => assert.fail('An unchanged review flush cannot save a document'),
  });
  const canonical = canonicalRefresh({
    itemId,
    scene,
    check: () => checkPin(review.resource_uri),
    read: async () => doc(),
    call: async () => ({ content: [], structuredContent: {} }),
    conflict: () => assert.fail('Same-head flush cannot introduce conflict'),
  });
  const readiness: boolean[] = [];
  canonical.readiness.subscribe(() => readiness.push(canonical.readiness.getSnapshot()));
  const preview = drawingReviewPort({
    store,
    pin: { itemId, resourceUri: review.resource_uri, appId: item.app!.app_id },
    spaceId: space,
    key: 'current-owned-frame',
    id: () => `preview-${++ids}`,
    check: () => checkPin(review.resource_uri),
    call: async (name, args) => {
      assert.equal(name, 'render_scene_review');
      const value = current(String(args.requestId), String(args.contentHash));
      assert.equal(args.artifactId, itemId);
      assert.equal(args.resourceUri, review.resource_uri);
      assert.equal(args.baseVersion, value.tool_approval!.version);
      renders.push(structuredClone(args));
      if (renderGate) await renderGate;
      return drawingReply(args);
    },
  });
  const publishReady = () => {
    const value = {
      zoen: {
        sceneSave: {
          itemId,
          ready,
          reviewPreview: 1,
          clean: true,
          phase: 'saved',
          liveRefresh: 1,
          version: item.version,
          sha256: review.before_sha256,
        },
      },
    };
    preview.observe(value);
    canonical.observe(value);
  };
  publishReady();
  const document: MiniappDocumentController = {
    agentDocument: { kind: 'owned-scene', resourceUri: review.resource_uri },
    drawingReview: preview,
    agentReadiness: options.canonicalFlush
      ? canonical.readiness
      : {
          getSnapshot: () => ready,
          subscribe: (f) => {
            listeners.add(f);
            return () => {
              listeners.delete(f);
            };
          },
        },
    observedHead: async () => ({ itemId, version: item.version, sha256: review.before_sha256 }),
    flush: options.canonicalFlush
      ? async () => {
          // Same production controller order as canonical-view: readonly refresh,
          // then the real barrier/publish around the native frame flush operation.
          await canonical.refresh();
          await canonical.barrier(async () => {
            if (flushGate) await flushGate;
            else await Promise.resolve();
          });
        }
      : async () => {},
  };
  return {
    store,
    snapshot,
    item,
    request,
    sibling,
    review,
    document,
    calls,
    renders,
    readiness,
    changed,
    members,
    targets,
    requests(value: AgentRequestDto[]) {
      requests = value;
      changed();
    },
    holdRequests(value: Promise<void> | null) {
      requestsGate = value;
    },
    holdItem(value: Promise<void> | null) {
      itemGate = value;
    },
    holdHead(value: Promise<void> | null) {
      headGate = value;
    },
    holdRender(value: Promise<void> | null) {
      renderGate = value;
    },
    holdFlush(value: Promise<void> | null) {
      flushGate = value;
    },
    ready(value: boolean) {
      ready = value;
      publishReady();
      for (const f of listeners) f();
    },
    invalidate() {
      allowed = false;
      changed();
    },
    allow(value: boolean) {
      allowed = value;
      publishReady();
      changed();
    },
    async revoke() {
      snapshot.conversations = [];
      await store.refresh();
    },
    async reopen(change?: 'device' | 'identity') {
      await store.close({ force: true });
      if (change === 'device') snapshot.account!.device_id = 'e'.repeat(64);
      if (change === 'identity') snapshot.account!.identity_id = 'f'.repeat(64);
      await store.open('account');
    },
    async close() {
      preview.dispose();
      canonical.dispose();
      scene.dispose();
      await store.close({ force: true });
    },
  };
}
