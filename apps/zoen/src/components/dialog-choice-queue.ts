import type { DialogChoiceOptions } from './dialog-types';

export type PendingDialogChoice = Readonly<{ id: number; options: DialogChoiceOptions }>;
type Request = PendingDialogChoice & { resolve(value: string | null): void; detach(): void };

/** Transient presentations only. Session aborts retire the exact request, including queued ones. */
export class DialogChoiceQueue {
  private sequence = 0;
  private active = true;
  private requests: Request[] = [];
  private listeners = new Set<() => void>();
  getSnapshot = (): PendingDialogChoice | null => this.requests[0] ?? null;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  choose = (options: DialogChoiceOptions): Promise<string | null> => {
    if (!this.active || options.signal?.aborted) return Promise.resolve(null);
    return new Promise((resolve) => {
      const id = ++this.sequence;
      const abort = () => this.finish(id, null);
      const request: Request = {
        id,
        options,
        resolve,
        detach: () => options.signal?.removeEventListener('abort', abort),
      };
      options.signal?.addEventListener('abort', abort, { once: true });
      this.requests.push(request);
      this.publish();
    });
  };
  finish = (id: number, value: string | null) => {
    const index = this.requests.findIndex((request) => request.id === id);
    if (index < 0) return;
    const [request] = this.requests.splice(index, 1);
    request!.detach();
    request!.resolve(
      value !== null && request!.options.choices.some((item) => item.id === value) ? value : null,
    );
    this.publish();
  };
  activate() {
    this.active = true;
  }
  retire() {
    this.active = false;
    const requests = this.requests.splice(0);
    for (const request of requests) {
      request.detach();
      request.resolve(null);
    }
    this.publish();
  }
  private publish() {
    for (const listener of this.listeners) listener();
  }
}
