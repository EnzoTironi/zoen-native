/** Resource lifetime only: the signed Core log remains the authority for a call. */
export class CallAudioFocus {
  private owner: string | null = null;
  private pending = new Set<Promise<unknown>>();
  get available(): boolean {
    return this.owner === null;
  }
  begin(ticket: string): void {
    if (this.owner && this.owner !== ticket) throw new Error('Audio is owned by another call.');
    this.owner = ticket;
  }
  owns(ticket: string): boolean {
    return this.owner === ticket;
  }
  end(ticket: string): boolean {
    if (!this.owns(ticket)) return false;
    this.owner = null;
    return true;
  }
  async drain(): Promise<void> {
    while (this.pending.size) await Promise.allSettled(this.pending);
  }
  run<T>(operation: () => Promise<T>): Promise<T> {
    if (!this.available) return Promise.reject(new Error('Audio is in use by a call.'));
    const task = Promise.resolve().then(() => {
      if (!this.available) throw new Error('Audio is in use by a call.');
      return operation();
    });
    this.pending.add(task);
    void task.finally(() => this.pending.delete(task)).catch(() => {});
    return task;
  }
}
export const callAudioFocus = new CallAudioFocus();
