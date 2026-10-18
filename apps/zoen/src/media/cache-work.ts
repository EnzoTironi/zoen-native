/** Temporary SDK file work only. Canonical account state remains in ClientStore/Rust. */
export class SensitiveCacheWork {
  private paused = false;
  private pending = new Set<Promise<unknown>>();

  run<T>(work: () => Promise<T>): Promise<T> {
    if (this.paused) return Promise.reject(new Error('Este aparelho está saindo da conta.'));
    try {
      return this.track(work());
    } catch (error) {
      return Promise.reject(error);
    }
  }

  /** Shutdown work may join the drain after new user operations have been fenced. */
  track<T>(task: Promise<T>): Promise<T> {
    this.pending.add(task);
    void task.finally(() => this.pending.delete(task)).catch(() => {});
    return task;
  }

  async erase(clear: () => Promise<void>): Promise<void> {
    this.paused = true;
    while (this.pending.size) await Promise.allSettled(this.pending);
    await clear();
  }

  resume(): void {
    this.paused = false;
  }
}

export const sensitiveCacheWork = new SensitiveCacheWork();
