import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import type { DecideOutcome } from '../core/generated/DecideOutcome';
import type { TimelineEntry } from '../core/generated/TimelineEntry';
import type { Command } from '../core/generated/Command';
import { requestDecisionCommand, toolApprovalValid } from '../core/tool-request-decision.ts';

export type EditorAgentHead = Readonly<{ version: number; sha256: string }>;
export type EditorAgentDecision = Extract<Command, { op: 'decide_tool_request' }>;
export type EditorAgentPort = Readonly<{
  scope: () => string;
  flush: () => Promise<void>;
  head: () => Promise<EditorAgentHead>;
  requests: () => Promise<AgentRequestDto[]>;
  send: (
    text: string,
    head: EditorAgentHead,
    id: string,
  ) => Promise<{
    entry: TimelineEntry | null;
    conflict: string | null;
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
  sent: TimelineEntry | null;
  requests: readonly AgentRequestDto[];
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
    sent: null,
    requests: [],
  };
  private listeners = new Set<() => void>();
  private generation = 0;
  private retired = false;
  private attempt: { text: string; head: EditorAgentHead; id: string } | null = null;
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
  };
  close = () => {
    this.generation++;
    this.update({ visible: false, loading: false });
  };
  retire = () => {
    this.retired = true;
    this.generation++;
    this.attempt = null;
    this.update({
      visible: false,
      draft: '',
      busy: false,
      loading: false,
      uncertain: false,
      sent: null,
      requests: [],
      error: null,
    });
  };
  setDraft = (draft: string) => {
    if (!this.retired && !this.state.busy && !this.attempt) this.update({ draft });
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
        const requests = await this.port.requests();
        this.check(ticket);
        this.update({
          requests: requests.filter(
            (request) =>
              request.space_id === this.space &&
              request.agent.is_mine &&
              (request.status === 'Pending' || request.status === 'Stale') &&
              request.tool_approval?.artifact_id === this.item &&
              toolApprovalValid(request.tool_approval),
          ),
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
  send = async (): Promise<void> => {
    if (this.state.busy || !this.state.draft.trim()) return;
    if (
      this.state.draft.includes('\0') ||
      new TextEncoder().encode(this.state.draft.trim()).length > 16 * 1024
    ) {
      this.update({ error: 'editor_agent_invalid_prompt' });
      return;
    }
    const ticket = this.generation,
      text = this.state.draft.trim();
    this.update({ busy: true, error: null });
    let submitted = false;
    try {
      this.check(ticket);
      await this.port.flush();
      this.check(ticket); // deliberately without retire
      if (!this.attempt) {
        const head = await this.port.head();
        this.check(ticket);
        this.attempt = { text, head, id: this.port.id() };
      }
      const attempt = this.attempt;
      submitted = true;
      const result = await this.port.send(attempt.text, attempt.head, attempt.id);
      if (this.retired || this.port.scope() !== this.owner) return;
      if (!result.entry && result.conflict) {
        this.attempt = null;
        this.update({ uncertain: false, error: result.conflict });
        return;
      }
      if (!result.entry || result.conflict !== null) throw Error('editor_agent_invalid_result');
      this.attempt = null;
      this.update({ draft: '', uncertain: false, sent: result.entry });
      if (this.state.visible) void this.refresh();
    } catch (error) {
      if (!this.retired && this.port.scope() === this.owner)
        this.update({
          uncertain: submitted,
          error: error instanceof Error ? error.message : 'editor_agent_send_failed',
        });
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
