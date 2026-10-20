/** Raw input stays native. This port carries ownership and retention metadata only. */
export type PageInputMode = 'rich' | 'markdown' | 'lossless-text';
export type PageInputOperation = 'apply' | 'discard';
export type PageInputPending = {
  revision: string;
  hasRetainedCapture: boolean;
  blockId: string | null;
  mode: PageInputMode | null;
};
export type PageInputStatus = {
  session: string;
  id: string;
  token: string | null;
  itemId: string;
  blockId: string | null;
  mode: PageInputMode;
  resuming: boolean;
  revision: string;
  statusVersion: string;
  sequence: string | null;
  hasRetainedCapture: boolean;
  retentionPending: boolean;
  failed: boolean;
  sealed: boolean;
  completed: boolean;
  operation: PageInputOperation | null;
  /** Canonical seed only. Native recovery never sends a capture to JavaScript. */
  initialMarkdown?: string;
};
export type PageInputOpen = {
  session: string;
  id: string;
  itemId: string;
  blockId: string | null;
  resume: boolean;
  revision: string;
  editContext: string;
  mode: PageInputMode;
};
export interface PageInputPort {
  session(): Promise<string>;
  load(session: string, itemId: string): Promise<PageInputPending>;
  reserve(session: string, itemId: string): Promise<string>;
  open(request: PageInputOpen): Promise<PageInputStatus>;
  status(session: string, id: string): Promise<PageInputStatus>;
  seal(session: string, id: string): Promise<PageInputStatus>;
  apply(session: string, id: string): Promise<PageInputStatus>;
  discard(session: string, id: string): Promise<PageInputStatus>;
  close(session: string, id: string): Promise<void>;
  subscribe(listener: (status: PageInputStatus) => void): () => void;
}
export type PageInputOptions = {
  itemId: string;
  blockId: string | null;
  editContext: string;
  mode: PageInputMode;
  resume?: boolean;
};

export class PageInputConflict extends Error {}

/** One mounted editor owns one native reservation; no text or mutation receipt lives here. */
export class PageInputEditor {
  private allocation: Promise<PageInputOpen> | undefined;
  private opening: Promise<PageInputStatus> | undefined;
  private retiring = false;
  private closed = false;
  private closing: Promise<void> | undefined;
  private operation: { kind: PageInputOperation; task: Promise<PageInputStatus> } | undefined;
  private state: PageInputStatus | null = null;
  private readonly listeners = new Set<() => void>();
  private detach: (() => void) | undefined;

  private readonly port: PageInputPort;
  private readonly options: PageInputOptions;
  constructor(port: PageInputPort, options: PageInputOptions) {
    this.port = port;
    this.options = options;
  }

  getSnapshot = (): PageInputStatus | null => this.state;
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private publish(status: PageInputStatus) {
    if (this.closed) return;
    if (this.state && BigInt(status.statusVersion) < BigInt(this.state.statusVersion)) return;
    this.state = { ...this.state, ...status };
    for (const listener of this.listeners) listener();
  }

  open(): Promise<PageInputStatus> {
    if (this.retiring) return Promise.reject(new Error('The editor is closing.'));
    if (this.state?.token) return Promise.resolve(this.state);
    if (this.opening) return this.opening;
    this.detach ??= this.port.subscribe((status) => {
      if (this.state?.session === status.session && this.state.id === status.id)
        this.publish(status);
    });
    this.allocation ??= (async () => {
      const session = await this.port.session();
      const pending = await this.port.load(session, this.options.itemId);
      if (
        pending.hasRetainedCapture &&
        pending.blockId !== this.options.blockId &&
        !this.options.resume
      )
        throw new PageInputConflict('Recover the existing input before editing another block.');
      const resume = pending.hasRetainedCapture;
      if (resume && !pending.mode)
        throw new PageInputConflict('The retained input has no editor mode.');
      const id = await this.port.reserve(session, this.options.itemId);
      return {
        session,
        id,
        itemId: this.options.itemId,
        blockId: resume ? pending.blockId : this.options.blockId,
        resume,
        revision: pending.revision,
        editContext: this.options.editContext,
        mode: resume ? pending.mode! : this.options.mode,
      };
    })();
    const task = (async () => {
      let request: PageInputOpen;
      try {
        request = await this.allocation!;
      } catch (error) {
        this.allocation = undefined;
        throw error;
      }
      if (this.retiring) throw new Error('The editor is closing.');
      const status = await this.port.open(request);
      if (this.retiring) throw new Error('The editor is closing.');
      this.publish(status);
      // Covers a native checkpoint/status transition delivered before open resolved.
      this.publish(await this.port.status(request.session, request.id));
      return this.state!;
    })();
    this.opening = task;
    void task
      .finally(() => {
        if (this.opening === task) this.opening = undefined;
      })
      .catch(() => {});
    return task;
  }

  async retryRetention(): Promise<void> {
    if (this.retiring || this.operation)
      throw new Error('Finish the pending editor operation first.');
    const request = await this.allocation;
    if (!request) throw new Error('The editor has not opened.');
    this.publish(await this.port.seal(request.session, request.id));
  }

  apply(): Promise<PageInputStatus> {
    return this.perform('apply');
  }
  discard(): Promise<PageInputStatus> {
    return this.perform('discard');
  }

  private perform(kind: PageInputOperation): Promise<PageInputStatus> {
    if (this.retiring) return Promise.reject(new Error('The editor is closing.'));
    if (this.operation) {
      return this.operation.kind === kind
        ? this.operation.task
        : Promise.reject(new Error('Retry the pending editor operation first.'));
    }
    if (this.state?.operation && this.state.operation !== kind)
      return Promise.reject(new Error('Retry the pending editor operation first.'));
    const task = (async () => {
      const request = await this.allocation;
      if (!request) throw new Error('The editor has not opened.');
      try {
        const status = await this.port[kind](request.session, request.id);
        this.publish(status);
        return status;
      } catch (error) {
        // Native owns the exact pending mutation even if its response was lost.
        try {
          this.publish(await this.port.status(request.session, request.id));
        } catch {
          /* Owner may have retired. */
        }
        throw error;
      }
    })();
    this.operation = { kind, task };
    void task
      .finally(() => {
        if (this.operation?.task === task) this.operation = undefined;
      })
      .catch(() => {});
    return task;
  }

  /** Confirmed device/session retirement is fenced and drained by the native owner. */
  retire(): void {
    this.retiring = true;
    this.closed = true;
    this.detach?.();
    this.detach = undefined;
  }

  /** Called before route removal. Failure keeps this owner retryable and visible. */
  close(): Promise<void> {
    if (this.closed) return Promise.resolve();
    if (this.closing) return this.closing;
    this.retiring = true;
    const task = (async () => {
      let request: PageInputOpen | undefined;
      try {
        request = await this.allocation;
      } catch {
        /* No reservation was allocated. */
      }
      if (request) await this.port.close(request.session, request.id);
      this.closed = true;
      this.detach?.();
      this.detach = undefined;
    })();
    this.closing = task;
    void task
      .finally(() => {
        this.closing = undefined;
        if (!this.closed) this.retiring = false;
      })
      .catch(() => {});
    return task;
  }
}
