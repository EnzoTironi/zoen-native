import { DurableObject } from "cloudflare:workers";
import {
  initSync,
  directoryHello,
  directoryAuthenticate,
  directoryLookup,
  directoryPreflight,
  directoryRequest,
  directoryAuthorize,
  directorySocketFrame,
  directorySocketControl,
  directorySocketDelivery,
  directoryLookupRate,
  verifyPublish,
  planSync,
  relaySocketHello,
  relaySocketFrame,
  relaySocketControl,
  relaySocketEvent,
  relaySocketRate,
  relaySocketRequestEffects,
} from "../generated/wasm/admission.js";
import wasmModule from "../generated/wasm/admission_bg.wasm";
import { ProviderAuthState } from "./provider-auth.mjs";
import { MediaBlobsState } from "./media-blobs.mjs";
import { TransferBlobsState } from "./transfer-blobs.mjs";
import { RelayComposition } from "./relay-composition.mjs";
import { SocketTransport } from "./socket-transport.mjs";
import { SocketAlarm } from "./socket-alarm.mjs";

initSync({ module: wasmModule });
const MAX_BYTES = 1024 * 1024;
const CHALLENGE_MS = 10_000;
// Private adapter bound, not the lifetime of a future public socket.
const SESSION_MS = 15 * 60_000;
const BOX_MS = 15 * 60_000;
const MAX_PENDING = 256;
const MAX_CLAIM_RECORDS = 128;
const MAX_READ_FACT_BYTES = 768 * 1024;
const utf8 = new TextEncoder();
const GC_BATCH = 256;
function base64(value) {
  const bytes = new Uint8Array(value);
  let text = "";
  for (let offset = 0; offset < bytes.length; offset += 8192)
    text += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
  return btoa(text);
}
const unbase64 = (value) => Uint8Array.from(atob(value), (char) => char.charCodeAt(0));

function error(reason, code = "directory") {
  return new Error(JSON.stringify({ code, reason, permanent: code === "directory" }));
}
function invoke(fn, ...args) {
  try {
    return JSON.parse(fn(...args));
  } catch (failure) {
    throw typeof failure === "string" ? new Error(failure) : failure;
  }
}
function frame(value) {
  if (!(value instanceof Uint8Array) || value.byteLength === 0 || value.byteLength > MAX_BYTES)
    throw error("invalid directory frame size");
  return value;
}
function token(value) {
  if (typeof value !== "string" || !/^[a-f0-9]{64}$/.test(value))
    throw error("invalid private session");
  return value;
}

/** One private transaction domain per relay cell. The outer Worker stays HTTP404.
 * Sessions are DO-owned capabilities, never caller-supplied registered/device facts.
 * authorize() is a request-admission snapshot; socket sends/deliver add the final
 * durable fence here. Internal DO fetch is available only through private bindings.
 */
export class DirectoryState extends DurableObject {
  #providerAuth;
  #mediaBlobs;
  #transferBlobs;
  #relay;
  #transport = new SocketTransport(MAX_PENDING);
  #socketAlarm;
  constructor(ctx, env) {
    super(ctx, env);
    this.#socketAlarm = new SocketAlarm(ctx, () => this._socketAlarmDeadline());
    const sql = ctx.storage.sql;
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_identities (
      id TEXT PRIMARY KEY, handle TEXT UNIQUE NOT NULL, profile TEXT NOT NULL)`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_devices (
      device TEXT PRIMARY KEY, identity TEXT NOT NULL REFERENCES directory_identities(id),
      cert TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0 CHECK(revoked IN (0,1)))`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_agreement_keys (
      identity TEXT PRIMARY KEY REFERENCES directory_identities(id), public TEXT NOT NULL,
      device TEXT NOT NULL, sig TEXT NOT NULL, cert TEXT NOT NULL)`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_sealed_profiles (
      identity TEXT PRIMARY KEY REFERENCES directory_identities(id), version TEXT NOT NULL,
      ciphertext BLOB NOT NULL, device TEXT NOT NULL, sig TEXT NOT NULL, cert TEXT NOT NULL)`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_boxes (
      id TEXT PRIMARY KEY, sealed BLOB NOT NULL, created_ms INTEGER NOT NULL)`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_challenges (
      id TEXT PRIMARY KEY, hello TEXT NOT NULL, nonce TEXT NOT NULL, relay TEXT NOT NULL,
      expires_ms INTEGER NOT NULL)`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_sessions (
      id TEXT PRIMARY KEY, bytes TEXT NOT NULL, expires_ms INTEGER NOT NULL)`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_key_packages (
      id INTEGER PRIMARY KEY AUTOINCREMENT, identity TEXT NOT NULL REFERENCES directory_identities(id),
      device TEXT NOT NULL, data BLOB NOT NULL, last_resort INTEGER NOT NULL DEFAULT 0)`);
    sql.exec(
      "CREATE INDEX IF NOT EXISTS directory_packages_device ON directory_key_packages(identity,device,last_resort,id)",
    );
    sql.exec(
      "CREATE UNIQUE INDEX IF NOT EXISTS directory_packages_resort ON directory_key_packages(identity,device) WHERE last_resort=1",
    );
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_publications (
      identity TEXT NOT NULL, device TEXT NOT NULL, package_hash TEXT NOT NULL, expires_secs INTEGER NOT NULL,
      PRIMARY KEY(identity,device,package_hash))`);
    sql.exec(
      "CREATE INDEX IF NOT EXISTS directory_publications_expiry ON directory_publications(identity,device,expires_secs)",
    );
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_claims (
      source_identity TEXT NOT NULL, source_device TEXT NOT NULL, operation_id TEXT NOT NULL,
      targets TEXT NOT NULL, response BLOB NOT NULL, expires_ms INTEGER NOT NULL,
      PRIMARY KEY(source_identity,source_device,operation_id))`);
    sql.exec(
      "CREATE INDEX IF NOT EXISTS directory_claims_expiry ON directory_claims(source_identity,source_device,expires_ms)",
    );
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_connections (
      id TEXT PRIMARY KEY, stage TEXT NOT NULL, challenge_id TEXT, session_id TEXT,
      identity TEXT, device TEXT, claim_receipts INTEGER NOT NULL DEFAULT 0, expires_ms INTEGER NOT NULL)`);
    if (this.env.RELAY_PROTOCOL_ENABLED === "true") {
      // Eagerly restore composition/presence SQL bookkeeping on every cold
      // constructor before any alarm or transport-end callback can run. Creating
      // helpers is not an attach: no existing connection is announced here.
      this.#relayProtocol();
      this.#relayGenesis();
    }
    // Ordinary handles never survive a genuine runtime replacement. Retire only
    // old connection authority, preserving actual-ended/offline work for retry.
    try {
      this._retireOrphanSocketRows();
    } finally {
      this._queueSocketAlarm();
    }
  }

  _nowMs() {
    return Date.now();
  }
  _providerCacheNow() {
    return performance.now();
  }
  _fetchProviderKeys(url, options) {
    return fetch(url, options);
  }
  provider(request) {
    this.#providerAuth ??= new ProviderAuthState(this.ctx, this.env, {
      nowMs: () => this._nowMs(),
      cacheNow: () => this._providerCacheNow(),
      randomToken: () => this._randomToken(),
      fetchKeys: (url, options) => this._fetchProviderKeys(url, options),
      authorizationChanged: () => this._retireSockets(),
    });
    return this.#providerAuth.fetch(request);
  }
  media(request) {
    this.#mediaBlobs ??= new MediaBlobsState(this.ctx, this.env, { nowMs: () => this._nowMs() });
    return this.#mediaBlobs.fetch(request);
  }
  transfer(request) {
    this.#transferBlobs ??= new TransferBlobsState(this.ctx, this.env, {
      nowMs: () => this._nowMs(),
    });
    return this.#transferBlobs.fetch(request);
  }
  _nonce() {
    return this._randomToken();
  }
  _randomToken() {
    return Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
  }
  #one(query, ...args) {
    return this.ctx.storage.sql.exec(query, ...args).toArray()[0] ?? null;
  }
  #device(key) {
    const row = this.#one(
      "SELECT identity,cert,revoked FROM directory_devices WHERE device=?",
      key,
    );
    return row && { ...row, revoked: Boolean(row.revoked) };
  }
  #facts(session, lookup, now) {
    const identity = this.#one(
      "SELECT profile FROM directory_identities WHERE id=?",
      session.identity,
    );
    const owner = lookup.handle
      ? this.#one("SELECT id FROM directory_identities WHERE handle=?", lookup.handle)
      : null;
    const box = lookup.link_id
      ? this.#one(
          "SELECT sealed FROM directory_boxes WHERE id=? AND created_ms > ?",
          lookup.link_id,
          now - BOX_MS,
        )
      : null;
    return {
      registered: Boolean(identity),
      device: this.#device(session.device),
      target: lookup.target_device ? this.#device(lookup.target_device) : null,
      profile: identity ? JSON.parse(identity.profile) : null,
      handle_owner: owner?.id ?? null,
      link_box: box ? Array.from(new Uint8Array(box.sealed)) : null,
      key_packages: this.#keyPackageFacts(session, lookup, now),
      reads: this.#readFacts(session, lookup),
      sealed_profiles: this.#encryptedFacts(lookup),
    };
  }
  #encryptedFacts(lookup) {
    if (lookup.kind !== "encrypted_directory") return null;
    const query = lookup.query;
    const facts = { query, agreements: [], profiles: [] };
    let bytes = utf8.encode(JSON.stringify(facts)).byteLength;
    const append = (target, row) => {
      const length = utf8.encode(JSON.stringify(row)).byteLength + 1;
      if (bytes + length > MAX_READ_FACT_BYTES)
        throw error(
          "encrypted directory read exceeds private byte capacity",
          "private_directory_read_capacity",
        );
      bytes += length;
      target.push(row);
    };
    if (bytes > MAX_READ_FACT_BYTES)
      throw error(
        "encrypted directory read exceeds private byte capacity",
        "private_directory_read_capacity",
      );
    if (query.kind === "agreement_keys") {
      if (query.ids.length)
        for (const row of this.ctx.storage.sql.exec(
          `SELECT identity,public,device,sig,cert FROM directory_agreement_keys
        WHERE identity IN (SELECT value FROM json_each(?))`,
          JSON.stringify(query.ids),
        ))
          append(facts.agreements, {
            identity: row.identity,
            public: row.public,
            signed: { device: row.device, sig: row.sig, cert: row.cert },
          });
    } else if (query.kind === "get_profiles" || query.kind === "put_profile") {
      const ids = query.kind === "put_profile" ? [query.identity] : query.ids;
      if (ids.length)
        for (const row of this.ctx.storage.sql.exec(
          `SELECT identity,version,ciphertext,device,sig,cert
        FROM directory_sealed_profiles WHERE identity IN (SELECT value FROM json_each(?))`,
          JSON.stringify(ids),
        ))
          append(facts.profiles, {
            identity: row.identity,
            version: row.version,
            ciphertext_base64: base64(row.ciphertext),
            signed: { device: row.device, sig: row.sig, cert: row.cert },
          });
    } else if (query.kind !== "publish_agreement") {
      throw error("unsupported encrypted directory query", "invalid_directory_context");
    }
    return facts;
  }
  #readFacts(session, lookup) {
    if (lookup.kind !== "directory_read") return null;
    const query = lookup.read;
    const facts = { query, profiles: [], devices: [] };
    // Account for the query, object/array punctuation and each selected row before
    // retaining it. Never materialize an unbounded cursor before Rust's 1MiB
    // final trusted-context backstop can run.
    let bytes = utf8.encode(JSON.stringify(facts)).byteLength;
    const capacity = () =>
      error("directory read exceeds private byte capacity", "private_directory_read_capacity");
    if (bytes > MAX_READ_FACT_BYTES) throw capacity();
    const append = (target, row) => {
      const length = utf8.encode(JSON.stringify(row)).byteLength + 1;
      if (bytes + length > MAX_READ_FACT_BYTES) throw capacity();
      bytes += length;
      target.push(row);
    };
    let rows = null;
    switch (query.kind) {
      case "lookup":
        if (query.handle)
          rows = query.prefix
            ? this.ctx.storage.sql.exec(
                "SELECT id,profile FROM directory_identities WHERE handle LIKE ? ESCAPE '\\' ORDER BY handle LIMIT 20",
                query.pattern,
              )
            : this.ctx.storage.sql.exec(
                "SELECT id,profile FROM directory_identities WHERE handle=?",
                query.handle,
              );
        break;
      case "profiles":
        // One JSON-table binding preserves native ANY semantics for all 500
        // requested IDs without exceeding workerd SQLite's bound-variable limit.
        if (query.ids.length)
          rows = this.ctx.storage.sql.exec(
            "SELECT id,profile FROM directory_identities WHERE id IN (SELECT value FROM json_each(?))",
            JSON.stringify(query.ids),
          );
        break;
      case "devices":
        // Existing private rows have no wall-clock creation metadata. SQLite's
        // stable rowid retains their enrollment order, including revoked devices.
        for (const row of this.ctx.storage.sql.exec(
          "SELECT identity,device,revoked FROM directory_devices WHERE identity=? ORDER BY rowid",
          session.identity,
        ))
          append(facts.devices, { ...row, revoked: Boolean(row.revoked) });
        break;
      default:
        throw error("unsupported directory read query", "invalid_directory_context");
    }
    // This Directory currently admits Person registrations only. Do not infer
    // authoritative kind/owner from a possibly malformed profile payload.
    if (rows)
      for (const row of rows)
        append(facts.profiles, {
          id: row.id,
          kind: "Person",
          owner: null,
          profile: JSON.parse(row.profile),
        });
    return facts;
  }
  #chargeLookup(identity, now) {
    const sql = this.ctx.storage.sql;
    // Do not cache DDL success inside a transaction that may still roll back.
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_lookup_limits (
      identity TEXT PRIMARY KEY, tat_ns TEXT NOT NULL, last_ns TEXT NOT NULL, expires_ms INTEGER NOT NULL)`);
    const previous = this.#one(
      "SELECT tat_ns,last_ns FROM directory_lookup_limits WHERE identity=?",
      identity,
    );
    const wall = BigInt(now) * 1_000_000n;
    const clock = previous && BigInt(previous.last_ns) > wall ? BigInt(previous.last_ns) : wall;
    const rate = invoke(
      directoryLookupRate,
      JSON.stringify({ now_ns: String(clock), tat_ns: previous?.tat_ns ?? null }),
    );
    if (!rate.accepted) throw error(rate.reason, "rate_limited");
    const expires = Number((BigInt(rate.next_ns) + 999_999n) / 1_000_000n);
    if (!Number.isSafeInteger(expires))
      throw error("invalid lookup clock", "invalid_directory_context");
    sql.exec(
      `DELETE FROM directory_lookup_limits WHERE identity IN (
      SELECT identity FROM directory_lookup_limits WHERE expires_ms<=? LIMIT 256)`,
      now,
    );
    sql.exec(
      `INSERT INTO directory_lookup_limits VALUES (?,?,?,?) ON CONFLICT(identity)
      DO UPDATE SET tat_ns=excluded.tat_ns,last_ns=excluded.last_ns,expires_ms=excluded.expires_ms`,
      identity,
      rate.next_ns,
      String(clock),
      expires,
    );
  }
  #keyPackageFacts(session, lookup, now) {
    if (!["publish_key_packages", "claim_key_packages"].includes(lookup.kind)) return null;
    const facts = {
      now_ms: String(now),
      publication_hashes: [],
      stock: 0,
      receipt: null,
      receipt_count: 0,
      receipt_bytes: "0",
      selected: [],
      oversized: false,
    };
    if (lookup.kind === "publish_key_packages") {
      facts.stock = this.#one(
        "SELECT count(*) AS count FROM directory_key_packages WHERE identity=? AND device=? AND last_resort=0",
        session.identity,
        session.device,
      ).count;
      for (const hash of lookup.publication_hashes) {
        if (
          this.#one(
            "SELECT package_hash FROM directory_publications WHERE identity=? AND device=? AND package_hash=? AND expires_secs>?",
            session.identity,
            session.device,
            hash,
            Math.floor(now / 1000),
          )
        )
          facts.publication_hashes.push(hash);
      }
      return facts;
    }
    if (lookup.operation_id !== null) {
      const receipt = this.#one(
        "SELECT targets,response,CAST(expires_ms AS TEXT) AS expires_ms FROM directory_claims WHERE source_identity=? AND source_device=? AND operation_id=? AND expires_ms>?",
        session.identity,
        session.device,
        lookup.operation_id,
        now,
      );
      if (receipt) {
        facts.receipt = {
          targets: JSON.parse(receipt.targets),
          response_base64: base64(receipt.response),
          expires_ms: receipt.expires_ms,
        };
        return facts;
      }
      const quota = this.#one(
        "SELECT count(*) AS count,coalesce(sum(length(response)),0) AS bytes FROM directory_claims WHERE source_identity=? AND source_device=? AND expires_ms>?",
        session.identity,
        session.device,
        now,
      );
      facts.receipt_count = quota.count;
      facts.receipt_bytes = String(quota.bytes);
    }
    if (lookup.targets.length) {
      // The 129th row is a capacity sentinel. Never silently truncate a target's devices.
      const rows = this.ctx.storage.sql
        .exec(
          `SELECT CAST(k.id AS TEXT) AS id,k.identity,k.device,k.data,k.last_resort
        FROM directory_key_packages k JOIN directory_devices d ON d.identity=k.identity AND d.device=k.device
        WHERE k.identity IN (${lookup.targets.map(() => "?").join(",")}) AND d.revoked=0 AND (
          (k.last_resort=0 AND NOT EXISTS(SELECT 1 FROM directory_key_packages earlier
            WHERE earlier.identity=k.identity AND earlier.device=k.device AND earlier.last_resort=0 AND earlier.id<k.id))
          OR (k.last_resort=1 AND NOT EXISTS(SELECT 1 FROM directory_key_packages single
            WHERE single.identity=k.identity AND single.device=k.device AND single.last_resort=0)))
        ORDER BY k.id LIMIT 129`,
          ...lookup.targets,
        )
        .toArray();
      facts.oversized = rows.length > MAX_CLAIM_RECORDS;
      if (!facts.oversized)
        facts.selected = rows.map((row) => ({
          id: row.id,
          identity: row.identity,
          device: row.device,
          data_base64: base64(row.data),
          last_resort: Boolean(row.last_resort),
        }));
    }
    return facts;
  }
  #session(id, now) {
    const row = this.#one("SELECT bytes,expires_ms FROM directory_sessions WHERE id=?", token(id));
    if (!row || row.expires_ms <= now) throw error("private session expired", "private_session");
    return JSON.parse(row.bytes);
  }
  #cleanup(table, now) {
    // Both tables are capped at 256, so expiry maintenance is bounded.
    this.ctx.storage.sql.exec(`DELETE FROM ${table} WHERE expires_ms <= ?`, now);
    if (this.#one(`SELECT COUNT(*) AS count FROM ${table}`).count >= MAX_PENDING)
      throw error("private directory session capacity reached", "private_capacity");
  }

  hello(input) {
    return this.#hello(input);
  }
  #hello(input, connectionId = null) {
    const bytes = frame(input?.frame);
    return this.ctx.storage.transactionSync(() => {
      const now = this._nowMs();
      this.#cleanup("directory_challenges", now);
      const nonce = this._nonce();
      const relay = this.env.RELAY_NAME;
      const result = invoke(
        this.env.RELAY_PROTOCOL_ENABLED === "true" ? relaySocketHello : directoryHello,
        bytes,
        JSON.stringify({ nonce, relay, now_ms: String(now) }),
      );
      const id = this._randomToken();
      this.ctx.storage.sql.exec(
        "INSERT INTO directory_challenges VALUES (?,?,?,?,?)",
        id,
        JSON.stringify(result.hello),
        nonce,
        relay,
        now + CHALLENGE_MS,
      );
      if (connectionId !== null) {
        const row = this.#one(
          "SELECT stage,expires_ms FROM directory_connections WHERE id=?",
          connectionId,
        );
        if (!row || row.stage !== "hello" || row.expires_ms <= now)
          throw error("private connection expired");
        this.ctx.storage.sql.exec(
          "UPDATE directory_connections SET stage=?,challenge_id=?,claim_receipts=?,expires_ms=? WHERE id=?",
          "auth",
          id,
          Number(invoke(directorySocketFrame, bytes).claim_receipts),
          now + CHALLENGE_MS,
          connectionId,
        );
      }
      return { challenge_id: id, challenge_base64: result.challenge_base64 };
    });
  }

  authenticate(input) {
    return this.#authenticate(input);
  }
  #authenticate(input, connectionId = null) {
    const outcome = this.ctx.storage.transactionSync(() => {
      const now = this._nowMs();
      const id = token(input?.challengeId);
      const challenge = this.#one("SELECT * FROM directory_challenges WHERE id=?", id);
      if (connectionId !== null) {
        const row = this.#one(
          "SELECT stage,challenge_id,expires_ms FROM directory_connections WHERE id=?",
          connectionId,
        );
        if (!row || row.stage !== "auth" || row.challenge_id !== id || row.expires_ms <= now)
          return { failure: "private challenge unavailable" };
      }
      if (!challenge) return { failure: "private challenge unavailable" };
      // Commit consumption even on a failed signature/expired challenge. Like the
      // native handshake, a failed attempt must begin again with a fresh challenge.
      this.ctx.storage.sql.exec("DELETE FROM directory_challenges WHERE id=?", id);
      if (challenge.expires_ms <= now) return { failure: "private challenge expired" };
      const hello = JSON.parse(challenge.hello);
      let session;
      try {
        const bytes = frame(input?.frame);
        session = invoke(
          directoryAuthenticate,
          bytes,
          JSON.stringify({
            hello,
            nonce: challenge.nonce,
            relay: challenge.relay,
            facts: this.#facts(hello, {}, now),
          }),
        );
        this.#cleanup("directory_sessions", now);
      } catch (failure) {
        return { rejected: failure.message };
      }
      const sessionId = this._randomToken();
      this.ctx.storage.sql.exec(
        "INSERT INTO directory_sessions VALUES (?,?,?)",
        sessionId,
        JSON.stringify(session),
        now + SESSION_MS,
      );
      if (connectionId !== null)
        this.ctx.storage.sql.exec(
          "UPDATE directory_connections SET stage=?,challenge_id=NULL,session_id=?,identity=?,device=?,expires_ms=? WHERE id=?",
          "ready",
          sessionId,
          session.identity,
          session.device,
          now + SESSION_MS,
          connectionId,
        );
      return { session_id: sessionId, session };
    });
    if (outcome.failure) throw error(outcome.failure, "private_challenge");
    if (outcome.rejected) throw new Error(outcome.rejected);
    return outcome;
  }

  authorize(input) {
    return this.ctx.storage.transactionSync(() => {
      const now = this._nowMs();
      const session = this.#session(input?.sessionId, now);
      return invoke(
        directoryAuthorize,
        JSON.stringify({ session, facts: this.#facts(session, {}, now) }),
      );
    });
  }

  request(input) {
    const bytes = frame(input?.frame);
    const now = this._nowMs();
    // Like the native account limiter, successful admission spends the lookup
    // budget even when later work is refused or its mutation rolls back.
    this.ctx.storage.transactionSync(() => {
      const session = this.#session(input?.sessionId, now);
      invoke(directoryAuthorize, JSON.stringify({ session, facts: this.#facts(session, {}, now) }));
      const preflight = invoke(directoryPreflight, bytes, JSON.stringify(session));
      if (preflight.charge_lookup) this.#chargeLookup(session.identity, now);
    });
    let authorizationChanged = false;
    const outcome = this.ctx.storage.transactionSync(() => {
      const session = this.#session(input?.sessionId, now);
      // Every request, including unsupported/replayed Req ids, is freshly fenced.
      invoke(directoryAuthorize, JSON.stringify({ session, facts: this.#facts(session, {}, now) }));
      const lookup = invoke(directoryLookup, bytes, JSON.stringify(session));
      const plan = invoke(
        directoryRequest,
        bytes,
        JSON.stringify({ session, facts: this.#facts(session, lookup, now) }),
      );
      authorizationChanged = plan.action === "register" || plan.action === "unlink";
      const sql = this.ctx.storage.sql;
      switch (plan.action) {
        case "read":
          break;
        case "publish_agreement": {
          const row = plan.agreement;
          sql.exec(
            `INSERT INTO directory_agreement_keys VALUES (?,?,?,?,?) ON CONFLICT(identity) DO UPDATE SET
            public=excluded.public,device=excluded.device,sig=excluded.sig,cert=excluded.cert`,
            row.identity,
            row.public,
            row.signed.device,
            row.signed.sig,
            row.signed.cert,
          );
          break;
        }
        case "put_profile": {
          // The shared planner compared exact decimal versions against this
          // transaction's row. SQLite must never coerce UInt64 through JS Number.
          const row = plan.profile;
          sql.exec(
            `INSERT INTO directory_sealed_profiles VALUES (?,?,?,?,?,?) ON CONFLICT(identity) DO UPDATE SET
            version=excluded.version,ciphertext=excluded.ciphertext,device=excluded.device,sig=excluded.sig,cert=excluded.cert`,
            row.identity,
            row.version,
            unbase64(row.ciphertext_base64),
            row.signed.device,
            row.signed.sig,
            row.signed.cert,
          );
          break;
        }
        case "register": {
          if (plan.first) {
            sql.exec(
              "INSERT INTO directory_identities VALUES (?,?,?)",
              session.identity,
              plan.profile.handle,
              JSON.stringify(plan.profile),
            );
            sql.exec(
              "INSERT INTO directory_devices(device,identity,cert) VALUES (?,?,?) ON CONFLICT(device) DO NOTHING",
              session.device,
              session.identity,
              session.cert,
            );
          } else {
            sql.exec(
              "UPDATE directory_identities SET handle=?,profile=? WHERE id=?",
              plan.profile.handle,
              JSON.stringify(plan.profile),
              session.identity,
            );
          }
          session.registered = true;
          sql.exec(
            "UPDATE directory_sessions SET bytes=? WHERE id=?",
            JSON.stringify(session),
            input.sessionId,
          );
          break;
        }
        case "deliver_link": {
          if (plan.device)
            sql.exec(
              "INSERT INTO directory_devices(device,identity,cert) VALUES (?,?,?) ON CONFLICT(device) DO NOTHING",
              plan.device.device,
              session.identity,
              plan.device.cert,
            );
          // Native cleanup is older-than; exact 15-minute boundary is not reusable.
          sql.exec(
            `DELETE FROM directory_boxes WHERE id IN (
            SELECT id FROM directory_boxes WHERE created_ms < ? ORDER BY created_ms LIMIT 128)
            OR (id=? AND created_ms < ?)`,
            now - BOX_MS,
            plan.link_id,
            now - BOX_MS,
          );
          const occupied = this.#one("SELECT id FROM directory_boxes WHERE id=?", plan.link_id);
          if (occupied) throw error("a link box is already there");
          sql.exec(
            "INSERT INTO directory_boxes VALUES (?,?,?)",
            plan.link_id,
            new Uint8Array(plan.sealed),
            now,
          );
          break;
        }
        case "fetch_link":
          sql.exec(
            "DELETE FROM directory_boxes WHERE id=? AND created_ms > ?",
            plan.link_id,
            now - BOX_MS,
          );
          break;
        case "unlink":
          sql.exec(
            "UPDATE directory_devices SET revoked=1 WHERE device=? AND identity=? AND revoked=0",
            plan.device,
            session.identity,
          );
          sql.exec(
            "DELETE FROM directory_key_packages WHERE identity=? AND device=?",
            session.identity,
            plan.device,
          );
          break;
        case "publish_key_packages": {
          // Bounded opportunistic GC plus exact expired-key replacement. Public
          // protocol behavior matches native while unused expired metadata may linger.
          sql.exec(
            `DELETE FROM directory_publications WHERE rowid IN (SELECT rowid FROM directory_publications
            WHERE identity=? AND device=? AND expires_secs<=? LIMIT ${GC_BATCH})`,
            session.identity,
            session.device,
            plan.cleanup_before_secs,
          );
          for (const publication of plan.publications) {
            sql.exec(
              "DELETE FROM directory_publications WHERE identity=? AND device=? AND package_hash=? AND expires_secs<=?",
              session.identity,
              session.device,
              publication.package_hash,
              plan.cleanup_before_secs,
            );
            sql.exec(
              "INSERT INTO directory_publications VALUES (?,?,?,?)",
              session.identity,
              session.device,
              publication.package_hash,
              publication.expires_secs,
            );
            sql.exec(
              "INSERT INTO directory_key_packages(identity,device,data) VALUES (?,?,?)",
              session.identity,
              session.device,
              unbase64(publication.data_base64),
            );
          }
          if (plan.last_resort_base64 !== null)
            sql.exec(
              `INSERT INTO directory_key_packages(identity,device,data,last_resort)
            VALUES (?,?,?,1) ON CONFLICT(identity,device) WHERE last_resort=1 DO UPDATE SET data=excluded.data`,
              session.identity,
              session.device,
              unbase64(plan.last_resort_base64),
            );
          break;
        }
        case "claim_key_packages":
          if (plan.operation_id !== null)
            sql.exec(
              `DELETE FROM directory_claims WHERE rowid IN (SELECT rowid FROM directory_claims
            WHERE source_identity=? AND source_device=? AND expires_ms<=? LIMIT ${GC_BATCH})`,
              session.identity,
              session.device,
              plan.cleanup_before_ms,
            );
          for (const id of plan.delete_ids)
            sql.exec("DELETE FROM directory_key_packages WHERE id=?", id);
          if (plan.receipt)
            sql.exec(
              "INSERT INTO directory_claims VALUES (?,?,?,?,?,?)",
              session.identity,
              session.device,
              plan.operation_id,
              JSON.stringify(plan.receipt.targets),
              unbase64(plan.receipt.response_base64),
              plan.receipt.expires_ms,
            );
          break;
        default:
          throw error("unsupported directory plan", "invalid_directory_context");
      }
      return { reply_base64: plan.reply_base64 };
    });
    if (authorizationChanged) this._retireSockets();
    return outcome;
  }

  #relayProtocol() {
    if (this.env.RELAY_PROTOCOL_ENABLED !== "true") return null;
    if (!this.env.CLOSED_RELAY) throw error("relay Space binding unavailable", "relay_unavailable");
    this.#relay ??= new RelayComposition(this.ctx, this.env.CLOSED_RELAY, {
      sockets: () => this._sockets(),
      socketId: (socket) => this._socketId(socket),
      socketFor: (id) => this.#transport.forId(id),
      socketRow: (socket) => this._socketRow(socket),
      incoming: (bytes) => invoke(relaySocketFrame, bytes),
      control: (input) => unbase64(invoke(relaySocketControl, JSON.stringify(input)).frame_base64),
      event: (bytes) => {
        const decoded = invoke(relaySocketEvent, bytes);
        return { ...decoded, frame: unbase64(decoded.frame_base64) };
      },
      plan: (bytes, context) => invoke(planSync, bytes, JSON.stringify(context)),
      admit: (id, bytes, input) => this.#relayAdmission(id, bytes, input),
      request: (admitted, bytes) =>
        unbase64(this.request({ sessionId: admitted.session_id, frame: bytes }).reply_base64),
      effects: (bytes) => invoke(relaySocketRequestEffects, bytes),
      presence: {
        current: (id) => this.#relayPresenceCurrent(id),
        active: () =>
          this._sockets().flatMap((socket) => {
            if (socket.readyState !== 1) return [];
            const current = this.#relayPresenceCurrent(this._socketId(socket));
            return current ? [current] : [];
          }),
        ended: (id) =>
          this.#one(
            "SELECT id,identity FROM directory_connections WHERE id=? AND stage='ended'",
            id,
          ),
        discardEnded: (id) =>
          this.ctx.storage.sql.exec(
            "DELETE FROM directory_connections WHERE id=? AND stage='ended'",
            id,
          ),
      },
      ownStock: (socket, admitted) => this.#relayNotifyOwnStock(socket, admitted),
      claimedStock: (devices) => this.#relayNotifyClaimedStock(devices),
      reason: (failure) => this._socketFailure(failure),
      refused: (id, reason) => this._socketControl({ kind: "refused", id, reason }),
      ping: (admitted) =>
        this._socketControl({
          kind: "pong",
          server_time_ms: admitted.claim_receipts ? String(this._nowMs()) : null,
        }),
      error: (reason, permanent) =>
        this._socketControl({ kind: "error", stage: "ready", reason, permanent }),
      send: (socket, bytes) => this._sendSocket(socket, bytes),
      close: (socket, code) => this._socketClose(socket, code),
      wake: () => this._scheduleSocketAlarm(),
      assertCurrent: (id) =>
        this.ctx.storage.transactionSync(() => {
          const row = this.#one("SELECT * FROM directory_connections WHERE id=?", id);
          this._socketAuthenticated(row, this._nowMs());
        }),
      activateCatalogue: () => this.#relayGenesis(),
    });
    return this.#relay;
  }
  #relayGenesis() {
    // A successful server-side empty-SQL check creates the marker exactly once.
    // Neither a gate flag nor a fixture caller can declare existing data complete.
    return this.ctx.storage.transactionSync(() => {
      const sql = this.ctx.storage.sql;
      sql.exec(`CREATE TABLE IF NOT EXISTS relay_catalogue_genesis (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1),version TEXT NOT NULL)`);
      const marker = this.#one("SELECT version FROM relay_catalogue_genesis WHERE singleton=1");
      if (marker) {
        if (marker.version !== "protocol4-fresh-v1")
          throw error("invalid relay genesis metadata", "relay_activation");
        return;
      }
      for (const table of [
        "directory_identities",
        "directory_devices",
        "directory_agreement_keys",
        "directory_sealed_profiles",
        "directory_boxes",
        "directory_challenges",
        "directory_sessions",
        "directory_key_packages",
        "directory_publications",
        "directory_claims",
        "directory_connections",
        "relay_pending",
        "relay_discovery",
      ]) {
        if (this.#one(`SELECT 1 AS occupied FROM ${table} LIMIT 1`))
          throw error("nonempty precomposition Directory cannot activate", "relay_activation");
      }
      for (const table of [
        "provider_challenges",
        "provider_proofs",
        "provider_bindings",
        "directory_lookup_limits",
        "directory_blob_limits",
      ]) {
        if (
          this.#one("SELECT name FROM sqlite_master WHERE type='table' AND name=?", table) &&
          this.#one(`SELECT 1 AS occupied FROM ${table} LIMIT 1`)
        )
          throw error("nonempty precomposition Directory cannot activate", "relay_activation");
      }
      sql.exec("INSERT INTO relay_catalogue_genesis VALUES(1,?)", "protocol4-fresh-v1");
      sql.exec(`CREATE TABLE IF NOT EXISTS relay_rate_limits (
        scope TEXT NOT NULL,key TEXT NOT NULL,tat_ns TEXT NOT NULL,last_ns TEXT NOT NULL,
        expires_ms INTEGER NOT NULL,PRIMARY KEY(scope,key))`);
      sql.exec(`CREATE TABLE IF NOT EXISTS relay_socket_send_usage (
        connection_id TEXT PRIMARY KEY,bytes INTEGER NOT NULL,frames INTEGER NOT NULL)`);
    });
  }
  #relayCharge(scope, key, now) {
    const sql = this.ctx.storage.sql;
    const previous = this.#one(
      "SELECT tat_ns,last_ns FROM relay_rate_limits WHERE scope=? AND key=?",
      scope,
      key,
    );
    const wall = BigInt(now) * 1_000_000n;
    const clock = previous && BigInt(previous.last_ns) > wall ? BigInt(previous.last_ns) : wall;
    const result = invoke(
      relaySocketRate,
      JSON.stringify({ scope, now_ns: String(clock), tat_ns: previous?.tat_ns ?? null }),
    );
    if (!result.accepted) return result; // Return, not throw: already-spent device quota must commit.
    const expires = Number((BigInt(result.next_ns) + 999_999n) / 1_000_000n);
    if (!Number.isSafeInteger(expires)) throw error("invalid relay rate clock", "relay_context");
    sql.exec(
      `DELETE FROM relay_rate_limits WHERE rowid IN (
      SELECT rowid FROM relay_rate_limits WHERE expires_ms<=? LIMIT 256)`,
      now,
    );
    sql.exec(
      `INSERT INTO relay_rate_limits VALUES(?,?,?,?,?) ON CONFLICT(scope,key)
      DO UPDATE SET tat_ns=excluded.tat_ns,last_ns=excluded.last_ns,expires_ms=excluded.expires_ms`,
      scope,
      key,
      result.next_ns,
      String(clock),
      expires,
    );
    return result;
  }
  #relayAdmission(id, bytes, input) {
    const now = this._nowMs();
    const first = this.ctx.storage.transactionSync(() => {
      const row = this.#one("SELECT * FROM directory_connections WHERE id=?", id);
      let session;
      try {
        session = this._socketAuthenticated(row, now);
      } catch {
        return { state: "unauthorized" };
      }
      if (["publish", "sync"].includes(input.kind) && !session.registered)
        return { state: "refused", frame: "error", reason: "register first", permanent: true };
      const admitted = {
        state: "admitted",
        identity: session.identity,
        device: session.device,
        registered: session.registered,
        origin_connection: row.id,
        session_id: row.session_id,
        claim_receipts: row.claim_receipts,
        session: { authenticated_identity: session.identity, authenticated_device: session.device },
      };
      if (["request", "publish"].includes(input.kind)) {
        const rate = this.#relayCharge(
          input.kind === "publish" ? "publish_device" : "request_device",
          session.device,
          now,
        );
        if (!rate.accepted)
          return {
            state: "refused",
            frame: input.kind === "publish" ? "rejected" : "refused",
            reason: rate.reason,
            permanent: false,
          };
      }
      return admitted;
    });
    if (first.state !== "admitted" || input.kind !== "publish") return first;
    // Separate commits preserve native device-first spending even if account
    // refuses or later signature/target validation fails. No await between them.
    let account;
    try {
      account = this.ctx.storage.transactionSync(() =>
        this.#relayCharge("publish_account", first.identity, now),
      );
    } catch {
      return {
        state: "refused",
        frame: "rejected",
        reason: "directory unavailable",
        permanent: false,
      };
    }
    if (!account.accepted)
      return { state: "refused", frame: "rejected", reason: account.reason, permanent: false };
    let verified;
    try {
      verified = invoke(verifyPublish, bytes, JSON.stringify(first.session));
    } catch (failure) {
      return { state: "refused", frame: "rejected", ...this._socketFailure(failure) };
    }
    let target = null;
    try {
      const identity = verified.lookup.target_identity;
      if (identity !== null)
        target = {
          identity,
          registered: Boolean(
            this.#one("SELECT id FROM directory_identities WHERE id=?", identity),
          ),
        };
    } catch {
      return {
        state: "refused",
        frame: "rejected",
        reason: "directory unavailable",
        permanent: false,
      };
    }
    return {
      ...first,
      space: verified.space_id,
      client_id: verified.client_id,
      frame_hash: input.frame_hash,
      target,
    };
  }
  #relayPresenceCurrent(id) {
    const socket = this.#transport.forId(id);
    if (!socket) return null;
    try {
      return this.ctx.storage.transactionSync(() => {
        const row = this.#one("SELECT * FROM directory_connections WHERE id=?", id);
        const session = this._socketAuthenticated(row, this._nowMs());
        if (
          !session.registered ||
          row.identity !== session.identity ||
          row.device !== session.device
        )
          return null;
        return { id, identity: session.identity, socket };
      });
    } catch {
      return null;
    }
  }

  #relaySendBudget(row, bytes) {
    if (this.env.RELAY_PROTOCOL_ENABLED !== "true") return;
    // INTERIM DISABLED-CANDIDATE guard only. It is not queue feedback or final
    // backpressure: negotiated perbatch credit + real slow-reader proof remain.
    const usage = this.#one(
      "SELECT bytes,frames FROM relay_socket_send_usage WHERE connection_id=?",
      row.id,
    );
    const total = (usage?.bytes ?? 0) + bytes.byteLength,
      frames = (usage?.frames ?? 0) + 1;
    if (total > 8 * 1024 * 1024 || frames > 4096)
      throw error("relay send capacity reached", "relay_send_capacity");
    this.ctx.storage.sql.exec(
      `INSERT INTO relay_socket_send_usage VALUES(?,?,?) ON CONFLICT(connection_id)
      DO UPDATE SET bytes=excluded.bytes,frames=excluded.frames`,
      row.id,
      total,
      frames,
    );
  }
  relayDispatch(input) {
    const relay = this.#relayProtocol();
    if (!relay) throw error("relay protocol disabled", "relay_unavailable");
    this.#relayGenesis();
    return relay.dispatch(input);
  }

  #relayStock(identity, device) {
    const row = this.#one(
      `SELECT (SELECT count(*) FROM directory_key_packages
      WHERE identity=? AND device=? AND last_resort=0) AS remaining
      WHERE EXISTS (SELECT 1 FROM directory_key_packages
      WHERE identity=? AND device=? AND last_resort=1)`,
      identity,
      device,
      identity,
      device,
    );
    // Match native db::key_package_stock: never-published devices aren't listed.
    return row && row.remaining < 8 ? row.remaining : null;
  }
  #relayNotifyOwnStock(socket, admitted) {
    try {
      const remaining = this.#relayStock(admitted.identity, admitted.device);
      if (remaining !== null)
        this._sendSocket(
          socket,
          unbase64(
            invoke(
              relaySocketControl,
              JSON.stringify({ kind: "key_packages_low", device: admitted.device, remaining }),
            ).frame_base64,
          ),
        );
    } catch {
      /* Like native stock notice failure, do not fabricate a count or fail Req. */
    }
  }
  #relayNotifyClaimedStock(devices) {
    if (!Array.isArray(devices) || devices.length > 128)
      throw error("invalid claim stock work", "relay_context");
    try {
      let attempts = 0;
      const sockets = this._sockets();
      for (const record of devices) {
        const remaining = this.#relayStock(record.identity, record.device);
        if (remaining === null) continue;
        const bytes = unbase64(
          invoke(
            relaySocketControl,
            JSON.stringify({ kind: "key_packages_low", device: record.device, remaining }),
          ).frame_base64,
        );
        for (const socket of sockets) {
          const row = this._socketRow(socket);
          if (row?.stage !== "ready" || row.identity !== record.identity) continue;
          if (++attempts > 1024) this._socketClose(socket, 1013);
          else this._sendSocket(socket, bytes);
        }
      }
    } catch {
      /* Authoritative notice is best-effort; the durable claim reply is preserved. */
    }
  }

  _sockets() {
    return this.#transport.sockets();
  }
  _socketId(socket) {
    return this.#transport.id(socket);
  }
  _queueSocketAlarm() {
    // A fallible terminal SQL write must not skip the next actual recovery alarm.
    this._scheduleSocketAlarm();
  }
  _terminalSocketId(id) {
    if (!id) return;
    this.ctx.storage.transactionSync(() => {
      const row = this.#one("SELECT * FROM directory_connections WHERE id=?", id);
      if (!row || row.stage === "ended") return;
      this._dropConnection(row);
      // Attached generations retain only lifecycle correlation, never a Session.
      this.ctx.storage.sql.exec(
        "UPDATE directory_connections SET stage='ended' WHERE id=? AND stage='closing'",
        id,
      );
    });
    // Also retry a previously ended generation whose first queue INSERT failed.
    // Presence preserves an existing offline job and never restores sourceSession.
    this.#relay?.presenceEnded(id);
  }
  _retireOrphanSocketRows() {
    const rows = this.ctx.storage.sql
      .exec("SELECT * FROM directory_connections ORDER BY id LIMIT 257")
      .toArray();
    if (rows.length > MAX_PENDING) throw error("connection inventory exceeds bound");
    for (const row of rows) if (!this.#transport.has(row.id)) this._terminalSocketId(row.id);
  }
  _socketRetirementPending() {
    if (this._sockets().some((socket) => socket.readyState === 3)) return true;
    const rows = this.ctx.storage.sql
      .exec("SELECT id FROM directory_connections WHERE stage!='ended' LIMIT 257")
      .toArray();
    if (rows.length > MAX_PENDING) throw error("connection inventory exceeds bound");
    return rows.some((row) => !this.#transport.has(row.id));
  }
  _socketRow(socket) {
    const id = this._socketId(socket);
    return id ? this.#one("SELECT * FROM directory_connections WHERE id=?", id) : null;
  }
  _socketClose(socket, code = 1008) {
    this.#transport.markClosing(socket);
    try {
      this.ctx.storage.transactionSync(() => {
        const row = this._socketRow(socket);
        if (row) this._dropConnection(row);
      });
    } finally {
      // Storage failure cannot keep an unauthorized/expired network socket open.
      try {
        this.#transport.close(socket, code, "private directory connection closed");
      } catch {}
    }
  }
  _dropConnection(row) {
    // A duplicate error/close cannot erase the subject of an outstanding offline
    // notification. This ended row has no Session and grants no active authority.
    if (this.#relay && row.stage === "ended") return;
    const attached = this.#relay?.presenceAttached(row.id) ?? false;
    if (attached) this.#relay.presenceClosing(row.id);
    if (this.#relay) {
      this.#relay.drop(row.id);
      this.ctx.storage.sql.exec(
        "DELETE FROM relay_socket_send_usage WHERE connection_id=?",
        row.id,
      );
    }
    if (row.challenge_id)
      this.ctx.storage.sql.exec("DELETE FROM directory_challenges WHERE id=?", row.challenge_id);
    if (row.session_id)
      this.ctx.storage.sql.exec("DELETE FROM directory_sessions WHERE id=?", row.session_id);
    if (attached) {
      // Retain public lifecycle correlation in the authoritative Directory row;
      // session authority is gone, and no closing/ended row may authenticate.
      this.ctx.storage.sql.exec(
        "UPDATE directory_connections SET stage='closing',challenge_id=NULL,session_id=NULL WHERE id=?",
        row.id,
      );
    } else this.ctx.storage.sql.exec("DELETE FROM directory_connections WHERE id=?", row.id);
  }
  _socketControl(input) {
    return unbase64(invoke(directorySocketControl, JSON.stringify(input)).frame_base64);
  }
  _socketFailure(failure) {
    try {
      const value = JSON.parse(failure.message ?? String(failure));
      if (typeof value.reason === "string") return value;
    } catch {}
    return { reason: "database unavailable", permanent: false };
  }
  _socketAuthenticated(row, now) {
    if (!row || row.stage !== "ready" || row.expires_ms <= now)
      throw error("private session expired", "private_session");
    const session = this.#session(row.session_id, now);
    const device = this.#device(session.device);
    const facts = {
      registered: Boolean(
        this.#one("SELECT id FROM directory_identities WHERE id=?", session.identity),
      ),
      device,
    };
    invoke(directoryAuthorize, JSON.stringify({ session, facts }));
    if (session.registered && device?.cert !== session.cert)
      throw error("device certificate changed", "private_session");
    return session;
  }
  _sendSocket(socket, bytes) {
    try {
      // Read authorization and enqueue the actual frame in ONE synchronous turn.
      // No await, external RPC, automatic Pong or caller-supplied auth snapshot.
      this.ctx.storage.transactionSync(() => {
        if (!this.#transport.isOpen(socket)) throw error("connection is closing");
        const row = this._socketRow(socket);
        this._socketAuthenticated(row, this._nowMs());
        this.#relaySendBudget(row, bytes);
        socket.send(bytes);
      });
      return true;
    } catch (failure) {
      let capacity = false;
      try {
        capacity = JSON.parse(failure.message).code === "relay_send_capacity";
      } catch {}
      this._socketClose(socket, capacity ? 1013 : 1008);
      return false;
    }
  }
  _retireSockets() {
    try {
      this._retireOrphanSocketRows();
      const sockets = this._sockets();
      if (!sockets.length) return;
      const now = this._nowMs();
      for (const socket of sockets) {
        const id = this._socketId(socket);
        if (socket.readyState === 3) {
          // Keep the real id/handle if SQL terminalization or enqueue fails.
          this._terminalSocketId(id);
          this.#transport.releaseTerminal(socket);
          continue;
        }
        const row = this._socketRow(socket);
        if (row?.stage === "closing" || row?.stage === "ended") continue;
        try {
          if (row && row.stage !== "ready" && row.expires_ms <= now)
            socket.send(
              this._socketControl({
                kind: "error",
                stage: row.stage,
                reason: row.stage === "hello" ? "expected hello" : "expected auth",
                permanent: true,
              }),
            );
          if (!row || row.expires_ms <= now) throw error("private connection expired");
          if (row.stage === "ready") this._socketAuthenticated(row, now);
        } catch {
          this._socketClose(socket);
        }
      }
    } finally {
      this._queueSocketAlarm();
    }
  }
  _scheduleSocketAlarm() {
    return this.#socketAlarm.schedule();
  }
  _socketAlarmDeadline() {
    const row = this.#one(
      "SELECT min(expires_ms) AS deadline FROM directory_connections WHERE stage NOT IN ('closing','ended')",
    );
    let deadline = row.deadline;
    if (this._socketRetirementPending() || this.#relay?.pending())
      deadline = Math.min(deadline ?? Infinity, this._nowMs() + 250);
    return deadline;
  }

  async alarm() {
    this.#relayProtocol();
    try {
      this._retireSockets();
      this.#relay?.kick();
    } finally {
      await this._scheduleSocketAlarm();
    }
  }

  async fetch(request) {
    if (new URL(request.url).pathname.startsWith("/v1/auth/")) return this.provider(request);
    if (
      new URL(request.url).pathname !== "/v1/sync" ||
      request.method !== "GET" ||
      request.headers.get("Upgrade")?.toLowerCase() !== "websocket"
    )
      return new Response("Not found", { status: 404 });
    this._retireSockets();
    // Private one-cell transport bound, not a claim of full public-relay capacity.
    if (
      this._sockets().filter((socket) => socket.readyState === 1 || socket.readyState === 2)
        .length >= MAX_PENDING ||
      this.#one("SELECT count(*) AS count FROM directory_connections").count >= MAX_PENDING
    )
      return new Response("Private connection capacity reached", { status: 429 });
    const pair = new WebSocketPair();
    const [client, socket] = Object.values(pair);
    const id = this._randomToken();
    this.ctx.storage.sql.exec(
      "INSERT INTO directory_connections(id,stage,expires_ms) VALUES (?, ?, ?)",
      id,
      "hello",
      this._nowMs() + CHALLENGE_MS,
    );
    try {
      this.#transport.accept(socket, id, {
        message: (actual, data) => {
          try {
            this.webSocketMessage(actual, data);
          } catch {
            this._socketClose(actual, 1011);
          }
        },
        close: (actual, code) => this.webSocketClose(actual, code),
        error: (actual) => this.webSocketError(actual),
      });
      await this._scheduleSocketAlarm();
    } catch {
      try {
        // Listener/accept failure may forget an unaccepted handle. Its exact
        // server-generated SQL id remains known here and is rolled back now.
        this.ctx.storage.transactionSync(() => {
          const owned = this.#one("SELECT * FROM directory_connections WHERE id=?", id);
          if (owned) this._dropConnection(owned);
        });
        if (socket.readyState === 0 || socket.readyState === 3) this._terminalSocketId(id);
      } finally {
        try {
          this.#transport.close(socket, 1011, "connection setup failed");
        } catch {}
        this._queueSocketAlarm();
      }
      throw new Error("Private connection scheduling unavailable");
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  webSocketMessage(socket, message) {
    let row;
    try {
      row = this._socketRow(socket);
    } catch {
      try {
        socket.close(1011, "private directory unavailable");
      } catch {}
      return;
    }
    if (!row) {
      this._socketClose(socket);
      return;
    }
    const stage = row.stage;
    const refuseHandshake = (failure) => {
      try {
        const { reason, permanent } = this._socketFailure(failure);
        socket.send(this._socketControl({ kind: "error", stage, reason, permanent }));
      } finally {
        this._socketClose(socket);
      }
    };
    if (row.expires_ms <= this._nowMs()) {
      if (stage === "ready") this._socketClose(socket);
      else refuseHandshake(error(stage === "hello" ? "expected hello" : "expected auth"));
      return;
    }
    // Text and noncanonical/oversized binary inputs are an explicit private bound.
    if (
      !(message instanceof ArrayBuffer) ||
      message.byteLength === 0 ||
      message.byteLength > MAX_BYTES
    ) {
      this._socketClose(socket, 1003);
      return;
    }
    const bytes = new Uint8Array(message);
    const relay = this.#relayProtocol();
    if (stage === "ready" && relay) {
      relay.enqueue(socket, bytes);
      return;
    }
    let input;
    try {
      input = invoke(directorySocketFrame, bytes);
    } catch {
      this._socketClose(socket, 1003);
      return;
    }
    if (stage === "hello") {
      try {
        if (input.kind !== "hello") throw error("expected hello");
        const challenge = this.#hello({ frame: bytes }, row.id);
        socket.send(unbase64(challenge.challenge_base64));
      } catch (failure) {
        refuseHandshake(failure);
        return;
      }
    } else if (stage === "auth") {
      try {
        if (input.kind !== "auth") throw error("expected auth");
        const result = this.#authenticate({ challengeId: row.challenge_id, frame: bytes }, row.id);
        const ready = this._sendSocket(
          socket,
          this._socketControl({ kind: "ready", session: result.session }),
        );
        if (ready && relay && result.session.registered) {
          relay.attached(row.id);
          this.#relayNotifyOwnStock(socket, result.session);
        }
      } catch (failure) {
        refuseHandshake(failure);
        return;
      }
    } else {
      try {
        // Admission is independently fresh, before executing even replayed requests.
        this._socketAuthenticated(row, this._nowMs());
        if (input.kind === "request") {
          let reply;
          try {
            reply = unbase64(
              this.request({ sessionId: row.session_id, frame: bytes }).reply_base64,
            );
          } catch (failure) {
            reply = this._socketControl({
              kind: "refused",
              id: input.id,
              reason: this._socketFailure(failure).reason,
            });
          }
          this._sendSocket(socket, reply);
        } else if (input.kind === "ping") {
          this._sendSocket(
            socket,
            this._socketControl({
              kind: "pong",
              server_time_ms: row.claim_receipts ? String(this._nowMs()) : null,
            }),
          );
        } else {
          const reason = ["hello", "auth"].includes(input.kind)
            ? "already logged in"
            : "private directory socket does not support Space operations";
          this._sendSocket(
            socket,
            this._socketControl({ kind: "error", stage: "ready", reason, permanent: true }),
          );
        }
      } catch {
        this._socketClose(socket);
        return;
      }
    }
    this.ctx.waitUntil(
      Promise.resolve(this._scheduleSocketAlarm()).catch(() => this._socketClose(socket, 1011)),
    );
  }
  webSocketClose(socket, code = 1000) {
    this.#relayProtocol();
    const id = this._socketId(socket);
    this.#transport.markClosing(socket);
    try {
      // This genuine runtime terminal callback never reconstructs a Session.
      this._terminalSocketId(id);
    } finally {
      try {
        this.#transport.close(
          socket,
          [1005, 1006].includes(code) ? 1000 : code,
          "private directory connection closed",
        );
      } catch {}
      this._queueSocketAlarm();
    }
  }
  webSocketError(socket) {
    this.#relayProtocol();
    const id = this._socketId(socket);
    try {
      this._socketClose(socket, 1011);
    } finally {
      try {
        // Some runtime errors are already terminal with no later close event.
        if (socket.readyState === 3) this._terminalSocketId(id);
      } finally {
        this._queueSocketAlarm();
      }
    }
  }

  // Trusted backend fanout seam only. The caller owns Space audience decisions;
  // this operation adds the durable device/session fence immediately before send.
  deliver(input) {
    token(input?.identity);
    token(input?.device);
    const bytes = frame(input?.frame);
    const canonical = unbase64(invoke(directorySocketDelivery, bytes).frame_base64);
    let delivered = 0,
      refused = 0;
    for (const socket of this._sockets()) {
      const row = this._socketRow(socket);
      if (row?.stage !== "ready" || row.identity !== input.identity || row.device !== input.device)
        continue;
      if (this._sendSocket(socket, canonical)) delivered++;
      else refused++;
    }
    return { delivered, refused };
  }
}
