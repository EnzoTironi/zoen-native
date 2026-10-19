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
      link_box: box ? Array.from(new Uint8Array(box.sealed)) : null };
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
      const lookup = invoke(directoryLookup, bytes);
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
          break;
        default: throw error('unsupported directory plan', 'invalid_directory_context');
      }
      return { reply_base64: plan.reply_base64 };
    });
  }
}
