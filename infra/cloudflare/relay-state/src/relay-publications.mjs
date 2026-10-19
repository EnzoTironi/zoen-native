// Server-only publication control state. Membership/log/MLS remain in ClosedSpace
// and shared Rust; this object owns no alternate permission or ciphertext history.
const MAX_ROWS = 64;
const MAX_BYTES = 4 * 1024 * 1024;
const TICKET_MS = 30_000;
const RETRY_MS = 1000;
const DRAIN_ROWS = 8;
const DRAIN_WAIT_MS = 4000;
const MAX_U64 = 18446744073709551615n;
// Multiple Space instances can share one isolate. Reserve before reading a
// canonical event; do not queue unbounded per-object frames behind hung RPCs.
const MAX_ISOLATE_CALLBACKS = 4;
let isolateCallbacks = 0;
function callbackLease() {
  if (isolateCallbacks >= MAX_ISOLATE_CALLBACKS) return null;
  isolateCallbacks++;
  let released = false;
  return () => {
    if (!released) {
      released = true;
      isolateCallbacks--;
    }
  };
}
const utf8 = new TextEncoder();
const key = (value) => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
const fail = (reason, code = "relay_publication") =>
  new Error(JSON.stringify({ code, reason, permanent: false }));
function ordinal(value) {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]{0,19})$/.test(value) || BigInt(value) > MAX_U64)
    throw fail("invalid relay dispatch ordinal");
  return value.padStart(20, "0");
}
function target(value) {
  if (value == null) return null;
  if (
    Object.keys(value).sort().join(",") !== "identity,registered" ||
    !key(value.identity) ||
    typeof value.registered !== "boolean"
  )
    throw fail("invalid trusted relay target");
  return { identity: value.identity, registered: value.registered };
}
function binding(metadata, input, grants) {
  if (!key(input.origin_connection)) throw fail("invalid relay origin connection");
  if (
    !Array.isArray(grants) ||
    grants.some((value) => !key(value)) ||
    new Set(grants).size !== grants.length
  )
    throw fail("invalid Rust relay discovery grants");
  return {
    space_id: metadata.space_id,
    frame_hash: metadata.frame_hash,
    identity: metadata.identity,
    device: metadata.device,
    client_id: metadata.client_id,
    origin_connection: input.origin_connection,
    target: target(input.target),
    grants,
  };
}
function refusal(error) {
  let value;
  try {
    value = JSON.parse(error.message ?? String(error));
  } catch {
    return null;
  }
  if (value.code === "relay_preparation_unavailable") return null;
  return typeof value.reason === "string" && typeof value.permanent === "boolean"
    ? { state: "rejected", reason: value.reason, permanent: value.permanent }
    : null;
}

/** Hooks are closures over ClosedSpace's real private store/Rust operations.
 * verify(input): exact frame metadata; preview(input): actual current Rust preview;
 * publish(input): existing transaction BODY only, no nested transaction;
 * snapshot(space): actual Space head; event(space,seq): canonical stored bytes;
 * nowMs(), dependencies(), dispatch(batch): fixed Directory callback only.
 */
export class RelayPublications {
  #ctx;
  #sql;
  #host;
  #drain = null;
  constructor(ctx, host) {
    this.#ctx = ctx;
    this.#sql = ctx.storage.sql;
    this.#host = host;
    this.#sql.exec(`CREATE TABLE IF NOT EXISTS relay_space_mode (
      singleton INTEGER PRIMARY KEY CHECK(singleton=1),space_id TEXT NOT NULL)`);
    this.#sql.exec(`CREATE TABLE IF NOT EXISTS relay_preparations (
      id TEXT PRIMARY KEY,binding TEXT NOT NULL UNIQUE,expires_ms INTEGER NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('prepared','committed')),dispatch_key TEXT)`);
    this.#sql.exec(`CREATE TABLE IF NOT EXISTS relay_dispatch_counter (
      singleton INTEGER PRIMARY KEY CHECK(singleton=1),next_key TEXT NOT NULL,acked_key TEXT NOT NULL)`);
    this.#sql.exec(`CREATE TABLE IF NOT EXISTS relay_publication_outbox (
      ordinal_key TEXT PRIMARY KEY,metadata TEXT NOT NULL)`);
  }
  #one(query, ...args) {
    return this.#sql.exec(query, ...args).toArray()[0] ?? null;
  }
  #now() {
    const value = Number(this.#host.nowMs());
    if (!Number.isSafeInteger(value) || value < 0 || value > Number.MAX_SAFE_INTEGER - TICKET_MS)
      throw fail("invalid relay clock");
    return value;
  }
  #token() {
    return Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
  }
  #cleanup(now) {
    this.#sql.exec("DELETE FROM relay_preparations WHERE status='prepared' AND expires_ms<=?", now);
  }
  #scope(space, allowCreate = false) {
    const row = this.#one("SELECT space_id FROM relay_space_mode WHERE singleton=1");
    if (row) {
      if (row.space_id !== space) throw fail("relay Space scope mismatch");
      return;
    }
    if (!allowCreate) throw fail("Space relay composition is not initialized");
    if (this.#host.snapshot(space).head !== null)
      throw fail("nonempty precomposition Space is unsupported; use a fresh verified realm");
    this.#sql.exec("INSERT INTO relay_space_mode VALUES (1,?)", space);
  }
  #eligible(space) {
    const row = this.#one("SELECT space_id FROM relay_space_mode WHERE singleton=1");
    if (row) {
      if (row.space_id !== space) throw fail("relay Space scope mismatch");
      return;
    }
    if (this.#host.snapshot(space).head !== null)
      throw fail("nonempty precomposition Space is unsupported; use a fresh verified realm");
  }
  assertFixtureOnly(space) {
    const mode = this.#one("SELECT space_id FROM relay_space_mode WHERE singleton=1");
    if (mode) throw fail("composed Space refuses direct fixture publication");
    this.#host.snapshot(space); // Existing authoritative Space binding still fences scope.
  }
  #capacity(table, column, bytes) {
    // Table/column are internal constants, never public strings.
    const row = this.#one(
      `SELECT count(*) AS n,coalesce(sum(length(CAST(${column} AS BLOB))),0) AS bytes FROM ${table}`,
    );
    if (row.n >= MAX_ROWS || row.bytes + bytes + 512 * (row.n + 1) > MAX_BYTES)
      throw fail("relay publication capacity reached", "relay_capacity");
  }
  #prepared(row, value) {
    return {
      state: "prepared",
      preparation_id: row.id,
      space_id: value.space_id,
      frame_hash: value.frame_hash,
      identity: value.identity,
      device: value.device,
      origin_connection: value.origin_connection,
      grants: value.grants,
      expires_ms: String(row.expires_ms),
    };
  }

  async prepare(input) {
    this.#host.dependencies();
    let result;
    try {
      result = this.#ctx.storage.transactionSync(() => {
        const now = this.#now();
        this.#cleanup(now);
        const verified = this.#host.verify(input);
        this.#eligible(verified.space_id);
        const preview = this.#host.preview(input);
        if (preview.error) {
          const known = refusal(preview.error);
          if (known) return known;
          throw preview.error;
        }
        const value = binding(preview, input, preview.grants);
        this.#scope(value.space_id, true);
        const encoded = JSON.stringify(value);
        const existing = this.#one("SELECT * FROM relay_preparations WHERE binding=?", encoded);
        if (existing) return this.#prepared(existing, value);
        this.#capacity("relay_preparations", "binding", utf8.encode(encoded).byteLength);
        const row = { id: this.#token(), expires_ms: now + TICKET_MS };
        this.#sql.exec(
          "INSERT INTO relay_preparations VALUES (?,?,?,'prepared',NULL)",
          row.id,
          encoded,
          row.expires_ms,
        );
        return this.#prepared(row, value);
      });
    } catch (error) {
      const known = refusal(error);
      if (known) return known;
      throw error;
    }
    // A ticket is not returned until its durable recovery alarm is persisted.
    // Pending tickets keep the alarm alive until commit/expiry, closing the
    // append-before-setAlarm crash gap without a transaction across an RPC.
    if (result.state === "prepared") await this.#schedule();
    return result;
  }

  async commit(input) {
    this.#host.dependencies();
    let result;
    try {
      result = this.#ctx.storage.transactionSync(() => {
        if (!key(input?.preparation_id))
          throw fail("relay preparation unavailable", "relay_preparation_unavailable");
        const row = this.#one("SELECT * FROM relay_preparations WHERE id=?", input.preparation_id);
        // Missing may mean already committed+acked before an RPC response was
        // lost. Never turn that ambiguity into a false Rejected frame.
        if (!row) throw fail("relay preparation unavailable", "relay_preparation_unavailable");
        const stored = JSON.parse(row.binding);
        const verified = this.#host.verify(input);
        const actual = binding(verified, input, stored.grants);
        if (JSON.stringify(actual) !== row.binding)
          throw fail("relay preparation binding mismatch");
        this.#scope(stored.space_id);
        if (row.status === "committed") return { dispatch_key: row.dispatch_key };
        if (row.expires_ms <= this.#now()) {
          this.#sql.exec("DELETE FROM relay_preparations WHERE id=?", row.id);
          return { state: "rejected", reason: "relay preparation expired", permanent: false };
        }
        const expected = {
          preparation_id: row.id,
          space_id: stored.space_id,
          frame_hash: stored.frame_hash,
          identities: stored.grants,
        };
        const given = input.catalogue;
        if (
          !given ||
          Object.keys(given).sort().join(",") !== "frame_hash,identities,preparation_id,space_id" ||
          JSON.stringify({
            preparation_id: given.preparation_id,
            space_id: given.space_id,
            frame_hash: given.frame_hash,
            identities: given.identities,
          }) !== JSON.stringify(expected)
        )
          throw fail("relay catalogue acknowledgment does not cover the exact preparation");
        const current = this.#host.preview(input); // CURRENT actual head/membership/MLS/dedupe.
        if (current.error) {
          const known = refusal(current.error);
          if (!known) throw current.error;
          this.#sql.exec("DELETE FROM relay_preparations WHERE id=?", row.id);
          return known;
        }
        if (JSON.stringify(current.grants) !== JSON.stringify(stored.grants))
          throw fail("relay discovery grants changed; prepare again");
        const outcome = this.#host.publish(input);
        if (outcome.error) {
          const known = refusal(outcome.error);
          if (!known) throw outcome.error;
          this.#sql.exec("DELETE FROM relay_preparations WHERE id=?", row.id);
          return known;
        }
        const accepted = outcome.result;
        const counter = this.#one("SELECT * FROM relay_dispatch_counter WHERE singleton=1");
        const next = BigInt(counter?.next_key ?? "0") + 1n;
        if (next > MAX_U64) throw fail("relay dispatch ordinal exhausted", "relay_capacity");
        const dispatch_key = ordinal(String(next));
        const packet = {
          space_id: stored.space_id,
          dispatch_ordinal: String(next),
          kind: accepted.kind,
          origin_connection: stored.origin_connection,
          origin_identity: stored.identity,
          origin_device: stored.device,
          seq: accepted.event.seq,
          hash: accepted.event.hash,
          audience: accepted.audience,
          joined: accepted.joined,
        };
        const encoded = JSON.stringify(packet);
        // A late capacity throw rolls back append, invite/MLS/pruning and ticket
        // together. No accepted event can exist without its reference outbox.
        this.#capacity("relay_publication_outbox", "metadata", utf8.encode(encoded).byteLength);
        this.#sql.exec("INSERT INTO relay_publication_outbox VALUES (?,?)", dispatch_key, encoded);
        this.#sql.exec(
          `INSERT INTO relay_dispatch_counter VALUES (1,?,?) ON CONFLICT(singleton)
          DO UPDATE SET next_key=excluded.next_key`,
          dispatch_key,
          counter?.acked_key ?? ordinal("0"),
        );
        this.#sql.exec(
          "UPDATE relay_preparations SET status='committed',dispatch_key=? WHERE id=?",
          dispatch_key,
          row.id,
        );
        return { dispatch_key };
      });
    } catch (error) {
      const known = refusal(error);
      if (known) return known;
      throw error;
    }
    if (result.state === "rejected") return result;
    const acknowledged = await this.drainThrough(BigInt(result.dispatch_key).toString());
    return {
      state: "committed",
      dispatch: acknowledged ? "acked" : "pending",
      dispatch_ordinal: BigInt(result.dispatch_key).toString(),
    };
  }

  #acked(through) {
    const row = this.#one("SELECT acked_key FROM relay_dispatch_counter WHERE singleton=1");
    return row !== null && row.acked_key >= ordinal(through);
  }
  async #schedule() {
    const pending =
      this.#one("SELECT id FROM relay_preparations LIMIT 1") ||
      this.#one("SELECT ordinal_key FROM relay_publication_outbox LIMIT 1");
    if (pending) await this.#ctx.storage.setAlarm(this.#now() + RETRY_MS);
    else await this.#ctx.storage.deleteAlarm();
  }
  async #drainRows() {
    for (let n = 0; n < DRAIN_ROWS; n++) {
      const row = this.#one("SELECT * FROM relay_publication_outbox ORDER BY ordinal_key LIMIT 1");
      if (!row) return;
      const release = callbackLease();
      if (!release) return; // Durable alarm retries; no retained event or waiter.
      try {
        const packet = JSON.parse(row.metadata);
        const event = this.#host.event(packet.space_id, packet.seq);
        if (!event || event.hash !== packet.hash)
          throw fail("relay outbox canonical reference is unavailable");
        const { seq, hash, ...metadata } = packet;
        // One uncancellable callback lease is retained until actual settlement.
        // There is never an SQL transaction or origin inbound-queue wait here.
        const response = await this.#host.dispatch({ ...metadata, event });
        if (response?.acknowledged !== true) throw fail("relay dispatch was not acknowledged");
        this.#ctx.storage.transactionSync(() => {
          const first = this.#one(
            "SELECT ordinal_key FROM relay_publication_outbox ORDER BY ordinal_key LIMIT 1",
          );
          if (first?.ordinal_key !== row.ordinal_key) throw fail("relay dispatch order changed");
          this.#sql.exec(
            "DELETE FROM relay_publication_outbox WHERE ordinal_key=?",
            row.ordinal_key,
          );
          this.#sql.exec(
            "DELETE FROM relay_preparations WHERE status='committed' AND dispatch_key=?",
            row.ordinal_key,
          );
          this.#sql.exec(
            "UPDATE relay_dispatch_counter SET acked_key=? WHERE singleton=1",
            row.ordinal_key,
          );
        });
      } finally {
        release();
      } // Actual callback settlement, never caller timeout.
    }
  }
  async drainThrough(through) {
    ordinal(through);
    if (this.#acked(through)) return true;
    // A second caller does not stack timeout races onto a hung callback.
    if (this.#drain) return false;
    const work = this.#drainRows();
    this.#drain = work;
    const settled = work
      .catch(() => {})
      .finally(async () => {
        if (this.#drain === work) this.#drain = null;
        try {
          await this.#schedule();
        } catch {
          /* Durable prior ticket alarm still exists. */
        }
      });
    this.#ctx.waitUntil(settled);
    let timer;
    try {
      await Promise.race([
        settled,
        new Promise((resolve) => {
          timer = setTimeout(resolve, DRAIN_WAIT_MS);
        }),
      ]);
    } finally {
      clearTimeout(timer);
    }
    return this.#acked(through);
  }
  async alarm() {
    this.#ctx.storage.transactionSync(() => this.#cleanup(this.#now()));
    // The triggering alarm is consumed. Renew its durable wake BEFORE an await
    // to Directory, including a callback that times out but cannot be cancelled.
    await this.#schedule();
    if (!this.#drain) {
      const row = this.#one(
        "SELECT ordinal_key FROM relay_publication_outbox ORDER BY ordinal_key LIMIT 1",
      );
      if (row) await this.drainThrough(BigInt(row.ordinal_key).toString());
    }
    await this.#schedule();
  }
}
