// Bounded SQL persistence only. Callers own the transaction and all admission policy.
// This private store retains its original Space <=128, client ID <=256 and lowercase
// author/hash bounds. Those are storage-contract limits, not full native LogStore equivalence.
const MAX_U64 = 18446744073709551615n;
const GENESIS_PREV = "0".repeat(64);
const MAX_ENTRY_BYTES = 1024 * 1024;
const MAX_ADMISSION_STATE_BYTES = 1024 * 1024;
const MAX_READ_BYTES = 4 * 1024 * 1024;
const MAX_READ_ENTRIES = 256;

function text(value, name, max = 128) {
  if (typeof value !== "string" || value.length === 0 || value.length > max)
    throw new TypeError(`Invalid ${name}`);
  return value;
}

export function spaceId(value) {
  return text(value, "space ID");
}

function sequence(value) {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]{0,19})$/.test(value))
    throw new TypeError("Invalid sequence");
  const parsed = BigInt(value);
  if (parsed > MAX_U64) throw new TypeError("Sequence exceeds UInt64");
  return parsed;
}

function hash(value) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value))
    throw new TypeError("Invalid hash");
  return value;
}

function seqKey(value) {
  sequence(value);
  return value.padStart(20, "0");
}

function entry(row) {
  return {
    seq: BigInt(row.seq_key).toString(),
    prev: row.prev,
    hash: row.hash,
    bytes: new Uint8Array(row.bytes),
  };
}

/** No method starts or nests a transaction. The Durable Object owns that boundary. */
export class SpaceStorage {
  #sql;

  constructor(sql) {
    this.#sql = sql;
    sql.exec(`
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

  #scope(value, create) {
    spaceId(value);
    const rows = this.#sql.exec("SELECT space_id FROM space_binding WHERE singleton = 1").toArray();
    if (rows.length && rows[0].space_id !== value)
      throw new TypeError("Space does not match this storage object");
    if (create && !rows.length)
      this.#sql.exec("INSERT INTO space_binding(singleton, space_id) VALUES (1, ?)", value);
  }

  bind(value) {
    this.#scope(value, true);
  }

  receipt(author, clientId) {
    const rows = this.#sql
      .exec(
        "SELECT seq_key, prev, hash, bytes FROM entries WHERE author = ? AND client_id = ?",
        author,
        clientId,
      )
      .toArray();
    return rows.length ? entry(rows[0]) : null;
  }

  /** Canonical reference resolution for a bounded trusted publication outbox. */
  eventAt(space, seq) {
    this.#scope(space, false);
    const rows = this.#sql
      .exec("SELECT seq_key,prev,hash,bytes FROM entries WHERE seq_key=?", seqKey(seq))
      .toArray();
    return rows.length ? entry(rows[0]) : null;
  }

  hashes(seqs) {
    const result = [];
    for (const seq of new Set(seqs)) {
      const rows = this.#sql
        .exec("SELECT seq_key, hash FROM entries WHERE seq_key = ?", seqKey(seq))
        .toArray();
      if (rows.length) result.push({ seq: BigInt(rows[0].seq_key).toString(), hash: rows[0].hash });
    }
    return result;
  }

  snapshot(value, { bind = true } = {}) {
    this.#scope(value, bind);
    const rows = this.#sql
      .exec("SELECT seq_key, hash FROM entries ORDER BY seq_key DESC LIMIT 1")
      .toArray();
    const state = this.#sql.exec("SELECT bytes FROM admission_state WHERE singleton = 1").toArray();
    return {
      head: rows.length ? { seq: BigInt(rows[0].seq_key).toString(), hash: rows[0].hash } : null,
      admissionState: state.length ? new Uint8Array(state[0].bytes) : null,
    };
  }

  /** Same storage-only contract as the original SpaceLog; dedupe precedes CAS. */
  appendAdmitted(input) {
    spaceId(input?.spaceId);
    text(input.author, "author", 64);
    if (!/^[0-9a-f]{64}$/.test(input.author)) throw new TypeError("Invalid author");
    text(input.clientId, "client ID", 256);
    const seq = sequence(input.event?.seq);
    const prev = hash(input.event.prev);
    const chainHash = hash(input.event.hash);
    if (
      !(input.event.bytes instanceof Uint8Array) ||
      input.event.bytes.byteLength === 0 ||
      input.event.bytes.byteLength > MAX_ENTRY_BYTES
    )
      throw new TypeError("Invalid canonical event bytes");
    if (
      !(input.admissionState instanceof Uint8Array) ||
      input.admissionState.byteLength === 0 ||
      input.admissionState.byteLength > MAX_ADMISSION_STATE_BYTES
    )
      throw new TypeError("Invalid admission state bytes");
    if (input.expectedHead !== null) {
      sequence(input.expectedHead?.seq);
      hash(input.expectedHead.hash);
    }
    this.bind(input.spaceId);
    const duplicate = this.receipt(input.author, input.clientId);
    if (duplicate) return { kind: "duplicate", event: duplicate };
    const current = this.snapshot(input.spaceId).head;
    if (current?.seq !== input.expectedHead?.seq || current?.hash !== input.expectedHead?.hash)
      return { kind: "conflict", head: current };
    const next = current === null ? 0n : BigInt(current.seq) + 1n;
    if (next > MAX_U64 || seq !== next || prev !== (current?.hash ?? GENESIS_PREV))
      throw new TypeError("Event does not extend the current head");
    this.#sql.exec(
      "INSERT INTO entries(seq_key, author, client_id, prev, hash, bytes) VALUES (?, ?, ?, ?, ?, ?)",
      seqKey(input.event.seq),
      input.author,
      input.clientId,
      prev,
      chainHash,
      input.event.bytes,
    );
    this.#sql.exec(
      "INSERT INTO admission_state(singleton, bytes) VALUES (1, ?) ON CONFLICT(singleton) DO UPDATE SET bytes = excluded.bytes",
      input.admissionState,
    );
    return {
      kind: "new",
      event: { seq: input.event.seq, prev, hash: chainHash, bytes: input.event.bytes },
    };
  }

  /** Caller owns the transaction; Rust supplies the admitted range and stubs. */
  pruneAdmitted(range, transform) {
    const from = sequence(range.from);
    const floor = sequence(range.to_exclusive);
    if (floor < from) throw new TypeError("Invalid admitted pruning range");
    let count = 0;
    let last = null;
    const keys = this.#sql
      .exec(
        "SELECT seq_key FROM entries WHERE seq_key >= ? AND seq_key < ? ORDER BY seq_key LIMIT 256",
        seqKey(range.from),
        seqKey(range.to_exclusive),
      )
      .toArray();
    for (const key of keys) {
      const row = this.#sql
        .exec("SELECT seq_key,bytes FROM entries WHERE seq_key=?", key.seq_key)
        .toArray()[0];
      if (!row) throw new TypeError("Admitted pruning row disappeared");
      const bytes = transform(new Uint8Array(row.bytes));
      if (!(bytes instanceof Uint8Array) || bytes.byteLength > MAX_ENTRY_BYTES)
        throw new TypeError("Invalid canonical pruning bytes");
      if (bytes.byteLength)
        this.#sql.exec("UPDATE entries SET bytes=? WHERE seq_key=?", bytes, row.seq_key);
      last = BigInt(row.seq_key).toString();
      count++;
    }
    return { count, last };
  }

  replaceAdmittedState(bytes) {
    if (
      !(bytes instanceof Uint8Array) ||
      bytes.byteLength === 0 ||
      bytes.byteLength > MAX_ADMISSION_STATE_BYTES
    )
      throw new TypeError("Invalid admission state bytes");
    this.#sql.exec("UPDATE admission_state SET bytes=? WHERE singleton=1", bytes);
  }

  /** Caller owns the transaction and obtains `through` from the Rust projection.
   * This is persistence only: no client may call it with a membership assertion.
   * A key-only lookahead distinguishes byte-limited pages from the actual EOF.
   */
  readAuthorizedPage(input) {
    this.#scope(input?.spaceId, false);
    const from = sequence(input.from);
    if (!Number.isSafeInteger(input.limit) || input.limit < 1 || input.limit > MAX_READ_ENTRIES)
      throw new TypeError("Invalid authorized read limit");
    if (input.through === null) return { events: [], has_more: false, done: true, next_seq: null };
    const through = sequence(input.through);
    if (from > through) return { events: [], has_more: false, done: true, next_seq: null };
    const events = [];
    let totalBytes = 0;
    let next = from;
    for (const row of this.#sql.exec(
      "SELECT seq_key,prev,hash,bytes FROM entries WHERE seq_key>=? AND seq_key<=? ORDER BY seq_key LIMIT ?",
      seqKey(input.from),
      seqKey(input.through),
      input.limit,
    )) {
      if (totalBytes + row.bytes.byteLength > MAX_READ_BYTES) break;
      totalBytes += row.bytes.byteLength;
      events.push(entry(row));
      next = BigInt(row.seq_key) + 1n;
    }
    // UInt64 exhaustion is terminal. Never coerce it through Number or expose
    // 2^64 as a protocol cursor. Stored gaps still use inclusive `next` semantics.
    const more =
      next <= through &&
      next <= MAX_U64 &&
      this.#sql
        .exec(
          "SELECT seq_key FROM entries WHERE seq_key>=? AND seq_key<=? ORDER BY seq_key LIMIT 1",
          seqKey(next.toString()),
          seqKey(input.through),
        )
        .toArray().length !== 0;
    if (more && events.length === 0)
      throw new TypeError("An authorized read cannot advance within its byte bound");
    return {
      events,
      has_more: Boolean(more),
      done: !more,
      next_seq: more ? next.toString() : null,
    };
  }

  /** Inclusive cursor, UInt64 ordering and bounded bytes match the original LogStore. */
  read(input, { bind = true } = {}) {
    spaceId(input?.spaceId);
    const from = seqKey(input.from);
    if (!Number.isSafeInteger(input.limit) || input.limit < 1 || input.limit > MAX_READ_ENTRIES)
      throw new TypeError("Invalid read limit");
    this.#scope(input.spaceId, bind);
    const result = [];
    let bytes = 0;
    for (const row of this.#sql.exec(
      "SELECT seq_key, prev, hash, bytes FROM entries WHERE seq_key >= ? ORDER BY seq_key LIMIT ?",
      from,
      input.limit,
    )) {
      if (bytes + row.bytes.byteLength > MAX_READ_BYTES) break;
      bytes += row.bytes.byteLength;
      result.push(entry(row));
    }
    return result;
  }
}
