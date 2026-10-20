/** Ephemeral gesture admission only. Persisted order stays in the existing owner-scoped preferences. */
export type CardOrderSnapshot = {
  order: readonly string[];
  editing: boolean;
  pending: boolean;
};
export function sameCardOrder(a: readonly string[], b: readonly string[]) {
  'worklet';
  return a.length === b.length && a.every((id, index) => id === b[index]);
}
export function moveCard(order: readonly string[], id: string, index: number): string[] {
  'worklet';
  const next = [...order];
  const previous = next.indexOf(id);
  if (previous < 0) return next;
  next.splice(previous, 1);
  next.splice(Math.max(0, Math.min(next.length, index)), 0, id);
  return next;
}
export function mergeCardSubset(complete: readonly string[], subset: readonly string[]): string[] {
  const selected = new Set(subset);
  let index = 0;
  return complete.map((id) => (selected.has(id) ? subset[index++]! : id));
}
export function cardDropIndex(
  x: number,
  y: number,
  count: number,
  columns: number,
  strideX: number,
  strideY: number,
) {
  'worklet';
  const column = Math.max(0, Math.min(columns - 1, Math.round(x / strideX)));
  const row = Math.max(0, Math.round(y / strideY));
  return Math.max(0, Math.min(count - 1, row * columns + column));
}
export function cardEdgeVelocity(position: number, start: number, length: number) {
  'worklet';
  const edge = Math.min(56, length / 4);
  const local = position - start;
  if (local < edge) return -260 * Math.min(1, Math.max(0, (edge - local) / edge));
  if (local > length - edge) return 260 * Math.min(1, Math.max(0, (local - length + edge) / edge));
  return 0;
}
export class CardReorderSession {
  private state: CardOrderSnapshot;
  private canonical: readonly string[];
  private generation = 0;
  private persist: (next: readonly string[], expected: readonly string[]) => Promise<unknown>;
  private listeners = new Set<() => void>();
  constructor(
    ids: readonly string[],
    persist: (next: readonly string[], expected: readonly string[]) => Promise<unknown>,
  ) {
    this.persist = persist;
    this.canonical = [...ids];
    this.state = { order: this.canonical, editing: false, pending: false };
  }
  setPersist(persist: (next: readonly string[], expected: readonly string[]) => Promise<unknown>) {
    this.persist = persist;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private publish(state: CardOrderSnapshot) {
    this.state = state;
    for (const listener of this.listeners) listener();
  }
  update(ids: readonly string[]) {
    if (sameCardOrder(ids, this.canonical)) return;
    this.canonical = [...ids];
    // The preferences controller publishes its acknowledged order before the promise resolves.
    if (this.state.pending && sameCardOrder(ids, this.state.order)) return;
    this.generation++;
    this.publish({ order: this.canonical, editing: this.state.editing, pending: false });
  }
  edit() {
    if (!this.state.editing && !this.state.pending) this.publish({ ...this.state, editing: true });
  }
  retire() {
    this.generation++;
    this.publish({ order: this.canonical, editing: false, pending: false });
  }
  async drop(next: readonly string[], expected: readonly string[]): Promise<boolean> {
    if (
      !this.state.editing ||
      this.state.pending ||
      !sameCardOrder(expected, this.canonical) ||
      next.length !== expected.length ||
      new Set(next).size !== next.length ||
      next.some((id) => !expected.includes(id))
    )
      return false;
    if (sameCardOrder(next, expected)) return false;
    const generation = this.generation;
    const proposed = [...next];
    this.publish({ order: proposed, editing: true, pending: true });
    try {
      await this.persist(proposed, [...expected]);
      if (generation !== this.generation) return false;
      this.canonical = proposed;
      this.publish({ order: proposed, editing: true, pending: false });
      return true;
    } catch {
      if (generation === this.generation)
        this.publish({ order: this.canonical, editing: true, pending: false });
      return false;
    }
  }
}
