import {
  LiveActivity,
  TypingSender,
  activityClock,
  emptyActivity,
  type ActivityClock,
  type LiveActivityState,
} from './live-activity.ts';
import { NormalMessageDraftService } from './normal-message-draft-service.ts';
import { MediaDraftService } from './media-draft-service.ts';
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

type SessionRetirement = {
  task: Promise<void>;
  cancelled: boolean;
  force: boolean;
  nativeCloseStarted: boolean;
  forceTask?: Promise<void>;
};

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
  private readonly canonicalListeners = new Set<(spaceIds?: readonly string[]) => void>();
  private generation = 0;
  private ownerEpoch = 0;
  private retiring = false;
  private retirement: SessionRetirement | undefined;
  private erasure: Promise<void> | undefined;
  private eraseRetirement: Promise<void> | undefined;
  private eraseScope: DeviceScope | null = null;
  private eraseCleanup: readonly (() => Promise<void>)[] | null = null;
  private readonly deviceCleanup = new Set<() => Promise<void>>();
  private readonly sessionCleanup = new Set<(discard: boolean) => Promise<void>>();
  private readonly sessionPreflight = new Set<() => void>();
  private activityGeneration = 0;
  private stopEvents: (() => void) | undefined;
  private refreshTask: Promise<void> | undefined;
  private refreshAgain = false;
  // undefined = no queued read; null = an unknown/global change, which dominates scoped reads.
  private refreshSpaces: Set<string> | null | undefined;
  private activeSpace: string | null = null;
  private readonly timelineVersions = new Map<string, number>();
  private readonly olderTasks = new Map<string, Promise<void>>();

  private readonly live: LiveActivity;
  private readonly typing: TypingSender;
  readonly driver: CoreDriver;
  readonly messageDrafts: NormalMessageDraftService;
  readonly mediaDrafts: MediaDraftService;
  constructor(driver: CoreDriver, clock: ActivityClock = activityClock) {
    this.driver = driver;
    this.messageDrafts = new NormalMessageDraftService(this);
    this.mediaDrafts = new MediaDraftService(this);
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
          !this.retiring &&
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
  get sessionEpoch(): number {
    return this.ownerEpoch;
  }
  get isSessionRetiring(): boolean {
    return this.retiring;
  }
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  /** Invalidate before a canonical read; scope must not be inferred from new object identities. */
  subscribeCanonicalChanges = (listener: (spaceIds?: readonly string[]) => void) => {
    this.canonicalListeners.add(listener);
    return () => {
      this.canonicalListeners.delete(listener);
    };
  };
  private canonicalChanging(spaceIds?: readonly string[]) {
    const spaces = spaceIds?.length ? Object.freeze([...new Set(spaceIds)]) : undefined;
    for (const listener of this.canonicalListeners) listener(spaces);
    return spaces;
  }
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

  registerSessionCleanup(cleanup: (discard: boolean) => Promise<void>): () => void {
    this.sessionCleanup.add(cleanup);
    return () => {
      this.sessionCleanup.delete(cleanup);
    };
  }

  /** Reject graceful replacement or close before retiring any current owner. */
  registerSessionPreflight(check: () => void): () => void {
    this.sessionPreflight.add(check);
    return () => {
      this.sessionPreflight.delete(check);
    };
  }

  private checkSessionPreflight() {
    for (const check of this.sessionPreflight) check();
  }

  /** External pushes must not blur an editor that still owns unsaved content. */
  assertExternalNavigationAllowed() {
    this.requireAvailableSession();
    this.checkSessionPreflight();
  }

  private retireSession(discard: boolean): Promise<void> | undefined {
    if (!this.sessionCleanup.size) return;
    // Stop admission synchronously, before publishing a different account.
    const work = [...this.sessionCleanup].map((cleanup) => {
      try {
        return cleanup(discard);
      } catch (error) {
        return Promise.reject(error);
      }
    });
    return Promise.allSettled(work).then((results) => {
      const rejected = results.find((result) => result.status === 'rejected');
      if (rejected?.status === 'rejected') throw rejected.reason;
    });
  }

  private fenceSessionWork(): number {
    this.retiring = true;
    const generation = ++this.generation;
    this.stopEvents?.();
    this.stopEvents = undefined;
    ++this.activityGeneration;
    this.live.reset();
    this.typing.reset();
    this.refreshTask = undefined;
    this.refreshAgain = false;
    this.refreshSpaces = undefined;
    this.olderTasks.clear();
    this.timelineVersions.clear();
    return generation;
  }

  private beginSessionRetirement(force = false): Promise<void> {
    if (this.retirement) {
      const retirement = this.retirement;
      if (force && !retirement.force) {
        retirement.force = true;
        if (retirement.nativeCloseStarted) {
          // A native seal can be waiting on UI that has already unmounted.
          // Escalate it now instead of waiting for the graceful close to return.
          retirement.forceTask = (async () => this.driver.close({ force: true }))();
          void retirement.forceTask.catch(() => {});
        }
      }
      return retirement.task;
    }
    let resolve!: () => void;
    let reject!: (error: unknown) => void;
    const task = new Promise<void>((done, failed) => {
      resolve = done;
      reject = failed;
    });
    const retirement: SessionRetirement = {
      task,
      cancelled: false,
      force,
      nativeCloseStarted: false,
    };
    this.retirement = retirement;
    // Cleanup freezes its own controllers synchronously. Keep the visible owner
    // and its epoch alive until admitted native saves and the final seal finish.
    const cleanup = this.retireSession(false);
    void (async () => {
      if (cleanup) await cleanup;
      if (retirement.cancelled) return;
      retirement.nativeCloseStarted = true;
      try {
        await this.driver.close(retirement.force ? { force: true } : undefined);
      } catch (error) {
        if (!retirement.forceTask) throw error;
      }
      if (retirement.forceTask) await retirement.forceTask;
      if (!retirement.cancelled) ++this.ownerEpoch;
    })().then(resolve, reject);
    const settled = () => {
      if (this.retirement === retirement) this.retirement = undefined;
    };
    void task.then(settled, settled);
    return task;
  }

  async open(mode: Mode = 'account'): Promise<void> {
    if (this.erasure) return this.erasure;
    if (this.state.erasePending && this.eraseScope) return this.eraseDevice(this.eraseScope);
    return this.openSession(mode);
  }

  private async openSession(mode: Mode, preflight = true): Promise<void> {
    if (preflight) this.checkSessionPreflight();
    const generation = this.fenceSessionWork();
    try {
      await this.beginSessionRetirement();
      if (generation !== this.generation) return;
      this.activeSpace = null;
      this.resetActivity(true);
      if (generation !== this.generation) return;
      this.publish({ phase: 'opening', mode, snapshot: null, timelines: {}, notice: null });
      if (generation !== this.generation) return;
      const stopEvents = this.driver.subscribe((event) => {
        if (generation === this.generation) this.event(event);
      });
      if (generation !== this.generation) {
        stopEvents();
        return;
      }
      this.stopEvents = stopEvents;
      const snapshot = await this.driver.open(mode);
      if (generation !== this.generation) return;
      this.retiring = false;
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
      if (generation === this.generation)
        this.publish({
          ...(this.state.snapshot ? {} : { phase: 'failed' as const }),
          notice: failure(error),
        });
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
    void this.refresh(event.kind === 'change' ? event.spaceIds : undefined);
  }

  refresh(spaceIds?: readonly string[]): Promise<void> {
    if (this.state.erasePending || this.retiring) return Promise.resolve();
    const generation = this.generation;
    // Notify even while a read is in flight: a matching deletion must fence media immediately.
    const spaces = this.canonicalChanging(spaceIds);
    if (generation !== this.generation || this.state.erasePending || this.retiring)
      return Promise.resolve();
    if (!spaces || this.refreshSpaces === null) this.refreshSpaces = null;
    else {
      this.refreshSpaces ??= new Set();
      for (const space of spaces) this.refreshSpaces.add(space);
    }
    if (this.refreshTask) {
      this.refreshAgain = true;
      return this.refreshTask;
    }
    const task = (async () => {
      do {
        const affected = this.refreshSpaces;
        this.refreshSpaces = undefined;
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
        if (this.activeSpace && (!affected || affected.has(this.activeSpace)))
          await this.readTimeline(this.activeSpace, false, false);
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
    this.checkSessionPreflight();
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
    if (this.retiring) return Promise.reject(new Error('A sessão está sendo encerrada.'));
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

  private async readTimeline(space: string, older: boolean, announce = true): Promise<void> {
    const current = this.state.timelines[space];
    if (older && !current?.has_more) return;
    const generation = this.generation;
    if (announce) this.canonicalChanging([space]);
    if (generation !== this.generation || this.state.erasePending || this.retiring) return;
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
    if (this.retiring) return;
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
      await this.refresh('space_id' in command ? [command.space_id] : undefined);
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
    if (this.retiring) throw new Error('A sessão está sendo encerrada. Tente novamente.');
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
    let resolve!: () => void;
    let reject!: (error: unknown) => void;
    const task = new Promise<void>((done, failed) => {
      resolve = done;
      reject = failed;
    });
    // Subscribers and cleanup owners may synchronously request another open,
    // close or erase. They must all observe this same confirmed erasure.
    this.erasure = task;
    const superseded = this.retirement;
    if (superseded) {
      superseded.cancelled = true;
      // Keep this drain through failed erase attempts as well: an old native
      // close must settle before any successful retry opens a replacement core.
      this.eraseRetirement = Promise.all([
        this.eraseRetirement,
        superseded.task.catch(() => {}),
      ]).then(() => {});
    }
    this.retirement = undefined;
    this.fenceSessionWork();
    const retirement = this.retireSession(true);
    ++this.ownerEpoch;
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
    void (async () => {
      const cleanup = await Promise.allSettled([
        ...cleanups.map((work) => Promise.resolve().then(work)),
        ...(retirement ? [retirement] : []),
      ]);
      try {
        // Preflight removes OS projections before committing canonical erasure.
        // If interrupted here, the unchanged account may reopen on restart.
        if (cleanup.some((result) => result.status === 'rejected'))
          throw new Error(
            'Não foi possível apagar todas as prévias deste aparelho. Tente sair novamente.',
          );
        await this.driver.eraseDevice(scope);
        // Native erasure may be what releases a suspended graceful seal. Start
        // it first, then drain the obsolete close before opening the empty core.
        await this.eraseRetirement;
      } catch (error) {
        const message = failure(error);
        this.publish({ phase: 'failed', erasePending: true, notice: message });
        throw new Error(message);
      }
      this.eraseScope = null;
      this.eraseCleanup = null;
      this.eraseRetirement = undefined;
      this.publish({ erasePending: false });
      // The confirmed erasure already discarded the old owners and must reopen empty.
      await this.openSession('account', false);
    })().then(resolve, reject);
    void task
      .finally(() => {
        if (this.erasure === task) this.erasure = undefined;
      })
      .catch(() => {});
    return task;
  }

  async close({ force = false }: { force?: boolean } = {}) {
    const erasure = this.erasure;
    if (erasure) await erasure.catch(() => {});
    if (!force && !erasure && !this.state.erasePending) this.checkSessionPreflight();
    const generation = this.fenceSessionWork();
    try {
      await this.beginSessionRetirement(force);
      if (generation !== this.generation) return;
      this.activeSpace = null;
      this.resetActivity();
      if (generation !== this.generation) return;
      this.publish({ phase: 'opening', snapshot: null, timelines: {}, notice: null });
    } catch (error) {
      if (generation !== this.generation) return;
      this.publish({
        ...(this.state.snapshot ? {} : { phase: 'failed' as const }),
        notice: failure(error),
      });
      throw error;
    }
  }
}
