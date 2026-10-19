import { DurableObject, WorkerEntrypoint } from 'cloudflare:workers';

const MAX_U64 = 18446744073709551615n;
const GENESIS_PREV = '0'.repeat(64);
// Keep one row below the SQLite row limit and one read below the Worker memory bound.
const MAX_ENTRY_BYTES = 1024 * 1024;
const MAX_ADMISSION_STATE_BYTES = 1024 * 1024;
const MAX_READ_BYTES = 4 * 1024 * 1024;
const MAX_READ_ENTRIES = 256;

function text(value, name, max = 128) {
  if (typeof value !== 'string' || value.length === 0 || value.length > max)
    throw new TypeError(`Invalid ${name}`);
  return value;
}

function sequence(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value))
    throw new TypeError('Invalid sequence');
  const parsed = BigInt(value);
  if (parsed > MAX_U64) throw new TypeError('Sequence exceeds UInt64');
  return parsed;
}

function hash(value) {
  if (typeof value !== 'string' || !/^[0-9a-f]{64}$/.test(value))
    throw new TypeError('Invalid hash');
  return value;
}

function seqKey(value) {
  sequence(value);
  return value.padStart(20, '0');
}

function entry(row) {
  return {
    seq: BigInt(row.seq_key).toString(),
    prev: row.prev,
    hash: row.hash,
    bytes: new Uint8Array(row.bytes),
  };
}

/**
 * Internal storage only. Rust owns envelope verification, membership admission,
 * MLS epoch rules and canonical Sequenced::encode()/chain_hash(). This class
 * never turns an untrusted client envelope into an admitted event.
 */
export class SpaceLog extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.ctx.storage.sql.exec(`
      CREATE TABLE IF NOT EXISTS space_binding (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), space_id TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS entries (
        seq_key TEXT PRIMARY KEY,
        author TEXT NOT NULL,
        client_id TEXT NOT NULL,
        prev TEXT NOT NULL,
        hash TEXT NOT NULL,
        bytes BLOB NOT NULL,
        UNIQUE(author, client_id)
      );
      CREATE TABLE IF NOT EXISTS admission_state (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), bytes BLOB NOT NULL);
    `);
  }

  #bind(spaceId) {
    text(spaceId, 'space ID');
    const rows = this.ctx.storage.sql.exec('SELECT space_id FROM space_binding WHERE singleton = 1').toArray();
    if (rows.length && rows[0].space_id !== spaceId)
      throw new TypeError('Space does not match this storage object');
    if (!rows.length)
      this.ctx.storage.sql.exec('INSERT INTO space_binding(singleton, space_id) VALUES (1, ?)', spaceId);
  }

  /** First log event has seq 0, matching FdbLog. Dedupe precedes the head fence. */
  appendAdmitted(input) {
    text(input?.spaceId, 'space ID');
    text(input.author, 'author', 64);
    if (!/^[0-9a-f]{64}$/.test(input.author)) throw new TypeError('Invalid author');
    text(input.clientId, 'client ID', 256);
    const seq = sequence(input.event?.seq);
    const prev = hash(input.event.prev);
    const chainHash = hash(input.event.hash);
    if (!(input.event.bytes instanceof Uint8Array) || input.event.bytes.byteLength === 0 || input.event.bytes.byteLength > MAX_ENTRY_BYTES)
      throw new TypeError('Invalid canonical event bytes');
    // Rust's opaque membership/MLS/checkpoint projection advances in the same
    // transaction as the head, so a later Rust admission sees a consistent view.
    if (!(input.admissionState instanceof Uint8Array) || input.admissionState.byteLength === 0 || input.admissionState.byteLength > MAX_ADMISSION_STATE_BYTES)
      throw new TypeError('Invalid admission state bytes');
    if (input.expectedHead !== null) {
      sequence(input.expectedHead?.seq);
      hash(input.expectedHead.hash);
    }
    return this.ctx.storage.transactionSync(() => {
      this.#bind(input.spaceId);
      const duplicate = this.ctx.storage.sql.exec(
        'SELECT seq_key, prev, hash, bytes FROM entries WHERE author = ? AND client_id = ?',
        input.author, input.clientId,
      ).toArray();
      // Existing Rust FdbLog returns the original acceptance for the same
      // (author, client_id), including when a retry carries changed bytes.
      if (duplicate.length) return { kind: 'duplicate', event: entry(duplicate[0]) };
      const rows = this.ctx.storage.sql.exec('SELECT seq_key, hash FROM entries ORDER BY seq_key DESC LIMIT 1').toArray();
      const current = rows.length ? { seq: BigInt(rows[0].seq_key).toString(), hash: rows[0].hash } : null;
      if (current?.seq !== input.expectedHead?.seq || current?.hash !== input.expectedHead?.hash)
        return { kind: 'conflict', head: current };
      const next = current === null ? 0n : BigInt(current.seq) + 1n;
      if (next > MAX_U64 || seq !== next || prev !== (current?.hash ?? GENESIS_PREV))
        throw new TypeError('Event does not extend the current head');
      this.ctx.storage.sql.exec(
        'INSERT INTO entries(seq_key, author, client_id, prev, hash, bytes) VALUES (?, ?, ?, ?, ?, ?)',
        seqKey(input.event.seq), input.author, input.clientId, prev, chainHash, input.event.bytes,
      );
      this.ctx.storage.sql.exec(
        'INSERT INTO admission_state(singleton, bytes) VALUES (1, ?) ON CONFLICT(singleton) DO UPDATE SET bytes = excluded.bytes',
        input.admissionState,
      );
      return { kind: 'new', event: { seq: input.event.seq, prev, hash: chainHash, bytes: input.event.bytes } };
    });
  }

  snapshot(input) {
    return this.ctx.storage.transactionSync(() => {
      this.#bind(input?.spaceId);
      const rows = this.ctx.storage.sql.exec('SELECT seq_key, hash FROM entries ORDER BY seq_key DESC LIMIT 1').toArray();
      const state = this.ctx.storage.sql.exec('SELECT bytes FROM admission_state WHERE singleton = 1').toArray();
      return {
        head: rows.length ? { seq: BigInt(rows[0].seq_key).toString(), hash: rows[0].hash } : null,
        admissionState: state.length ? new Uint8Array(state[0].bytes) : null,
      };
    });
  }

  /** Inclusive cursor, ordered like LogStore::read(space, from, limit). */
  read(input) {
    text(input?.spaceId, 'space ID');
    const from = seqKey(input.from);
    if (!Number.isSafeInteger(input.limit) || input.limit < 1 || input.limit > MAX_READ_ENTRIES)
      throw new TypeError('Invalid read limit');
    return this.ctx.storage.transactionSync(() => {
      this.#bind(input.spaceId);
      const result = [];
      let bytes = 0;
      for (const row of this.ctx.storage.sql.exec(
        'SELECT seq_key, prev, hash, bytes FROM entries WHERE seq_key >= ? ORDER BY seq_key LIMIT ?',
        from, input.limit,
      )) {
        if (bytes + row.bytes.byteLength > MAX_READ_BYTES) break;
        bytes += row.bytes.byteLength;
        result.push(entry(row));
      }
      return result;
    });
  }
}

/** Only trusted service bindings can invoke RPC. No public sync/auth facade. */
export default class RelayState extends WorkerEntrypoint {
  appendAdmitted(input) {
    text(input?.spaceId, 'space ID');
    return this.env.SPACE_LOG.getByName(input.spaceId).appendAdmitted(input);
  }

  read(input) {
    text(input?.spaceId, 'space ID');
    return this.env.SPACE_LOG.getByName(input.spaceId).read(input);
  }

  snapshot(input) {
    text(input?.spaceId, 'space ID');
    return this.env.SPACE_LOG.getByName(input.spaceId).snapshot(input);
  }

  fetch() {
    return new Response('Not found', { status: 404 });
  }
}
