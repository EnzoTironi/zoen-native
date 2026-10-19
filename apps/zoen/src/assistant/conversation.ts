import type { AssistantPort, AssistantUnavailableReason } from './types.ts';

export type AssistantTurn = Readonly<{ role: 'user' | 'assistant'; text: string }>;
export type AssistantState = Readonly<{
  phase: 'opening' | 'ready' | 'asking' | 'unavailable' | 'failed' | 'closed';
  turns: readonly AssistantTurn[];
  pending: string | null;
  failed: boolean;
  reason?: AssistantUnavailableReason;
  contextMessages: number;
  limited: boolean;
}>;

/** This owner never writes a message, route query, profile or conversation to the core. */
export class EphemeralConversation {
  private state: AssistantState = {
    phase: 'opening',
    turns: [],
    pending: null,
    failed: false,
    contextMessages: 0,
    limited: true,
  };
  private listeners = new Set<() => void>();
  private id: string | null = null;
  private closed = false;
  private opening: Promise<void> | null = null;
  private closing: Promise<void> | null = null;
  private asking: Promise<boolean> | null = null;
  private readonly port: AssistantPort;
  constructor(port: AssistantPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private update(state: AssistantState) {
    this.state = state;
    for (const listener of this.listeners) listener();
  }
  open(spaceId: string, locale: 'en' | 'pt-BR'): Promise<void> {
    if (this.closed) return Promise.resolve();
    if (this.opening) return this.opening;
    const task = (async () => {
      try {
        const result = await this.port.open(spaceId, locale);
        if (result.available) {
          this.id = result.id;
          if (this.closed) return;
          this.update({
            ...this.state,
            phase: 'ready',
            contextMessages: result.contextMessages,
            limited: result.limited,
          });
        } else if (!this.closed)
          this.update({ ...this.state, phase: 'unavailable', reason: result.reason });
      } catch {
        if (!this.closed) this.update({ ...this.state, phase: 'failed', failed: true });
      }
    })();
    this.opening = task;
    return task;
  }
  ask(question: string): Promise<boolean> {
    const text = question.trim();
    if (this.closed || !this.id || this.state.phase !== 'ready' || this.asking || !text)
      return Promise.resolve(false);
    const id = this.id;
    this.update({ ...this.state, phase: 'asking', pending: text, failed: false });
    const task = (async () => {
      try {
        const result = await this.port.ask(id, text);
        if (this.closed) return false;
        if (!result.text.trim()) throw new Error('empty');
        this.update({
          ...this.state,
          phase: 'ready',
          pending: null,
          turns: [
            ...this.state.turns,
            { role: 'user', text },
            { role: 'assistant', text: result.text },
          ],
        });
        return true;
      } catch {
        if (!this.closed)
          this.update({ ...this.state, phase: 'ready', pending: null, failed: true });
        return false;
      }
    })();
    this.asking = task;
    void task.finally(() => {
      this.asking = null;
    });
    return task;
  }
  close = (): Promise<void> => {
    this.closed = true;
    this.update({ ...this.state, phase: 'closed', turns: [], pending: null, failed: false });
    if (this.closing) return this.closing;
    const task = (async () => {
      // A late open is still ours to close. Native close cancels/drains an active response.
      await this.opening;
      if (this.id) {
        await this.port.close(this.id);
        this.id = null;
      }
      await this.asking;
    })();
    this.closing = task;
    void task
      .finally(() => {
        this.closing = null;
      })
      .catch(() => {});
    return task;
  };
}
