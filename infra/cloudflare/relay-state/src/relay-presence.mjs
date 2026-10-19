// No active-device or identity authority is stored here.
// Queue and attach records contain ONLY server-owned opaque connection generations.
// Host hooks MUST restore current/ended directory_connections rows; no client facts.
const MAX_ATTACHED = 256;
const MAX_JOBS = 512;
const MAX_PUMPS = 2;
const AUDIENCE_DEADLINE_MS = 2000;
const MAX_PEERS = 5000;
const MAX_SEND_BYTES = 4 * 1024 * 1024;
const generation = (id) => typeof id === "string" && /^[0-9a-f]{64}$/.test(id);
const identity = generation;

/**
 * Trusted host hooks:
 * - current(id): actual OPEN socket + ready Directory connection/session + fresh
 *   registered/current device/cert authorization; returns {id,identity,socket} or null.
 * - ended(id): actual Directory row stage=ended ONLY, returns {id,identity}; NEVER
 *   reconstructs a Session, marks a device active, or permits a request/send.
 * - active(): <=256 actual OPEN/freshly authorized registered connection tuples.
 * - coMembers(identity,deadlineMs): complete current Space-authorized union through
 *   existing conservative catalogue; four-way RPCs per job and an independent
 *   eight-slot presence lane cap actual-unsettled calls across timed-out jobs.
 *   Its leases remain held until underlying actual settlement after timeout.
 * - control({identity,online}): actual shared Rust ServerFrame::Presence codec.
 * - send(socket,bytes): synchronous current durable authorization + actual write.
 * - discardEnded(id): deletes ONLY that ended generation's authoritative row.
 * - wake(): existing shared Directory alarm minimum; no independent alarm owner.
 * No hook is exposed via a public roster route or accepts caller auth booleans.
 */
export class RelayPresence {
  #ctx;
  #sql;
  #host;
  #running = new Set();
  #wake = null;
  constructor(ctx, host) {
    this.#ctx = ctx;
    this.#sql = ctx.storage.sql;
    this.#host = host;
    this.#sql.exec("CREATE TABLE IF NOT EXISTS relay_attached (connection_id TEXT PRIMARY KEY)");
    this.#sql.exec(`CREATE TABLE IF NOT EXISTS relay_presence_jobs (
      id INTEGER PRIMARY KEY AUTOINCREMENT, connection_id TEXT NOT NULL,
      kind TEXT NOT NULL CHECK(kind IN ('online','offline')), UNIQUE(connection_id,kind))`);
    // Constructor does NOT attach, announce, authenticate, or manufacture jobs.
    // The existing Directory alarm resumes a persisted actual pending job only.
  }
  #one(query, ...args) {
    return this.#sql.exec(query, ...args).toArray()[0] ?? null;
  }
  #attached(id) {
    return Boolean(this.#one("SELECT connection_id FROM relay_attached WHERE connection_id=?", id));
  }
  pending() {
    if (this.#one("SELECT CAST(id AS TEXT) AS id FROM relay_presence_jobs LIMIT 1")) return true;
    // A durable actual-ended marker is recoverable work even if its first queue
    // INSERT failed. Keep the shared alarm alive until a later kick reconciles it.
    for (const row of this.#sql.exec(
      "SELECT connection_id FROM relay_attached ORDER BY connection_id LIMIT 256",
    )) {
      if (this.#host.ended(row.connection_id)) return true;
    }
    return false;
  }
  isAttached(id) {
    return generation(id) && this.#attached(id);
  }
  #enqueue(id, kind) {
    if (
      this.#one(
        "SELECT CAST(id AS TEXT) AS id FROM relay_presence_jobs WHERE connection_id=? AND kind=?",
        id,
        kind,
      )
    )
      return true;
    if (this.#one("SELECT count(*) AS n FROM relay_presence_jobs").n >= MAX_JOBS) return false;
    this.#sql.exec("INSERT INTO relay_presence_jobs(connection_id,kind) VALUES (?,?)", id, kind);
    return true;
  }
  /** After Ready for registered Auth, or first successful Register before Res.
   * Attach is synchronous/idempotent. Caller then sends authoritative own stock
   * before any async presence job can run. Repeated Register does not reattach.
   */
  attach(id) {
    if (!generation(id)) throw new TypeError("Invalid server connection generation");
    const current = this.#host.current(id);
    if (!current || !identity(current.identity)) return false;
    let inserted = false;
    this.#ctx.storage.transactionSync(() => {
      if (this.#attached(id)) return;
      if (this.#one("SELECT count(*) AS n FROM relay_attached").n >= MAX_ATTACHED) return;
      this.#sql.exec("INSERT INTO relay_attached VALUES (?)", id);
      inserted = true;
      // Presence is best effort. Queue capacity cannot corrupt/fail registration.
      this.#enqueue(id, "online");
    });
    if (inserted) this.#schedule();
    return inserted;
  }
  /** Close initiation changes authoritative Directory stage to closing, removes
   * session/challenge authority, and cancels this attach's pending online notice.
   * It does NOT erase the attach marker or announce offline before transport end.
   */
  closing(id) {
    if (!generation(id)) return;
    this.#sql.exec("DELETE FROM relay_presence_jobs WHERE connection_id=? AND kind='online'", id);
  }
  /** Call ONLY from actual runtime transport-end handler after Directory row
   * becomes stage=ended. No SQL-retirement/timeout/readyState=2 shortcut.
   */
  transportEnded(id) {
    if (!generation(id)) return;
    const ended = this.#host.ended(id);
    if (!ended || !identity(ended.identity)) return;
    // Duplicate terminal callbacks cannot erase the source of an already queued
    // offline job, including one whose co-member RPC is currently in flight.
    if (
      this.#one(
        "SELECT CAST(id AS TEXT) AS id FROM relay_presence_jobs WHERE connection_id=? AND kind='offline'",
        id,
      )
    )
      return;
    let queued = false;
    this.#ctx.storage.transactionSync(() => {
      const wasAttached = this.#attached(id);
      this.#sql.exec("DELETE FROM relay_attached WHERE connection_id=?", id);
      this.#sql.exec("DELETE FROM relay_presence_jobs WHERE connection_id=? AND kind='online'", id);
      if (wasAttached) queued = this.#enqueue(id, "offline");
      // Same local SQL transaction: a discard fault must not erase the only
      // generation marker that lets a later real wake recover this ended row.
      if (!queued) this.#host.discardEnded(id);
    });
    if (queued) this.#schedule();
  }
  #active() {
    const rows = this.#host.active();
    if (!Array.isArray(rows) || rows.length > MAX_ATTACHED)
      throw new Error("Presence socket bound exceeded");
    return rows.filter(
      (row) => generation(row.id) && identity(row.identity) && this.#attached(row.id),
    );
  }
  #online(who) {
    return this.#active().some((row) => row.identity === who);
  }
  #recipients(peers) {
    return this.#active().filter((row) => peers.has(row.identity));
  }
  #validatePeers(value, subject) {
    if (!Array.isArray(value) || value.length > MAX_PEERS)
      throw new Error("Invalid complete co-member union");
    const peers = new Set();
    for (const key of value) {
      if (!identity(key) || key === subject)
        throw new Error("Invalid co-member subject or identity");
      peers.add(key);
    }
    return peers;
  }
  #schedule() {
    // One uncancellable alarm-installation lease, not one new Promise per attach.
    if (this.#wake) return;
    const wake = Promise.resolve().then(() => this.#host.wake());
    this.#wake = wake;
    const settled = () => {
      if (this.#wake === wake) this.#wake = null;
    };
    this.#ctx.waitUntil(
      wake.then(
        () => {
          settled();
          this.kick();
        },
        () => {
          settled();
          // Persisted best-effort jobs remain bounded, visible to the next actual
          // Directory event/alarm. No lost wake is called delivered or accepted.
          // Adapter must record this failure; no unbounded immediate retry loop.
        },
      ),
    );
  }
  kick() {
    try {
      // Only reconcile a genuinely recorded terminal row. If its queue write
      // previously failed, the old generation marker is still durable; this does
      // not manufacture an attach or infer physical end from a closing socket.
      for (const row of this.#sql.exec(
        "SELECT connection_id FROM relay_attached ORDER BY connection_id LIMIT 256",
      )) {
        try {
          if (this.#host.ended(row.connection_id)) this.transportEnded(row.connection_id);
        } catch {
          // Keep the bounded marker for the next real event/alarm, no hot retry.
        }
      }
      for (const job of this.#sql.exec(
        "SELECT CAST(id AS TEXT) AS id,connection_id,kind FROM relay_presence_jobs ORDER BY relay_presence_jobs.id LIMIT 2",
      )) {
        if (this.#running.size >= MAX_PUMPS) break;
        if (this.#running.has(job.id)) continue;
        this.#running.add(job.id);
        this.#ctx.waitUntil(this.#run(job));
      }
    } catch {
      // A SQL read fault does not create a detached rejection or hot retry loop.
      // Existing markers/jobs remain authoritative input to the next real wake.
    }
  }
  async #run(job) {
    try {
      if (job.kind === "online") {
        const before = this.#host.current(job.connection_id);
        if (!before || !this.#attached(job.connection_id)) return;
        const peers = this.#validatePeers(
          await this.#host.coMembers(before.identity, AUDIENCE_DEADLINE_MS),
          before.identity,
        );
        // Session-drop cancels the native presence future. Revalidate after await;
        // an old close/hibernation task cannot reattach or resurrect the device.
        const current = this.#host.current(job.connection_id);
        if (!current || current.identity !== before.identity || !this.#attached(job.connection_id))
          return;
        const bytes = this.#host.control({ identity: current.identity, online: true });
        const recipients = this.#recipients(peers);
        const snapshot = [
          ...new Set(
            this.#active()
              .filter((row) => peers.has(row.identity))
              .map((row) => row.identity),
          ),
        ];
        const snapshotBytes = snapshot.map((who) =>
          this.#host.control({ identity: who, online: true }),
        );
        const volume =
          recipients.length * bytes.byteLength +
          snapshotBytes.reduce((n, frame) => n + frame.byteLength, 0);
        if (volume > MAX_SEND_BYTES) return;
        // Always announce each genuine attach, including a sibling device attach.
        // Announcement then connecting-device snapshot, matching native go_online.
        for (const recipient of recipients) this.#host.send(recipient.socket, bytes);
        for (const frame of snapshotBytes) this.#host.send(current.socket, frame);
      } else if (job.kind === "offline") {
        const before = this.#host.ended(job.connection_id);
        if (!before || this.#online(before.identity)) return;
        const peers = this.#validatePeers(
          await this.#host.coMembers(before.identity, AUDIENCE_DEADLINE_MS),
          before.identity,
        );
        const ended = this.#host.ended(job.connection_id);
        // A reconnect/new sibling during lookup wins; never emit a stale offline.
        if (!ended || ended.identity !== before.identity || this.#online(ended.identity)) return;
        const bytes = this.#host.control({ identity: ended.identity, online: false });
        const recipients = this.#recipients(peers);
        if (recipients.length * bytes.byteLength > MAX_SEND_BYTES) return;
        for (const recipient of recipients) this.#host.send(recipient.socket, bytes);
      }
    } catch {
      // Native two-second co_members failure emits no partial notification set.
      // This is NOT durable message delivery, and does not fail login/registration.
    } finally {
      try {
        this.#ctx.storage.transactionSync(() => {
          this.#sql.exec("DELETE FROM relay_presence_jobs WHERE id=?", job.id);
          if (job.kind === "offline") this.#host.discardEnded(job.connection_id);
        });
        if (this.pending()) this.#schedule();
      } catch {
        // A cleanup write fault leaves a bounded persisted intent; no hot loop or
        // detached unhandled rejection. Adapter records it and next real wake retries.
      } finally {
        this.#running.delete(job.id);
      }
    }
  }
}
