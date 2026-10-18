import {
  LiveActivity,
  TypingSender,
  activityClock,
  emptyActivity,
  type ActivityClock,
  type LiveActivityState,
} from './live-activity.ts';
import type {
  Command,
  CommandResult,
  CoreDriver,
  CoreEvent,
  DeviceScope,
  Mode,
  Snapshot,
  TimelinePage,
} from './types';

export type ClientState = {
  phase: 'opening' | 'erasing' | 'onboarding' | 'ready' | 'failed';
  erasePending: boolean;
  mode: Mode;
  snapshot: Snapshot | null;
  timelines: Readonly<Record<string, TimelinePage>>;
  notice: string | null;
  activity: LiveActivityState;
};

const failure = (error: unknown) =>
  error instanceof Error ? error.message : 'Não deu para concluir. Tente novamente.';

/** One observable UI projection. Rust remains the durable source of truth. */
export class ClientStore {
  private state: ClientState = {
    phase: 'opening',
    erasePending: false,
    mode: 'account',
    snapshot: null,
    timelines: {},
    notice: null,
    activity: emptyActivity(),
  };
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private erasure: Promise<void> | undefined;
  private eraseScope: DeviceScope | null = null;
  private eraseCleanup: readonly (() => Promise<void>)[] | null = null;
  private readonly deviceCleanup = new Set<() => Promise<void>>();
  private activityGeneration = 0;
  private stopEvents: (() => void) | undefined;
  private refreshTask: Promise<void> | undefined;
  private refreshAgain = false;
  private activeSpace: string | null = null;
  private readonly timelineVersions = new Map<string, number>();
  private readonly olderTasks = new Map<string, Promise<void>>();

  private readonly live: LiveActivity;
  private readonly typing: TypingSender;
  readonly driver: CoreDriver;
  constructor(driver: CoreDriver, clock: ActivityClock = activityClock) {
    this.driver = driver;
    this.live = new LiveActivity(
      () => this.state.snapshot,
      (activity) => this.publish({ activity }),
      clock,
    );
    this.typing = new TypingSender(
      (space) => {
        const snapshot = this.state.snapshot;
        const me = snapshot?.me?.id ?? snapshot?.account?.identity_id;
        return (
          this.live.isConnected() &&
          this.state.phase === 'ready' &&
          snapshot?.connection.state === 'online' &&
          snapshot.connection.synced &&
          snapshot.conversations.some(
            (chat) => chat.id === space && chat.members.some((person) => person.id === me),
          )
        );
      },
      (space_id, typing) => this.driver.request({ op: 'set_typing', space_id, typing }),
      clock,
    );
  }
  setTyping(space: string, typing: boolean) {
    this.typing.update(space, typing);
  }
  private resetActivity(waitForSnapshot = false) {
    ++this.activityGeneration;
    if (waitForSnapshot) this.live.beginConnection();
    else this.live.reset();
    this.typing.reset();
    this.publish({ activity: emptyActivity() });
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

  registerDeviceCleanup(cleanup: () => Promise<void>): () => void {
    this.deviceCleanup.add(cleanup);
    return () => {
      this.deviceCleanup.delete(cleanup);
    };
  }

  async open(mode: Mode = 'account'): Promise<void> {
    if (this.erasure) return this.erasure;
    if (this.state.erasePending && this.eraseScope) return this.eraseDevice(this.eraseScope);
    return this.openSession(mode);
  }

  private async openSession(mode: Mode): Promise<void> {
    const generation = ++this.generation;
    this.stopEvents?.();
    this.resetActivity(true);
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
      this.live.synchronize(snapshot);
      this.publish({
        snapshot,
        activity: this.live.value(),
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
      this.resetActivity();
      this.publish({ notice: event.message ?? 'Não foi possível sincronizar.' });
      return;
    }
    if (event.kind === 'ephemeral' || event.kind === 'presence') {
      this.live.receive(event);
      return;
    }
    if (event.kind === 'connection') this.resetActivity(true);
    if (this.state.phase === 'opening') {
      this.refreshAgain = true;
      return;
    }
    void this.refresh();
  }

  refresh(): Promise<void> {
    if (this.state.erasePending) return Promise.resolve();
    if (this.refreshTask) {
      this.refreshAgain = true;
      return this.refreshTask;
    }
    const generation = this.generation;
    const task = (async () => {
      do {
        this.refreshAgain = false;
        const activityGeneration = this.activityGeneration;
        const snapshot = await this.driver.request({ op: 'snapshot' });
        if (generation !== this.generation) return;
        if (activityGeneration === this.activityGeneration) this.live.synchronize(snapshot);

        if (snapshot.connection.state !== 'online' || !snapshot.connection.synced)
          this.typing.reset();
        this.publish({
          snapshot,
          activity: this.live.value(),
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
    this.requireAvailableSession();
    const generation = this.generation;
    try {
      const snapshot = await this.driver.createAccount(name, handle, relay);
      if (generation !== this.generation) throw new Error('A sessão mudou.');
      this.live.synchronize(snapshot);
      this.publish({ snapshot, phase: 'ready', notice: null, activity: this.live.value() });
    } catch (error) {
      if (generation === this.generation) this.publish({ notice: failure(error) });
      throw error;
    }
  }

  loadTimeline(space: string, older = false): Promise<void> {
    if (this.state.erasePending) return Promise.reject(new Error('A sessão está sendo apagada.'));
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
    this.requireAvailableSession();
    const generation = this.generation;
    try {
      const result = await this.driver.request(command);
      if (generation !== this.generation) throw new Error('A sessão mudou.');
      await this.refresh();
      if (generation !== this.generation) throw new Error('A sessão mudou.');
      return result;
    } catch (error) {
      // Provisioning retains keys before contacting the relay. Even when its
      // response is lost, the next provider proof must see that pending identity.
      if (command.op === 'provider_sign_in' && generation === this.generation) await this.refresh();
      if (generation === this.generation) this.publish({ notice: failure(error) });
      throw error;
    }
  }

  dismissNotice() {
    this.publish({ notice: null });
  }
  private requireAvailableSession() {
    if (this.state.erasePending)
      throw new Error('Conclua a saída deste aparelho antes de continuar.');
  }

  eraseDevice(expected: DeviceScope): Promise<void> {
    const current = this.eraseScope ?? this.state.snapshot?.account;
    if (
      this.state.mode !== 'account' ||
      !current ||
      current.identity_id !== expected.identity_id ||
      current.device_id !== expected.device_id
    )
      return Promise.reject(new Error('A conta mudou. Abra a confirmação novamente.'));
    if (this.erasure) return this.erasure;
    const scope = Object.freeze({ identity_id: current.identity_id, device_id: current.device_id });
    this.eraseScope = scope;
    // Clearing the snapshot can unmount account-bound providers. Preserve the
    // original cleanup owners through every retry, including that publication.
    const cleanups = (this.eraseCleanup ??= [...this.deviceCleanup]);
    ++this.generation;
    this.stopEvents?.();
    this.stopEvents = undefined;
    this.refreshTask = undefined;
    this.refreshAgain = false;
    this.activeSpace = null;
    this.olderTasks.clear();
    this.timelineVersions.clear();
    this.publish({
      phase: 'erasing',
      erasePending: true,
      snapshot: null,
      timelines: {},
      notice: null,
      activity: emptyActivity(),
    });
    this.resetActivity();
    const task = (async () => {
      const cleanup = await Promise.allSettled(
        cleanups.map((work) => Promise.resolve().then(work)),
      );
      try {
        // Preflight removes OS projections before committing canonical erasure.
        // If interrupted here, the unchanged account may reopen on restart.
        if (cleanup.some((result) => result.status === 'rejected'))
          throw new Error(
            'Não foi possível apagar todas as prévias deste aparelho. Tente sair novamente.',
          );
        await this.driver.eraseDevice(scope);
      } catch (error) {
        const message = failure(error);
        this.publish({ phase: 'failed', erasePending: true, notice: message });
        throw new Error(message);
      }
      this.eraseScope = null;
      this.eraseCleanup = null;
      this.publish({ erasePending: false });
      await this.openSession('account');
    })();
    this.erasure = task;
    void task
      .finally(() => {
        if (this.erasure === task) this.erasure = undefined;
      })
      .catch(() => {});
    return task;
  }

  async close() {
    if (this.erasure) await this.erasure.catch(() => {});
    ++this.generation;
    this.stopEvents?.();
    this.stopEvents = undefined;
    this.resetActivity();
    await this.driver.close();
  }
}
