/** Drain already-started OS writes before removing the device's notifications. */
export class NotificationJobs {
  private pending = new Set<Promise<unknown>>();

  track(job: Promise<unknown>): void {
    this.pending.add(job);
    void job.finally(() => this.pending.delete(job)).catch(() => {});
  }

  async drain(): Promise<void> {
    while (this.pending.size) await Promise.allSettled(this.pending);
  }
}
