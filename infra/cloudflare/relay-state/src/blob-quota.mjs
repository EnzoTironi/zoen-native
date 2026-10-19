// Media and history chunks spend the same native weighted device budget.
import { blobRateConfig, blobRateDecision } from "../generated/wasm/admission.js";
import { policy, fail, unavailable } from "./blob-io.mjs";

export class DeviceBlobQuota {
  #ctx;
  constructor(ctx) {
    this.#ctx = ctx;
    ctx.storage.sql.exec(`CREATE TABLE IF NOT EXISTS directory_blob_limits (
      device TEXT PRIMARY KEY, tat_ns TEXT NOT NULL, last_ns TEXT NOT NULL,
      expires_ms INTEGER NOT NULL)`);
  }

  active(device) {
    let row;
    try {
      row = this.#ctx.storage.sql
        .exec("SELECT device FROM directory_devices WHERE device=? AND revoked=0", device)
        .toArray()[0];
    } catch {
      throw fail(503, "database unavailable");
    }
    if (!row) throw fail(403, "unlinked device");
  }

  charge(device, now, cost) {
    return this.#ctx.storage.transactionSync(() => {
      this.active(device);
      // A callback preserves media's device-before-body refusal precedence.
      const costKib = cost();
      const sql = this.#ctx.storage.sql;
      const previous = sql
        .exec("SELECT tat_ns,last_ns FROM directory_blob_limits WHERE device=?", device)
        .toArray()[0];
      const wall = BigInt(now) * 1_000_000n;
      const clock = previous && BigInt(previous.last_ns) > wall ? BigInt(previous.last_ns) : wall;
      const quota = policy(blobRateConfig);
      const rate = policy(
        blobRateDecision,
        JSON.stringify({
          now_ns: String(clock),
          tat_ns: previous?.tat_ns ?? null,
          cost: costKib,
          interval_ns: quota.interval_ns,
          burst: quota.burst,
        }),
      );
      if (!rate.accepted)
        throw Object.assign(fail(429, rate.body), { retryAfter: rate.retry_after });
      const expires = Number((BigInt(rate.next_ns) + 999_999n) / 1_000_000n);
      if (!Number.isSafeInteger(expires)) throw unavailable();
      sql.exec(
        "DELETE FROM directory_blob_limits WHERE device IN (SELECT device FROM directory_blob_limits WHERE expires_ms <= ? LIMIT ?)",
        now,
        256,
      );
      sql.exec(
        `INSERT INTO directory_blob_limits VALUES (?,?,?,?)
        ON CONFLICT(device) DO UPDATE SET tat_ns=excluded.tat_ns,last_ns=excluded.last_ns,expires_ms=excluded.expires_ms`,
        device,
        rate.next_ns,
        String(clock),
        expires,
      );
    });
  }
}
