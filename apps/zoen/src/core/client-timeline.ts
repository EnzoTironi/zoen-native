import type { ClientState } from './client-store-state.ts';
import type { Command, CommandResult } from './types';
type TimelineOwner = {
  state(): Pick<ClientState, 'erasePending' | 'timelines'>;
  retiring(): boolean;
  generation(): number;
  request<C extends Command>(command: C): Promise<CommandResult<C>>;
  changing(spaces: readonly string[]): void;
  publish(update: Pick<ClientState, 'timelines'>): void;
};

/** Session-bound read scheduling; ClientStore remains the sole visible state owner. */
export class ClientTimeline {
  private readonly owner: TimelineOwner;
  private readonly timelineVersions = new Map<string, number>();
  private readonly olderTasks = new Map<string, Promise<void>>();
  constructor(owner: TimelineOwner) {
    this.owner = owner;
  }
  clear() {
    this.olderTasks.clear();
    this.timelineVersions.clear();
  }

  load(space: string, older = false): Promise<void> {
    if (this.owner.state().erasePending)
      return Promise.reject(new Error('A sessão está sendo apagada.'));
    if (this.owner.retiring()) return Promise.reject(new Error('A sessão está sendo encerrada.'));
    if (older) {
      const pending = this.olderTasks.get(space);
      if (pending) return pending;
    }
    const task = this.read(space, older);
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

  async read(space: string, older: boolean, announce = true): Promise<void> {
    const current = this.owner.state().timelines[space];
    if (older && !current?.has_more) return;
    const generation = this.owner.generation();
    if (announce) this.owner.changing([space]);
    if (
      generation !== this.owner.generation() ||
      this.owner.state().erasePending ||
      this.owner.retiring()
    )
      return;
    const version = (this.timelineVersions.get(space) ?? 0) + 1;
    this.timelineVersions.set(space, version);
    let next = await this.owner.request({
      op: 'timeline',
      space_id: space,
      before: older ? (current?.before ?? null) : null,
      limit: 80,
    });
    // Re-read the whole visible window so sync preserves history and updates older edits.
    const anchor = !older ? current?.entries[0]?.id : undefined;
    const cursors = new Set<string>();
    while (anchor && next.has_more && !next.entries.some((entry) => entry.id === anchor)) {
      if (generation !== this.owner.generation() || this.timelineVersions.get(space) !== version)
        return;
      if (!next.before || cursors.has(next.before))
        throw new Error('Não foi possível carregar o histórico da conversa.');
      cursors.add(next.before);
      const earlier = await this.owner.request({
        op: 'timeline',
        space_id: space,
        before: next.before,
        limit: 200,
      });
      next = { ...earlier, entries: [...earlier.entries, ...next.entries] };
    }
    if (generation !== this.owner.generation() || this.timelineVersions.get(space) !== version)
      return;
    const page =
      older && current ? { ...next, entries: [...next.entries, ...current.entries] } : next;
    this.owner.publish({ timelines: { ...this.owner.state().timelines, [space]: page } });
  }

  async reveal(space: string, message: string): Promise<number> {
    const generation = this.owner.generation();
    if (!this.owner.state().timelines[space]) await this.load(space);
    for (;;) {
      if (generation !== this.owner.generation()) return -1;
      const page = this.owner.state().timelines[space];
      const index = page?.entries.findIndex((entry) => entry.id === message) ?? -1;
      if (index >= 0 || !page?.has_more) return index;
      const cursor = page.before;
      await this.load(space, true);
      if (this.owner.state().timelines[space]?.before === cursor) return -1;
    }
  }
}
