import { RelayPresence } from "./relay-presence.mjs";
// Authenticated protocol composition; Directory owns sessions/devices and Space owns log/membership.
// Host hooks below MUST close over DirectoryState's actual private SQL/session methods.
// Tests may pause I/O but must not supply authenticated facts or replace these hooks.
const MAX_FRAME = 1024 * 1024;
const MAX_CONNECTION_JOBS = 16;
const MAX_CONNECTION_BYTES = 2 * 1024 * 1024;
const MAX_CELL_BYTES = 8 * 1024 * 1024;
const MAX_PUMPS = 32;
const MAX_UNSETTLED_RPC = 32;
const MAX_UNSETTLED_READS = 2;
const MAX_UNSETTLED_PRESENCE = 8;
const JOBS_PER_TURN = 4;
const RPC_TIMEOUT_MS = 5000;
const SYNC_DEADLINE_MS = 30000;
const MAX_CANDIDATES = 4096;
const MAX_FACT_BYTES = 768 * 1024;
const DISCOVERY_TOTAL = 65536;
const MAX_DISCOVERY_BYTES = 16 * 1024 * 1024;
const encoder = new TextEncoder();
const failure = (reason) => new Error(JSON.stringify({ reason, permanent: false }));
const key = (value) => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
const decimal = (value) =>
  typeof value === "string" &&
  /^(0|[1-9][0-9]{0,19})$/.test(value) &&
  BigInt(value) <= 18446744073709551615n;

/**
 * Fixed, server-only Space service binding passed as `spaces` by Directory.
 * host hooks:
 * - socketId/socketFor/socketRow: existing opaque attachment and SQL lookups.
 * - sockets(): actual generation-local handles only; Directory owns authorization.
 * - incoming/control/event/plan: shared Rust/WASM only.
 * - admit(connectionId,bytes,input): synchronous Directory transaction returning
 *   {state:'admitted',identity,device,session:{authenticated_identity,authenticated_device},target,
 *    origin_connection,space,client_id,frame_hash}. It restores actual session/device, requires
 *   registration for Publish/Sync, commits native request/publish rate charges in
 *   native device-before-account order, and runs verifyPublish before target SQL.
 * - reason(error): existing _socketFailure; refused(id,reason): actual native Res codec.
 * - request(admission,bytes): existing Directory request() (fresh own fence).
 * - ping(admission)/error(reason,permanent): actual native control frame bytes.
 * - send(socket,bytes): actual _sendSocket, including fresh durable auth + byte/rate
 *   policy in its synchronous send turn; returns false only after closing socket.
 * - close(socket,code): existing _socketClose (also deletes that connection's jobs).
 * - wake(): existing shared alarm scheduler taking min(socket expiry,pending wake).
 * - assertCurrent(connectionId): current Directory authorization, synchronous.
 * - activateCatalogue(): real fresh-cell genesis invariant; NEVER a caller true flag.
 */
export class RelayComposition {
  #ctx;
  #sql;
  #spaces;
  #host;
  #presence;
  #pumps = new Set();
  #unsettled = new Set();
  #unsettledReads = new Set();
  #unsettledPresence = new Set();
  constructor(ctx, spaces, host) {
    this.#ctx = ctx;
    this.#sql = ctx.storage.sql;
    this.#spaces = spaces;
    this.#host = host;
    this.#sql.exec(`CREATE TABLE IF NOT EXISTS relay_pending (
      id INTEGER PRIMARY KEY AUTOINCREMENT, connection_id TEXT NOT NULL,
      bytes BLOB NOT NULL, size INTEGER NOT NULL)`);
    this.#sql.exec(
      "CREATE INDEX IF NOT EXISTS relay_pending_connection ON relay_pending(connection_id,id)",
    );
    this.#sql.exec(`CREATE TABLE IF NOT EXISTS relay_discovery (
      identity TEXT NOT NULL,space_id TEXT NOT NULL,PRIMARY KEY(identity,space_id))`);
    this.#presence = new RelayPresence(ctx, {
      ...host.presence,
      coMembers: (identity, timeoutMs) => this.#coMembers(identity, timeoutMs, 4, "presence"),
      control: (notice) => host.control({ kind: "presence", ...notice }),
      send: (socket, bytes) => host.send(socket, bytes),
      wake: () => host.wake(),
    });
  }
  attached(connectionId) {
    return this.#presence.attach(connectionId);
  }
  presenceAttached(connectionId) {
    return this.#presence.isAttached(connectionId);
  }
  presenceClosing(connectionId) {
    this.#presence.closing(connectionId);
  }
  presenceEnded(connectionId) {
    this.#presence.transportEnded(connectionId);
  }

  #one(query, ...args) {
    return this.#sql.exec(query, ...args).toArray()[0] ?? null;
  }
  pending() {
    return (
      Boolean(this.#one("SELECT CAST(id AS TEXT) AS id FROM relay_pending LIMIT 1")) ||
      this.#presence.pending()
    );
  }
  drop(connectionId) {
    this.#sql.exec("DELETE FROM relay_pending WHERE connection_id=?", connectionId);
  }

  enqueue(socket, bytes) {
    if (!(bytes instanceof Uint8Array) || !bytes.length || bytes.length > MAX_FRAME) {
      this.#host.close(socket, 1003);
      return;
    }
    const id = this.#host.socketId(socket);
    if (!id) {
      this.#host.close(socket, 1008);
      return;
    }
    try {
      this.#ctx.storage.transactionSync(() => {
        const own = this.#one(
          "SELECT count(*) AS n,coalesce(sum(size),0) AS bytes FROM relay_pending WHERE connection_id=?",
          id,
        );
        const total = this.#one("SELECT coalesce(sum(size),0) AS bytes FROM relay_pending");
        if (
          own.n >= MAX_CONNECTION_JOBS ||
          own.bytes + bytes.length > MAX_CONNECTION_BYTES ||
          total.bytes + bytes.length > MAX_CELL_BYTES
        )
          throw failure("relay pending capacity reached");
        this.#sql.exec(
          "INSERT INTO relay_pending(connection_id,bytes,size) VALUES (?,?,?)",
          id,
          bytes,
          bytes.length,
        );
      });
    } catch {
      this.#host.close(socket, 1013);
      return;
    }
    // Persist durable alarm wake before any external RPC. wake() must share the
    // Directory alarm minimum instead of racing a second independent scheduler.
    this.#ctx.waitUntil(
      this.#wake()
        .then(() => this.kick())
        .catch(() => {}),
    );
  }
  kick() {
    this.#presence.kick();
    for (const row of this.#sql.exec(
      "SELECT connection_id,CAST(min(id) AS TEXT) AS oldest FROM relay_pending GROUP BY connection_id ORDER BY min(id) LIMIT 32",
    )) {
      if (this.#pumps.size >= MAX_PUMPS) break;
      if (this.#pumps.has(row.connection_id)) continue;
      this.#pumps.add(row.connection_id);
      this.#ctx.waitUntil(this.#pump(row.connection_id));
    }
  }
  async #wake() {
    try {
      await this.#host.wake();
    } catch (error) {
      // No unhandled rejection or accepted pending work with a lost alarm.
      // Closing bounded sockets makes queued, not-yet-admitted work retryable.
      for (const socket of this.#host.sockets()) {
        try {
          this.#host.close(socket, 1011);
        } catch {}
      }
      throw error;
    }
  }
  async #rpc(operation, read = false, timeoutMs = RPC_TIMEOUT_MS, lane = "ordinary") {
    if (read && this.#unsettledReads.size >= MAX_UNSETTLED_READS)
      throw failure("relay read capacity reached");
    if (this.#unsettled.size >= MAX_UNSETTLED_RPC)
      throw failure("relay dependency capacity reached");
    if (lane === "presence" && this.#unsettledPresence.size >= MAX_UNSETTLED_PRESENCE)
      throw failure("presence dependency capacity reached");
    const lease = {};
    this.#unsettled.add(lease);
    if (read) this.#unsettledReads.add(lease);
    if (lane === "presence") this.#unsettledPresence.add(lease);
    let underlying;
    try {
      underlying = Promise.resolve(operation());
    } catch (error) {
      this.#unsettled.delete(lease);
      this.#unsettledReads.delete(lease);
      this.#unsettledPresence.delete(lease);
      throw error;
    }
    // A timeout does not cancel a Workers RPC. Keep its slot until real settle,
    // so new sockets cannot accumulate unbounded abandoned in-flight promises.
    const release = () => {
      this.#unsettled.delete(lease);
      this.#unsettledReads.delete(lease);
      this.#unsettledPresence.delete(lease);
    };
    underlying.then(release, release);
    let timeout;
    try {
      return await Promise.race([
        underlying,
        new Promise((_, reject) => {
          timeout = setTimeout(() => reject(failure("relay dependency timeout")), timeoutMs);
        }),
      ]);
    } finally {
      clearTimeout(timeout);
    }
  }
  async #pump(id) {
    try {
      let handled = 0;
      while (handled++ < JOBS_PER_TURN) {
        const queued = this.#one(
          "SELECT CAST(id AS TEXT) AS id,bytes FROM relay_pending WHERE connection_id=? ORDER BY relay_pending.id LIMIT 1",
          id,
        );
        if (!queued) return;
        const socket = this.#host.socketFor(id);
        if (!socket) {
          this.drop(id);
          return;
        }
        const bytes = new Uint8Array(queued.bytes);
        let input;
        try {
          input = this.#host.incoming(bytes);
        } catch {
          this.#host.close(socket, 1003);
          this.drop(id);
          return;
        }
        let admitted;
        try {
          admitted = this.#host.admit(id, bytes, input);
        } catch {
          this.#host.close(socket, 1011);
          this.drop(id);
          return;
        }
        if (admitted.state === "unauthorized") {
          this.#host.close(socket, 1008);
          this.drop(id);
          return;
        }
        if (admitted.state === "refused") {
          if (input.kind === "request")
            this.#host.send(socket, this.#host.refused(input.id, admitted.reason));
          else if (input.kind === "publish" && admitted.frame === "rejected")
            this.#reject(socket, input, admitted.reason, admitted.permanent);
          else this.#host.send(socket, this.#host.error(admitted.reason, admitted.permanent));
        } else if (admitted.state !== "admitted") {
          this.#host.close(socket, 1011);
          this.drop(id);
          return;
        } else if (input.kind === "request") {
          await this.#request(socket, bytes, input, admitted);
        } else if (input.kind === "ping") this.#host.send(socket, this.#host.ping(admitted));
        else if (input.kind === "publish") await this.#publish(socket, bytes, admitted);
        else if (input.kind === "sync") {
          try {
            await this.#sync(socket, bytes, input, admitted);
          } catch (error) {
            this.#host.send(socket, this.#host.error(this.#host.reason(error).reason, false));
          }
        } else this.#host.send(socket, this.#host.error("unsupported relay operation", true));
        this.#sql.exec("DELETE FROM relay_pending WHERE id=? AND connection_id=?", queued.id, id);
        if (!this.#host.socketFor(id)) {
          this.drop(id);
          return;
        }
      }
    } catch {
      const socket = this.#host.socketFor(id);
      if (socket) this.#host.close(socket, 1011);
      this.drop(id);
    } finally {
      this.#pumps.delete(id);
      await this.#wake().catch(() => {});
    }
  }

  async #request(socket, bytes, input, admitted) {
    let reply;
    try {
      reply = this.#host.request(admitted, bytes);
    } catch (error) {
      this.#host.send(socket, this.#host.refused(input.id, this.#host.reason(error).reason));
      return;
    }
    const effects = this.#host.effects(reply);
    if (effects.ok) {
      // Native go_online precedes first Register Res; Claim low notices precede
      // claimant Res. Notifications query real remaining stock, never input counts.
      if (input.register && !admitted.registered) {
        this.attached(admitted.origin_connection);
        this.#host.ownStock(socket, admitted);
      }
      if (effects.claimed.length) this.#host.claimedStock(effects.claimed);
      // Native PutProfile invalidates other current sessions BEFORE Reply::Done.
      // Resolve the complete audience first: a failed lookup leaks no partial set.
      if (input.profile_changed) await this.#profileChanged(input.profile_changed, admitted);
    }
    this.#host.send(socket, reply);
    // Native successful refill Res precedes the newly observed stock notice.
    if (effects.ok && input.refill) this.#host.ownStock(socket, admitted);
  }

  async #coMembers(identity, timeoutMs, width, lane = "ordinary") {
    if (!key(identity)) throw failure("invalid trusted co-member subject");
    const deadline = performance.now() + timeoutMs;
    const current = () => {
      if (performance.now() > deadline) throw failure("profile audience deadline reached");
    };
    const spaces = [];
    let after = "";
    while (true) {
      const rows = this.#sql
        .exec(
          "SELECT space_id FROM relay_discovery WHERE identity=? AND space_id>? ORDER BY space_id LIMIT 64",
          identity,
          after,
        )
        .toArray();
      for (const row of rows) spaces.push(row.space_id);
      if (spaces.length > MAX_CANDIDATES) throw failure("profile discovery capacity reached");
      if (rows.length < 64) break;
      after = rows[rows.length - 1].space_id;
    }
    const audience = new Set();
    // Each Space owns current membership. Catalogue rows are conservative hints;
    // retained rows for removal/unknown Spaces grant no invalidation audience.
    for (let offset = 0; offset < spaces.length; offset += width) {
      current();
      const results = await Promise.all(
        spaces
          .slice(offset, offset + width)
          .map((spaceId) =>
            this.#rpc(
              () => this.#spaces.coMembers({ spaceId, identity }),
              false,
              Math.max(1, Math.min(RPC_TIMEOUT_MS, deadline - performance.now())),
              lane,
            ),
          ),
      );
      current();
      for (const result of results) {
        if (!Array.isArray(result?.members) || result.members.length > 1000)
          throw failure("invalid profile audience response");
        for (const identity of result.members) {
          if (!key(identity)) throw failure("invalid profile audience identity");
          audience.add(identity);
          if (audience.size > 5000) throw failure("profile audience capacity reached");
        }
      }
    }
    audience.delete(identity);
    return [...audience];
  }

  async #profileChanged(profile, admitted) {
    try {
      if (
        profile.identity !== admitted.identity ||
        !key(profile.identity) ||
        !decimal(profile.version)
      )
        throw failure("invalid successful profile metadata");
      const audience = new Set(await this.#coMembers(admitted.identity, RPC_TIMEOUT_MS, 8));
      audience.add(admitted.identity);
      const bytes = this.#host.control({
        kind: "profile_changed",
        identity: profile.identity,
        version: profile.version,
      });
      const recipients = [];
      for (const socket of this.#host.sockets()) {
        const row = this.#host.socketRow(socket);
        if (
          socket.readyState === 1 &&
          row?.stage === "ready" &&
          row.id !== admitted.origin_connection &&
          audience.has(row.identity)
        )
          recipients.push(socket);
      }
      if (recipients.length > 256 || bytes.byteLength * recipients.length > 4 * 1024 * 1024)
        throw failure("profile recipient capacity reached");
      // Current device/certificate/expiry authorization stays in every actual send.
      for (const socket of recipients) this.#host.send(socket, bytes);
    } catch {
      // Exactly native best effort: successful storage still returns Done even
      // when co_members fails. Never send a partial audience on lookup failure.
    }
  }

  #catalogue(prepared, admitted) {
    if (
      prepared.identity !== admitted.identity ||
      prepared.device !== admitted.device ||
      prepared.origin_connection !== admitted.origin_connection ||
      prepared.space_id !== admitted.space ||
      !key(prepared.preparation_id) ||
      prepared.frame_hash !== admitted.frame_hash ||
      !key(prepared.frame_hash) ||
      !Array.isArray(prepared.grants) ||
      prepared.grants.length > 1 ||
      prepared.grants.some((id) => !key(id))
    )
      throw failure("prepared publication does not match Directory admission");
    this.#host.activateCatalogue();
    return this.#ctx.storage.transactionSync(() => {
      for (const id of prepared.grants) {
        const exists = this.#one(
          "SELECT 1 AS yes FROM relay_discovery WHERE identity=? AND space_id=?",
          id,
          prepared.space_id,
        );
        if (exists) continue;
        const own = this.#one("SELECT count(*) AS n FROM relay_discovery WHERE identity=?", id).n;
        const all = this.#one(
          "SELECT count(*) AS n,coalesce(sum(length(identity)+length(space_id)+32),0) AS bytes FROM relay_discovery",
        );
        if (
          own >= MAX_CANDIDATES ||
          all.n >= DISCOVERY_TOTAL ||
          all.bytes + id.length + prepared.space_id.length + 32 > MAX_DISCOVERY_BYTES
        )
          throw failure("relay discovery capacity reached");
        this.#sql.exec(
          "INSERT INTO relay_discovery(identity,space_id) VALUES (?,?)",
          id,
          prepared.space_id,
        );
      }
      return {
        preparation_id: prepared.preparation_id,
        space_id: prepared.space_id,
        frame_hash: prepared.frame_hash,
        identities: [...prepared.grants],
      };
    });
  }
  #reject(socket, metadata, reason, permanent) {
    this.#host.send(
      socket,
      this.#host.control({
        kind: "rejected",
        space: metadata.space,
        client_id: metadata.client_id,
        reason,
        permanent,
      }),
    );
  }
  async #publish(socket, bytes, admitted) {
    const input = {
      frame: bytes,
      session: admitted.session,
      target: admitted.target,
      origin_connection: admitted.origin_connection,
    };
    let prepared, catalogue;
    try {
      prepared = await this.#rpc(() => this.#spaces.prepareRelayPublication(input));
      if (prepared.state === "rejected") {
        this.#reject(socket, admitted, prepared.reason, prepared.permanent);
        return;
      }
      if (prepared.state !== "prepared") throw failure("invalid publication preparation response");
      catalogue = this.#catalogue(prepared, admitted);
    } catch (error) {
      // No commit RPC started: preparation and catalogue cannot append an event.
      const reason = this.#host.reason(error);
      this.#reject(socket, admitted, reason.reason, reason.permanent === true);
      return;
    }
    let committed;
    try {
      committed = await this.#rpc(() =>
        this.#spaces.commitRelayPublication({
          ...input,
          preparation_id: prepared.preparation_id,
          catalogue,
        }),
      );
    } catch {
      // Timed-out/missing/GC'd commit may have appended and lost its response.
      this.#host.close(socket, 1013);
      return;
    }
    if (committed.state === "rejected") {
      this.#reject(socket, admitted, committed.reason, committed.permanent);
      return;
    }
    if (committed.state !== "committed" || committed.dispatch !== "acked")
      this.#host.close(socket, 1013);
    // No direct Accepted here: ordered durable Space outbox callback owns it.
  }

  async #sync(socket, bytes, input, admitted) {
    this.#host.activateCatalogue();
    if (input.cursors.length > MAX_CANDIDATES) throw failure("sync cursor capacity reached");
    const explicit = new Set(input.cursors.map((cursor) => cursor.space));
    const candidates = new Set(explicit);
    if (input.all) {
      let after = "";
      while (true) {
        const rows = this.#sql
          .exec(
            "SELECT space_id FROM relay_discovery WHERE identity=? AND space_id>? ORDER BY space_id LIMIT 64",
            admitted.identity,
            after,
          )
          .toArray();
        for (const row of rows) candidates.add(row.space_id);
        if (candidates.size > MAX_CANDIDATES) throw failure("sync discovery capacity reached");
        if (rows.length < 64) break;
        after = rows[rows.length - 1].space_id;
      }
    }
    const deadline = performance.now() + SYNC_DEADLINE_MS;
    const assertDeadline = () => {
      if (performance.now() > deadline) throw failure("sync work deadline reached");
    };
    const facts = { identity: admitted.identity, current_spaces: [], removed_at: {} };
    let factBytes = encoder.encode(JSON.stringify(facts)).length;
    // Sequential prototype: bounded count but needs a bounded 8-wide I/O pool and
    // explicit overall job deadline before integration; no unbounded Promise.all.
    for (const spaceId of candidates) {
      this.#host.assertCurrent(admitted.origin_connection);
      assertDeadline();
      const visible = await this.#rpc(() =>
        this.#spaces.visibility({ spaceId, session: admitted.session }),
      );
      assertDeadline();
      if (
        typeof visible.member !== "boolean" ||
        (visible.removed_at !== null && !decimal(visible.removed_at))
      )
        throw failure("invalid Space visibility response");
      if (visible.member === true) {
        factBytes += encoder.encode(JSON.stringify(spaceId)).length + 1;
        facts.current_spaces.push(spaceId);
      } else if (explicit.has(spaceId) && visible.removed_at !== null) {
        factBytes +=
          encoder.encode(JSON.stringify(spaceId)).length +
          encoder.encode(JSON.stringify(visible.removed_at)).length +
          2;
        facts.removed_at[spaceId] = visible.removed_at;
      }
      if (factBytes > MAX_FACT_BYTES) throw failure("sync facts byte capacity reached");
    }
    const plan = this.#host.plan(bytes, { session: admitted.session, facts });
    for (const read of plan) {
      let from = read.from;
      while (true) {
        this.#host.assertCurrent(admitted.origin_connection);
        assertDeadline();
        const page = await this.#rpc(
          () =>
            this.#spaces.readAuthorized({
              spaceId: read.space,
              session: admitted.session,
              from,
              limit: 256,
            }),
          true,
        );
        assertDeadline();
        if (
          !Array.isArray(page.events) ||
          page.events.length > 256 ||
          page.done !== !page.has_more ||
          (page.next_seq === null) !== page.done
        )
          throw failure("invalid authorized Space continuation");
        let terminal = false;
        for (const event of page.events) {
          if (!decimal(event.seq)) throw failure("invalid Space event sequence");
          if (read.through !== null && BigInt(event.seq) > BigInt(read.through)) {
            terminal = true;
            break;
          }
          const canonical = this.#host.event(event.bytes); // Rust validates signature/hash and produces actual Event.
          if (
            canonical.space !== read.space ||
            canonical.seq !== event.seq ||
            canonical.hash !== event.hash ||
            canonical.prev !== event.prev
          )
            throw failure("Space event does not match canonical bytes");
          if (!this.#host.send(socket, canonical.frame)) return; // Current durable fence after every await.
        }
        if (terminal || page.done) break;
        if (!decimal(page.next_seq) || BigInt(page.next_seq) <= BigInt(from))
          throw failure("Space page did not advance");
        from = page.next_seq; // Do NOT infer EOF from a short byte-limited page.
      }
    }
    assertDeadline();
    this.#host.send(socket, this.#host.control({ kind: "sync_done" }));
    // Read/delivery receipt watermark replay is a distinct still-unimplemented
    // native parity requirement. Do not advertise that private scope as complete.
  }

  /** Called only by fixed Space service binding; bypasses inbound pump. */
  dispatch(input) {
    if (
      !decimal(input?.dispatch_ordinal) ||
      !["new", "duplicate"].includes(input.kind) ||
      !key(input.origin_connection) ||
      !key(input.origin_identity) ||
      !key(input.origin_device) ||
      !Array.isArray(input.audience) ||
      input.audience.length > 5000 ||
      input.audience.some((id) => !key(id)) ||
      (input.joined !== null && !key(input.joined))
    )
      throw failure("invalid relay dispatch shape");
    const canonical = this.#host.event(input.event?.bytes);
    if (
      canonical.space !== input.space_id ||
      canonical.seq !== input.event.seq ||
      canonical.hash !== input.event.hash ||
      canonical.prev !== input.event.prev ||
      canonical.author !== input.origin_identity ||
      (input.kind === "new" &&
        canonical.device !== null &&
        canonical.device !== input.origin_device)
    )
      throw failure("relay dispatch event binding mismatch");
    const audience = new Set(input.audience);
    const origin = this.#host.socketFor(input.origin_connection);
    const originRow = origin && this.#host.socketRow(origin);
    const validOrigin =
      originRow?.identity === input.origin_identity && originRow?.device === input.origin_device;
    let byteBudget = 4 * 1024 * 1024; // Total callback send volume; also perconnection policy in host.send.
    const send = (socket, bytes) => {
      if (bytes.length > byteBudget) {
        this.#host.close(socket, 1013);
        return false;
      }
      byteBudget -= bytes.length;
      return this.#host.send(socket, bytes); // Synchronous actual final authorization fence.
    };
    if (validOrigin)
      send(
        origin,
        this.#host.control({
          kind: "accepted",
          space: canonical.space,
          client_id: canonical.client_id,
          seq: canonical.seq,
        }),
      );
    if (input.kind === "duplicate" && validOrigin) send(origin, canonical.frame);
    const joined = input.joined !== input.origin_identity ? input.joined : null;
    // Actual sockets bounded by existing256 connection limit. No audience×devices RPC fanout.
    const sockets = this.#host.sockets();
    if (sockets.length > 256) throw failure("relay socket inventory exceeds private bound");
    for (const socket of sockets) {
      const row = this.#host.socketRow(socket);
      if (row?.stage !== "ready") continue;
      if (
        audience.has(row.identity) &&
        !(input.kind === "duplicate" && row.id === input.origin_connection)
      )
        send(socket, canonical.frame);
    }
    if (joined !== null) {
      const joinedFrame = this.#host.control({ kind: "joined", space: canonical.space });
      for (const socket of sockets)
        if (this.#host.socketRow(socket)?.identity === joined) send(socket, joinedFrame);
    }
    return { acknowledged: true };
  }
}
