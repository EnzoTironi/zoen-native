import { DurableObject } from 'cloudflare:workers';
import { initSync, directoryHello, directoryAuthenticate, directoryLookup,
  directoryRequest, directoryAuthorize } from '../generated/wasm/admission.js';
import wasmModule from '../generated/wasm/admission_bg.wasm';

initSync({ module: wasmModule });
const MAX_BYTES = 1024 * 1024;
const CHALLENGE_MS = 10_000;
// Private adapter bound, not the lifetime of a future public socket.
const SESSION_MS = 15 * 60_000;
const BOX_MS = 15 * 60_000;
const MAX_PENDING = 256;
const MAX_CLAIM_RECORDS = 128;
const GC_BATCH = 256;
function base64(value) {
  const bytes = new Uint8Array(value);
  let text = '';
  for (let offset = 0; offset < bytes.length; offset += 8192)
    text += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
  return btoa(text);
}
const unbase64 = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));

function error(reason, code = 'directory') {
  return new Error(JSON.stringify({ code, reason, permanent: code === 'directory' }));
}
function invoke(fn, ...args) {
  try { return JSON.parse(fn(...args)); }
  catch (failure) { throw typeof failure === 'string' ? new Error(failure) : failure; }
}
function frame(value) {
  if (!(value instanceof Uint8Array) || value.byteLength === 0 || value.byteLength > MAX_BYTES)
    throw error('invalid directory frame size');
  return value;
}
function token(value) {
  if (typeof value !== 'string' || !/^[a-f0-9]{64}$/.test(value)) throw error('invalid private session');
  return value;
}

/** One private transaction domain per relay cell. No raw enroll or public fetch exists.
 * Sessions are DO-owned capabilities, never caller-supplied registered/device facts.
 * authorize() is a request-admission snapshot, NOT an outbound delivery fence.
 */
export class DirectoryState extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    const sql = ctx.storage.sql;
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_identities (
      id TEXT PRIMARY KEY, handle TEXT UNIQUE NOT NULL, profile TEXT NOT NULL)`);
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_devices (
      device TEXT PRIMARY KEY, identity TEXT NOT NULL REFERENCES directory_identities(id),
      cert TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0 CHECK(revoked IN (0,1)))`);
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
    sql.exec('CREATE INDEX IF NOT EXISTS directory_packages_device ON directory_key_packages(identity,device,last_resort,id)');
    sql.exec('CREATE UNIQUE INDEX IF NOT EXISTS directory_packages_resort ON directory_key_packages(identity,device) WHERE last_resort=1');
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_publications (
      identity TEXT NOT NULL, device TEXT NOT NULL, package_hash TEXT NOT NULL, expires_secs INTEGER NOT NULL,
      PRIMARY KEY(identity,device,package_hash))`);
    sql.exec('CREATE INDEX IF NOT EXISTS directory_publications_expiry ON directory_publications(identity,device,expires_secs)');
    sql.exec(`CREATE TABLE IF NOT EXISTS directory_claims (
      source_identity TEXT NOT NULL, source_device TEXT NOT NULL, operation_id TEXT NOT NULL,
      targets TEXT NOT NULL, response BLOB NOT NULL, expires_ms INTEGER NOT NULL,
      PRIMARY KEY(source_identity,source_device,operation_id))`);
    sql.exec('CREATE INDEX IF NOT EXISTS directory_claims_expiry ON directory_claims(source_identity,source_device,expires_ms)');
  }

  _nowMs() { return Date.now(); }
  _nonce() { return this._randomToken(); }
  _randomToken() {
    return Array.from(crypto.getRandomValues(new Uint8Array(32)), byte => byte.toString(16).padStart(2, '0')).join('');
  }
  #one(query, ...args) { return this.ctx.storage.sql.exec(query, ...args).toArray()[0] ?? null; }
  #device(key) {
    const row = this.#one('SELECT identity,cert,revoked FROM directory_devices WHERE device=?', key);
    return row && { ...row, revoked: Boolean(row.revoked) };
  }
  #facts(session, lookup, now) {
    const identity = this.#one('SELECT profile FROM directory_identities WHERE id=?', session.identity);
    const owner = lookup.handle ? this.#one('SELECT id FROM directory_identities WHERE handle=?', lookup.handle) : null;
    const box = lookup.link_id ? this.#one(
      'SELECT sealed FROM directory_boxes WHERE id=? AND created_ms > ?', lookup.link_id, now - BOX_MS) : null;
    return { registered: Boolean(identity), device: this.#device(session.device),
      target: lookup.target_device ? this.#device(lookup.target_device) : null,
      profile: identity ? JSON.parse(identity.profile) : null, handle_owner: owner?.id ?? null,
      link_box: box ? Array.from(new Uint8Array(box.sealed)) : null,
      key_packages: this.#keyPackageFacts(session, lookup, now) };
  }
  #keyPackageFacts(session, lookup, now) {
    if (!['publish_key_packages', 'claim_key_packages'].includes(lookup.kind)) return null;
    const facts = { now_ms: String(now), publication_hashes: [], stock: 0, receipt: null,
      receipt_count: 0, receipt_bytes: '0', selected: [], oversized: false };
    if (lookup.kind === 'publish_key_packages') {
      facts.stock = this.#one('SELECT count(*) AS count FROM directory_key_packages WHERE identity=? AND device=? AND last_resort=0', session.identity, session.device).count;
      for (const hash of lookup.publication_hashes) {
        if (this.#one('SELECT package_hash FROM directory_publications WHERE identity=? AND device=? AND package_hash=? AND expires_secs>?', session.identity, session.device, hash, Math.floor(now / 1000)))
          facts.publication_hashes.push(hash);
      }
      return facts;
    }
    if (lookup.operation_id !== null) {
      const receipt = this.#one('SELECT targets,response,CAST(expires_ms AS TEXT) AS expires_ms FROM directory_claims WHERE source_identity=? AND source_device=? AND operation_id=? AND expires_ms>?',
        session.identity, session.device, lookup.operation_id, now);
      if (receipt) {
        facts.receipt = { targets: JSON.parse(receipt.targets), response_base64: base64(receipt.response), expires_ms: receipt.expires_ms };
        return facts;
      }
      const quota = this.#one('SELECT count(*) AS count,coalesce(sum(length(response)),0) AS bytes FROM directory_claims WHERE source_identity=? AND source_device=? AND expires_ms>?', session.identity, session.device, now);
      facts.receipt_count = quota.count; facts.receipt_bytes = String(quota.bytes);
    }
    if (lookup.targets.length) {
      // The 129th row is a capacity sentinel. Never silently truncate a target's devices.
      const rows = this.ctx.storage.sql.exec(`SELECT CAST(k.id AS TEXT) AS id,k.identity,k.device,k.data,k.last_resort
        FROM directory_key_packages k JOIN directory_devices d ON d.identity=k.identity AND d.device=k.device
        WHERE k.identity IN (${lookup.targets.map(() => '?').join(',')}) AND d.revoked=0 AND (
          (k.last_resort=0 AND NOT EXISTS(SELECT 1 FROM directory_key_packages earlier
            WHERE earlier.identity=k.identity AND earlier.device=k.device AND earlier.last_resort=0 AND earlier.id<k.id))
          OR (k.last_resort=1 AND NOT EXISTS(SELECT 1 FROM directory_key_packages single
            WHERE single.identity=k.identity AND single.device=k.device AND single.last_resort=0)))
        ORDER BY k.id LIMIT 129`, ...lookup.targets).toArray();
      facts.oversized = rows.length > MAX_CLAIM_RECORDS;
      if (!facts.oversized) facts.selected = rows.map(row => ({ id: row.id, identity: row.identity,
        device: row.device, data_base64: base64(row.data), last_resort: Boolean(row.last_resort) }));
    }
    return facts;
  }
  #session(id, now) {
    const row = this.#one('SELECT bytes,expires_ms FROM directory_sessions WHERE id=?', token(id));
    if (!row || row.expires_ms <= now) throw error('private session expired', 'private_session');
    return JSON.parse(row.bytes);
  }
  #cleanup(table, now) {
    // Both tables are capped at 256, so expiry maintenance is bounded.
    this.ctx.storage.sql.exec(`DELETE FROM ${table} WHERE expires_ms <= ?`, now);
    if (this.#one(`SELECT COUNT(*) AS count FROM ${table}`).count >= MAX_PENDING)
      throw error('private directory session capacity reached', 'private_capacity');
  }

  hello(input) {
    const bytes = frame(input?.frame);
    return this.ctx.storage.transactionSync(() => {
      const now = this._nowMs();
      this.#cleanup('directory_challenges', now);
      const nonce = this._nonce();
      const relay = this.env.RELAY_NAME;
      const result = invoke(directoryHello, bytes, JSON.stringify({ nonce, relay, now_ms: String(now) }));
      const id = this._randomToken();
      this.ctx.storage.sql.exec('INSERT INTO directory_challenges VALUES (?,?,?,?,?)',
        id, JSON.stringify(result.hello), nonce, relay, now + CHALLENGE_MS);
      return { challenge_id: id, challenge_base64: result.challenge_base64 };
    });
  }

  authenticate(input) {
    const outcome = this.ctx.storage.transactionSync(() => {
      const now = this._nowMs();
      const id = token(input?.challengeId);
      const challenge = this.#one('SELECT * FROM directory_challenges WHERE id=?', id);
      if (!challenge) return { failure: 'private challenge unavailable' };
      // Commit consumption even on a failed signature/expired challenge. Like the
      // native handshake, a failed attempt must begin again with a fresh challenge.
      this.ctx.storage.sql.exec('DELETE FROM directory_challenges WHERE id=?', id);
      if (challenge.expires_ms <= now) return { failure: 'private challenge expired' };
      const hello = JSON.parse(challenge.hello);
      let session;
      try {
        const bytes = frame(input?.frame);
        session = invoke(directoryAuthenticate, bytes, JSON.stringify({ hello, nonce: challenge.nonce,
          relay: challenge.relay, facts: this.#facts(hello, {}, now) }));
        this.#cleanup('directory_sessions', now);
      } catch (failure) { return { rejected: failure.message }; }
      const sessionId = this._randomToken();
      this.ctx.storage.sql.exec('INSERT INTO directory_sessions VALUES (?,?,?)',
        sessionId, JSON.stringify(session), now + SESSION_MS);
      return { session_id: sessionId, session };
    });
    if (outcome.failure) throw error(outcome.failure, 'private_challenge');
    if (outcome.rejected) throw new Error(outcome.rejected);
    return outcome;
  }

  authorize(input) {
    return this.ctx.storage.transactionSync(() => {
      const now = this._nowMs();
      const session = this.#session(input?.sessionId, now);
      return invoke(directoryAuthorize, JSON.stringify({ session, facts: this.#facts(session, {}, now) }));
    });
  }

  request(input) {
    const bytes = frame(input?.frame);
    return this.ctx.storage.transactionSync(() => {
      const now = this._nowMs();
      const session = this.#session(input?.sessionId, now);
      // Every request, including unsupported/replayed Req ids, is freshly fenced.
      invoke(directoryAuthorize, JSON.stringify({ session, facts: this.#facts(session, {}, now) }));
      const lookup = invoke(directoryLookup, bytes, JSON.stringify(session));
      const plan = invoke(directoryRequest, bytes, JSON.stringify({ session, facts: this.#facts(session, lookup, now) }));
      const sql = this.ctx.storage.sql;
      switch (plan.action) {
        case 'register': {
          if (plan.first) {
            sql.exec('INSERT INTO directory_identities VALUES (?,?,?)', session.identity,
              plan.profile.handle, JSON.stringify(plan.profile));
            sql.exec('INSERT INTO directory_devices(device,identity,cert) VALUES (?,?,?) ON CONFLICT(device) DO NOTHING',
              session.device, session.identity, session.cert);
          } else {
            sql.exec('UPDATE directory_identities SET handle=?,profile=? WHERE id=?',
              plan.profile.handle, JSON.stringify(plan.profile), session.identity);
          }
          session.registered = true;
          sql.exec('UPDATE directory_sessions SET bytes=? WHERE id=?', JSON.stringify(session), input.sessionId);
          break;
        }
        case 'deliver_link': {
          if (plan.device) sql.exec(
            'INSERT INTO directory_devices(device,identity,cert) VALUES (?,?,?) ON CONFLICT(device) DO NOTHING',
            plan.device.device, session.identity, plan.device.cert);
          // Native cleanup is older-than; exact 15-minute boundary is not reusable.
          sql.exec(`DELETE FROM directory_boxes WHERE id IN (
            SELECT id FROM directory_boxes WHERE created_ms < ? ORDER BY created_ms LIMIT 128)
            OR (id=? AND created_ms < ?)`, now - BOX_MS, plan.link_id, now - BOX_MS);
          const occupied = this.#one('SELECT id FROM directory_boxes WHERE id=?', plan.link_id);
          if (occupied) throw error('a link box is already there');
          sql.exec('INSERT INTO directory_boxes VALUES (?,?,?)', plan.link_id, new Uint8Array(plan.sealed), now);
          break;
        }
        case 'fetch_link':
          sql.exec('DELETE FROM directory_boxes WHERE id=? AND created_ms > ?', plan.link_id, now - BOX_MS);
          break;
        case 'unlink':
          sql.exec('UPDATE directory_devices SET revoked=1 WHERE device=? AND identity=? AND revoked=0',
            plan.device, session.identity);
          sql.exec('DELETE FROM directory_key_packages WHERE identity=? AND device=?', session.identity, plan.device);
          break;
        case 'publish_key_packages': {
          // Bounded opportunistic GC plus exact expired-key replacement. Public
          // protocol behavior matches native while unused expired metadata may linger.
          sql.exec(`DELETE FROM directory_publications WHERE rowid IN (SELECT rowid FROM directory_publications
            WHERE identity=? AND device=? AND expires_secs<=? LIMIT ${GC_BATCH})`, session.identity, session.device, plan.cleanup_before_secs);
          for (const publication of plan.publications) {
            sql.exec('DELETE FROM directory_publications WHERE identity=? AND device=? AND package_hash=? AND expires_secs<=?',
              session.identity, session.device, publication.package_hash, plan.cleanup_before_secs);
            sql.exec('INSERT INTO directory_publications VALUES (?,?,?,?)', session.identity, session.device,
              publication.package_hash, publication.expires_secs);
            sql.exec('INSERT INTO directory_key_packages(identity,device,data) VALUES (?,?,?)',
              session.identity, session.device, unbase64(publication.data_base64));
          }
          if (plan.last_resort_base64 !== null) sql.exec(`INSERT INTO directory_key_packages(identity,device,data,last_resort)
            VALUES (?,?,?,1) ON CONFLICT(identity,device) WHERE last_resort=1 DO UPDATE SET data=excluded.data`,
          session.identity, session.device, unbase64(plan.last_resort_base64));
          break;
        }
        case 'claim_key_packages':
          if (plan.operation_id !== null) sql.exec(`DELETE FROM directory_claims WHERE rowid IN (SELECT rowid FROM directory_claims
            WHERE source_identity=? AND source_device=? AND expires_ms<=? LIMIT ${GC_BATCH})`,
          session.identity, session.device, plan.cleanup_before_ms);
          for (const id of plan.delete_ids) sql.exec('DELETE FROM directory_key_packages WHERE id=?', id);
          if (plan.receipt) sql.exec('INSERT INTO directory_claims VALUES (?,?,?,?,?,?)', session.identity, session.device,
            plan.operation_id, JSON.stringify(plan.receipt.targets), unbase64(plan.receipt.response_base64), plan.receipt.expires_ms);
          break;
        default: throw error('unsupported directory plan', 'invalid_directory_context');
      }
      return { reply_base64: plan.reply_base64 };
    });
  }
}
