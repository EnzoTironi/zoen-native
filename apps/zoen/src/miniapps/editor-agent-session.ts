import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import type { DecideOutcome } from '../core/generated/DecideOutcome';
import type { TimelineEntry } from '../core/generated/TimelineEntry';
import type { Command } from '../core/generated/Command';
import type { AppEditConflictKind } from '../core/generated/AppEditConflictKind';
import type { AppEditTargets } from '../core/generated/AppEditTargets';
import type { Persona } from '../core/generated/Persona';
import { requestDecisionCommand, toolApprovalValid } from '../core/tool-request-decision.ts';

export type EditorAgentTicket = Readonly<{ owner: string; generation: number }>;
export type EditorAgentHead = Readonly<{ version: number; sha256: string }>;
export type EditorAgentTarget = Readonly<{
  agent: Persona;
  head: EditorAgentHead;
  revision: string;
}>;
export type EditorAgentTargetContext = Readonly<{ head: EditorAgentHead; targets: AppEditTargets }>;
export type EditorAgentDecision = Extract<Command, { op: 'decide_tool_request' }>;
export type EditorAgentPort = Readonly<{
  scope: () => string;
  flush: () => Promise<void>;
  head: () => Promise<EditorAgentHead>;
  targets: () => Promise<EditorAgentTargetContext>;
  requests: () => Promise<AgentRequestDto[]>;
  /** The original request and its replies, including its current delivery state. */
  replies: (sourceId: string) => Promise<TimelineEntry[]>;
  send: (
    text: string,
    head: EditorAgentHead,
    id: string,
    target: EditorAgentTarget,
  ) => Promise<{
    entry: TimelineEntry | null;
    conflict: string | null;
    conflict_kind?: AppEditConflictKind;
  }>;
  decide: (command: EditorAgentDecision) => Promise<DecideOutcome>;
  id: () => string;
}>;
export type EditorAgentState = Readonly<{
  visible: boolean;
  draft: string;
  busy: boolean;
  loading: boolean;
  uncertain: boolean;
  error: string | null;
  conflictKind: AppEditConflictKind | null;
  sent: TimelineEntry | null;
  requests: readonly AgentRequestDto[];
  replies: readonly TimelineEntry[];
  targetPhase: 'idle' | 'loading' | 'ready' | 'failed';
  targets: AppEditTargets | null;
  targetHead: EditorAgentHead | null;
  target: EditorAgentTarget | null;
}>;

/** One mounted document owns this overlay. Closing it never retires the editor. */
export class EditorAgentSession {
  private state: EditorAgentState = {
    visible: false,
    draft: '',
    busy: false,
    loading: false,
    uncertain: false,
    error: null,
    conflictKind: null,
    sent: null,
    requests: [],
    replies: [],
    targetPhase: 'idle',
    targets: null,
    targetHead: null,
    target: null,
  };
  private listeners = new Set<() => void>();
  private generation = 0;
  private retired = false;
  private attempt: {
    text: string;
    head: EditorAgentHead;
    id: string;
    target: EditorAgentTarget;
  } | null = null;
  private targetReading = 0;
  private refreshing: Promise<void> | null = null;
  private refreshAgain = false;
  private readonly port: EditorAgentPort;
  private readonly owner: string;
  private readonly item: string;
  private readonly space: string;
  constructor(port: EditorAgentPort, owner: string, item: string, space: string) {
    this.port = port;
    this.owner = owner;
    this.item = item;
    this.space = space;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private update(patch: Partial<EditorAgentState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  ticket(observed = this.state): EditorAgentTicket | null {
    return observed === this.state &&
      this.state.visible &&
      !this.retired &&
      this.owner &&
      this.port.scope() === this.owner
      ? { owner: this.owner, generation: this.generation }
      : null;
  }
  accepts(ticket: EditorAgentTicket | null): boolean {
    return Boolean(
      ticket &&
      !this.retired &&
      this.state.visible &&
      ticket.owner === this.owner &&
      this.port.scope() === this.owner &&
      ticket.generation === this.generation,
    );
  }
  renewTicket = () => {
    if (!this.accepts(this.ticket())) return;
    this.generation++;
    this.update({});
  };
  private check(ticket = this.generation) {
    if (
      this.retired ||
      !this.owner ||
      this.port.scope() !== this.owner ||
      ticket !== this.generation ||
      !this.state.visible
    )
      throw Error('editor_agent_owner_changed');
  }
  activate = () => {
    if (this.owner && this.port.scope() === this.owner) this.retired = false;
  };
  open = () => {
    if (this.retired || !this.owner || this.port.scope() !== this.owner) return;
    this.generation++;
    this.update({ visible: true, error: null });
    void this.refresh();
    void this.refreshTargets();
  };
  close = () => {
    this.generation++;
    this.targetReading++;
    this.update({ visible: false, loading: false });
  };
  retire = () => {
    this.retired = true;
    this.generation++;
    this.targetReading++;
    this.attempt = null;
    this.update({
      visible: false,
      draft: '',
      busy: false,
      loading: false,
      uncertain: false,
      sent: null,
      requests: [],
      replies: [],
      error: null,
      conflictKind: null,
      targetPhase: 'idle',
      targets: null,
      targetHead: null,
      target: null,
    });
  };
  setDraft = (draft: string, ticket: EditorAgentTicket | null = this.ticket()) => {
    if (this.accepts(ticket) && !this.retired && !this.state.busy && !this.attempt)
      this.update({ draft });
  };
  invalidateTargets = () => {
    if (this.retired) return;
    this.generation++;
    this.targetReading++;
    this.update({
      targetPhase: 'idle',
      targets: null,
      targetHead: null,
      // Keep a presentation choice while revalidating; it becomes usable only
      // after Core returns the same revision and displayed head.
      target: this.attempt?.target ?? this.state.target,
    });
  };
  refreshTargets = async () => {
    if (!this.state.visible || this.retired) return;
    const ticket = this.generation,
      reading = ++this.targetReading;
    this.update({ targetPhase: 'loading' });
    try {
      const context = await this.port.targets();
      this.check(ticket);
      if (reading !== this.targetReading) return;
      const prior = this.state.target;
      const retained =
        prior &&
        prior.revision === context.targets.revision &&
        prior.head.version === context.head.version &&
        prior.head.sha256 === context.head.sha256
          ? context.targets.agents.find(
              (candidate) => candidate.eligible && candidate.agent.id === prior.agent.id,
            )?.agent
          : undefined;
      const eligible = context.targets.agents.filter((candidate) => candidate.eligible);
      const agent = eligible.length === 1 ? eligible[0]?.agent : retained;
      if (this.state.targets && this.state.targets.revision !== context.targets.revision)
        this.generation++;
      this.update({
        targetPhase: 'ready',
        targets: context.targets,
        targetHead: context.head,
        target:
          this.attempt?.target ??
          (agent ? { agent, head: context.head, revision: context.targets.revision } : null),
      });
    } catch {
      if (!this.retired && this.port.scope() === this.owner && reading === this.targetReading)
        this.update({
          targetPhase: 'failed',
          targets: null,
          targetHead: null,
          target: this.attempt?.target ?? null,
        });
    }
  };
  selectAgent = (agentId: string, revision: string, ticket: EditorAgentTicket | null) => {
    if (
      !this.accepts(ticket) ||
      this.state.busy ||
      this.attempt ||
      this.state.targetPhase !== 'ready' ||
      !this.state.targetHead ||
      this.state.targets?.revision !== revision
    )
      return;
    const agent = this.state.targets.agents.find(
      (candidate) => candidate.eligible && candidate.agent.id === agentId,
    )?.agent;
    if (!agent) return;
    if (this.state.target?.agent.id === agentId && this.state.target.revision === revision) return;
    this.generation++;
    this.update({ target: { agent, head: this.state.targetHead, revision } });
  };
  refresh = (): Promise<void> => {
    if (!this.state.visible || this.retired) return Promise.resolve();
    if (this.refreshing) {
      this.refreshAgain = true;
      return this.refreshing;
    }
    const ticket = this.generation;
    this.update({ loading: true });
    const work = (async () => {
      try {
        this.check(ticket);
        const source = this.state.sent?.id;
        const [requests, replies] = await Promise.all([
          this.port.requests(),
          source ? this.port.replies(source) : Promise.resolve([]),
        ]);
        this.check(ticket);
        if (source !== this.state.sent?.id) this.refreshAgain = true;
        const original = replies.find((entry) => entry.id === source);
        const failedPrompt =
          original?.delivery === 'Failed' && !original.deleted && 'Message' in original.kind
            ? original.kind.Message.text
            : null;
        this.update({
          requests: requests.filter(
            (request) =>
              request.space_id === this.space &&
              request.agent.is_mine &&
              (request.status === 'Pending' || request.status === 'Stale') &&
              request.tool_approval?.artifact_id === this.item &&
              toolApprovalValid(request.tool_approval),
          ),
          ...(source === this.state.sent?.id
            ? {
                ...(original ? { sent: original } : {}),
                ...(failedPrompt !== null &&
                this.state.sent?.delivery !== 'Failed' &&
                !this.attempt &&
                !this.state.draft
                  ? { draft: failedPrompt }
                  : {}),
                replies: replies.filter(
                  (entry) =>
                    entry.reply_to?.id === source &&
                    entry.author.kind === 'Agent' &&
                    !entry.deleted &&
                    'Message' in entry.kind,
                ),
              }
            : {}),
          error: null,
        });
      } catch (error) {
        if (ticket === this.generation && this.state.visible && !this.retired)
          this.update({
            error: error instanceof Error ? error.message : 'editor_agent_read_failed',
          });
      } finally {
        if (ticket === this.generation && !this.retired) this.update({ loading: false });
      }
    })();
    this.refreshing = work;
    void work.finally(() => {
      if (this.refreshing === work) {
        this.refreshing = null;
        const again = this.refreshAgain || ticket !== this.generation;
        this.refreshAgain = false;
        if (again && this.state.visible && !this.retired) void this.refresh();
      }
    });
    return work;
  };
  send = async (observed: EditorAgentTicket | null = this.ticket()): Promise<void> => {
    if (!this.accepts(observed) || this.state.busy || !this.state.draft.trim()) return;
    const target = this.state.target;
    if (!this.attempt && (!target || this.state.targetPhase !== 'ready')) return;
    if (
      this.state.draft.includes('\0') ||
      new TextEncoder().encode(this.state.draft.trim()).length > 16 * 1024
    ) {
      this.update({ error: 'editor_agent_invalid_prompt' });
      return;
    }
    const ticket = this.generation,
      text = this.state.draft.trim();
    this.update({ busy: true, error: null, conflictKind: null });
    let submitted = false;
    try {
      this.check(ticket);
      await this.port.flush();
      this.check(ticket); // deliberately without retire
      if (!this.attempt) {
        const head = await this.port.head();
        this.check(ticket);
        if (!target || head.version !== target.head.version || head.sha256 !== target.head.sha256)
          throw Error('editor_agent_target_changed');
        this.attempt = { text, head, id: this.port.id(), target };
      }
      const attempt = this.attempt;
      submitted = true;
      const result = await this.port.send(attempt.text, attempt.head, attempt.id, attempt.target);
      if (this.retired || this.port.scope() !== this.owner) return;
      if (!result.entry && result.conflict) {
        this.attempt = null;
        this.update({
          uncertain: false,
          error: result.conflict,
          conflictKind: result.conflict_kind ?? 'Changed',
        });
        this.invalidateTargets();
        void this.refreshTargets();
        return;
      }
      if (!result.entry || result.conflict !== null) throw Error('editor_agent_invalid_result');
      this.attempt = null;
      this.update({ draft: '', uncertain: false, sent: result.entry, replies: [] });
      if (this.state.visible) void this.refresh();
    } catch (error) {
      if (!this.retired && this.port.scope() === this.owner)
        this.update({
          uncertain: submitted || this.attempt !== null,
          error: error instanceof Error ? error.message : 'editor_agent_send_failed',
        });
      if (!submitted && !this.attempt && !this.retired && this.port.scope() === this.owner) {
        this.invalidateTargets();
        void this.refreshTargets();
      }
    } finally {
      if (!this.retired) this.update({ busy: false });
    }
  };
  decide = async (command: EditorAgentDecision): Promise<DecideOutcome> => {
    const ticket = this.generation;
    this.check(ticket);
    const request = this.state.requests.find((r) => r.id === command.request_id);
    if (
      this.state.busy ||
      !request ||
      request.tool_approval?.content_hash !== command.observed_content_hash
    )
      throw Error('editor_agent_review_changed');
    this.update({ busy: true });
    try {
      await this.port.flush();
      this.check(ticket);
      const decision = command.decision;
      if (decision !== 'ApproveOnce' && decision !== 'Deny' && decision !== 'Cancel')
        throw Error('editor_agent_review_changed');
      const valid = requestDecisionCommand(request, decision);
      if (
        !valid ||
        valid.op !== 'decide_tool_request' ||
        valid.observed_content_hash !== command.observed_content_hash
      )
        throw Error('editor_agent_review_changed');
      const result = await this.port.decide(command);
      if (!this.retired && this.state.visible) void this.refresh();
      return result;
    } finally {
      if (!this.retired) this.update({ busy: false });
    }
  };
}
