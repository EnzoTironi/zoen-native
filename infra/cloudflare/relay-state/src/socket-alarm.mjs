// One shared alarm owner; no socket, identity, session or membership authority.
export class SocketAlarm {
  #ctx;
  #deadline;
  #flight = null;
  #dirty = false;
  constructor(ctx, deadline) {
    this.#ctx = ctx;
    this.#deadline = deadline;
  }
  schedule() {
    this.#dirty = true;
    if (this.#flight) return this.#flight;
    const flight = Promise.resolve().then(async () => {
      try {
        do {
          this.#dirty = false;
          const installed = await this.#ctx.storage.getAlarm();
          // Read current SQL/work AFTER the awaited read, never reuse an earlier
          // expiry/pending snapshot. A request during a write dirties another pass.
          const deadline = this.#deadline();
          if (deadline !== null && (!Number.isSafeInteger(deadline) || deadline < 0))
            throw new TypeError("Invalid shared socket alarm deadline");
          if (deadline !== null) {
            if (installed === null || installed > deadline)
              await this.#ctx.storage.setAlarm(deadline);
          } else if (installed !== null) await this.#ctx.storage.deleteAlarm();
        } while (this.#dirty);
      } finally {
        // Synchronous clear before settlement: no caller can join a finished
        // flight whose last dirty check has already happened. Failure does not
        // create an immediate retry loop; the next genuine event/alarm retries.
        this.#flight = null;
      }
    });
    this.#flight = flight;
    this.#ctx.waitUntil(flight);
    return flight;
  }
}
