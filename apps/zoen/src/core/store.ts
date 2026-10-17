import type {
  Command,
  CommandResult,
  CoreDriver,
  CoreEvent,
  Mode,
  Snapshot,
  TimelinePage,
} from './types';

export type ClientState = {
  phase: 'opening' | 'onboarding' | 'ready' | 'failed';
  mode: Mode;
  snapshot: Snapshot | null;
  timelines: Readonly<Record<string, TimelinePage>>;
  notice: string | null;
};

const failure = (error: unknown) =>
  error instanceof Error ? error.message : 'Não deu para concluir. Tente novamente.';

/** One observable UI projection. Rust remains the durable source of truth. */
export class ClientStore {
  private state: ClientState = {
    phase: 'opening',
    mode: 'account',
    snapshot: null,
    timelines: {},
    notice: null,
  };
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private stopEvents: (() => void) | undefined;
  private refreshTask: Promise<void> | undefined;
  private refreshAgain = false;
  private activeSpace: string | null = null;
  private readonly timelineVersions = new Map<string, number>();
  private readonly olderTasks = new Map<string, Promise<void>>();

  readonly driver: CoreDriver;
  constructor(driver: CoreDriver) {
    this.driver = driver;
  }
  getSnapshot = (): ClientState => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<ClientState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  async open(mode: Mode = 'account'): Promise<void> {
    const generation = ++this.generation;
    this.stopEvents?.();
    this.refreshTask = undefined;
    this.refreshAgain = false;
    this.activeSpace = null;
    this.olderTasks.clear();
    this.timelineVersions.clear();
    this.publish({ phase: 'opening', mode, snapshot: null, timelines: {}, notice: null });
    try {
      await this.driver.close();
      if (generation !== this.generation) return;
      this.stopEvents = this.driver.subscribe((event) => {
        if (generation === this.generation) this.event(event);
      });
      const snapshot = await this.driver.open(mode);
      if (generation !== this.generation) return;
      this.publish({
        snapshot,
        phase:
          (snapshot.account && !snapshot.account.setup_pending) || mode === 'demo'
            ? 'ready'
            : 'onboarding',
      });
      if (this.refreshAgain) void this.refresh();
    } catch (error) {
      if (generation === this.generation) this.publish({ phase: 'failed', notice: failure(error) });
    }
  }

  private event(event: CoreEvent) {
    if (event.kind === 'error') {
      this.publish({ notice: event.message ?? 'Não foi possível sincronizar.' });
      return;
    }
    if (event.kind === 'ephemeral' || event.kind === 'presence') return;
    if (this.state.phase === 'opening') {
      this.refreshAgain = true;
      return;
    }
    void this.refresh();
  }

  refresh(): Promise<void> {
    if (this.refreshTask) {
      this.refreshAgain = true;
      return this.refreshTask;
    }
    const generation = this.generation;
    const task = (async () => {
      do {
        this.refreshAgain = false;
        const snapshot = await this.driver.request({ op: 'snapshot' });
        if (generation !== this.generation) return;
        this.publish({
          snapshot,
          phase:
            (snapshot.account && !snapshot.account.setup_pending) || this.state.mode === 'demo'
              ? 'ready'
              : 'onboarding',
        });
        if (this.activeSpace) await this.loadTimeline(this.activeSpace);
      } while (this.refreshAgain && generation === this.generation);
    })().catch((error: unknown) => {
      if (generation === this.generation) this.publish({ notice: failure(error) });
    });
    this.refreshTask = task;
    void task.finally(() => {
      if (this.refreshTask === task) this.refreshTask = undefined;
    });
    return task;
  }

  async createAccount(name: string, handle: string, relay: string) {
    const generation = this.generation;
    try {
      const snapshot = await this.driver.createAccount(name, handle, relay);
      if (generation === this.generation) this.publish({ snapshot, phase: 'ready', notice: null });
    } catch (error) {
      if (generation === this.generation) this.publish({ notice: failure(error) });
      throw error;
    }
  }

  loadTimeline(space: string, older = false): Promise<void> {
    if (older) {
      const pending = this.olderTasks.get(space);
      if (pending) return pending;
    }
    const task = this.readTimeline(space, older);
    if (older) {
      this.olderTasks.set(space, task);
      void task
        .finally(() => {
          if (this.olderTasks.get(space) === task) this.olderTasks.delete(space);
        })
        .catch(() => {});
    }
    return task;
  }

  private async readTimeline(space: string, older: boolean): Promise<void> {
    const current = this.state.timelines[space];
    if (older && !current?.has_more) return;
    const generation = this.generation;
    const version = (this.timelineVersions.get(space) ?? 0) + 1;
    this.timelineVersions.set(space, version);
    let next = await this.driver.request({
      op: 'timeline',
      space_id: space,
      before: older ? (current?.before ?? null) : null,
      limit: 80,
    });
    // Re-read the whole visible window so sync preserves history and updates older edits.
    const anchor = !older ? current?.entries[0]?.id : undefined;
    const cursors = new Set<string>();
    while (anchor && next.has_more && !next.entries.some((entry) => entry.id === anchor)) {
      if (generation !== this.generation || this.timelineVersions.get(space) !== version) return;
      if (!next.before || cursors.has(next.before))
        throw new Error('Não foi possível carregar o histórico da conversa.');
      cursors.add(next.before);
      const earlier = await this.driver.request({
        op: 'timeline',
        space_id: space,
        before: next.before,
        limit: 200,
      });
      next = { ...earlier, entries: [...earlier.entries, ...next.entries] };
    }
    if (generation !== this.generation || this.timelineVersions.get(space) !== version) return;
    const page =
      older && current ? { ...next, entries: [...next.entries, ...current.entries] } : next;
    this.publish({ timelines: { ...this.state.timelines, [space]: page } });
  }

  async revealMessage(space: string, message: string): Promise<number> {
    const generation = this.generation;
    if (!this.state.timelines[space]) await this.loadTimeline(space);
    for (;;) {
      if (generation !== this.generation) return -1;
      const page = this.state.timelines[space];
      const index = page?.entries.findIndex((entry) => entry.id === message) ?? -1;
      if (index >= 0 || !page?.has_more) return index;
      const cursor = page.before;
      await this.loadTimeline(space, true);
      if (this.state.timelines[space]?.before === cursor) return -1;
    }
  }

  selectSpace(space: string | null) {
    this.activeSpace = space;
    const generation = this.generation;
    if (space)
      void this.loadTimeline(space).catch((error: unknown) => {
        if (generation === this.generation) this.publish({ notice: failure(error) });
      });
  }

  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    const generation = this.generation;
    try {
      const result = await this.driver.request(command);
      if (generation === this.generation) await this.refresh();
      return result;
    } catch (error) {
      if (generation === this.generation) this.publish({ notice: failure(error) });
      throw error;
    }
  }

  dismissNotice() {
    this.publish({ notice: null });
  }
  async close() {
    ++this.generation;
    this.stopEvents?.();
    this.stopEvents = undefined;
    await this.driver.close();
  }
}
