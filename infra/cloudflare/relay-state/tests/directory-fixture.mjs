// Local test bridge only. Production config never imports clocks, raw inspection,
// actor aliases, or failure injection from this module.
import { DirectoryState as ProductionDirectory } from '../src/directory-state.mjs';
const bytes = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
const base64 = value => btoa(String.fromCharCode(...new Uint8Array(value)));

export class DirectoryState extends ProductionDirectory {
  #clock = 1_700_000_000_000;
  #nonce = '0'.repeat(64);
  #singleClock = false;
  #clockReads = 0;
  constructor(ctx, env) {
    super(ctx, env);
    ctx.storage.sql.exec('CREATE TABLE IF NOT EXISTS directory_test_actors (actor TEXT PRIMARY KEY, session TEXT NOT NULL)');
  }
  _nowMs() {
    if (this.#singleClock && ++this.#clockReads > 1) throw new Error('Operation read the transaction clock twice');
    return this.#clock;
  }
  _nonce() { return this.#nonce; }
  clock(input) { this.#clock = Number(input.now_ms); }
  start(input) {
    this.#nonce = input.nonce;
    this.#clock = Number(input.now_ms ?? 1_700_000_000_000);
    return super.hello({ frame: bytes(input.hello_base64) });
  }
  finish(input) {
    const result = super.authenticate({ challengeId: input.challenge_id,
      frame: input.frame_null ? null : bytes(input.auth_base64) });
    if (input.actor) this.ctx.storage.sql.exec('INSERT INTO directory_test_actors VALUES (?,?) ON CONFLICT(actor) DO UPDATE SET session=excluded.session', input.actor, result.session_id);
    return { kind: 'session', ...result.session };
  }
  step(input) {
    if (input.op === 'login') {
      const challenge = this.start(input);
      return this.finish({ ...input, ...challenge });
    }
    if (input.op !== 'request') throw new Error('Unsupported fixture step');
    this.#clock = Number(input.now_ms ?? 1_700_000_000_000);
    const row = this.ctx.storage.sql.exec('SELECT session FROM directory_test_actors WHERE actor=?', input.actor).toArray()[0];
    if (!row) throw new Error('Fixture actor is not logged in');
    this.#singleClock = Boolean(input.enforce_single_clock);
    this.#clockReads = 0;
    let result;
    try { result = super.request({ sessionId: row.session, frame: bytes(input.frame_base64) }); }
    finally { this.#singleClock = false; }
    return { kind: 'reply', ...result };
  }
  inspect() {
    const sql = this.ctx.storage.sql;
    return {
      identities: sql.exec('SELECT profile FROM directory_identities ORDER BY id').toArray().map(row => JSON.parse(row.profile)),
      devices: sql.exec('SELECT device,identity,cert,revoked FROM directory_devices ORDER BY device').toArray()
        .map(row => ({ ...row, revoked: Boolean(row.revoked) })),
      boxes: sql.exec('SELECT id,sealed FROM directory_boxes ORDER BY id').toArray()
        .map(row => ({ id: row.id, sealed_base64: base64(row.sealed) })),
    };
  }
  sessions() {
    return this.ctx.storage.sql.exec('SELECT bytes FROM directory_sessions ORDER BY id').toArray().map(row => JSON.parse(row.bytes));
  }
  reject(input) {
    if (input.table === 'device') this.ctx.storage.sql.exec(`CREATE TRIGGER directory_test_failure BEFORE INSERT ON directory_devices
      BEGIN SELECT RAISE(ABORT, 'test-only device failure'); END`);
    else if (input.table === 'box') this.ctx.storage.sql.exec(`CREATE TRIGGER directory_test_failure BEFORE INSERT ON directory_boxes
      BEGIN SELECT RAISE(ABORT, 'test-only box failure'); END`);
    else throw new Error('Unsupported failure table');
  }
  allow() { this.ctx.storage.sql.exec('DROP TRIGGER directory_test_failure'); }
  fillSessions() {
    for (let index = 0; index < 256; index++) this.ctx.storage.sql.exec(
      'INSERT INTO directory_sessions VALUES (?,?,?)', index.toString(16).padStart(64, '0'), '{}', this.#clock + 900_000);
  }
}

export default {
  async fetch(request, env) {
    const input = await request.json();
    const stub = env.DIRECTORY.getByName('private-native-fixture');
    try {
      let result;
      switch (new URL(request.url).pathname) {
        case '/step': result = await stub.step(input); break;
        case '/inspect': result = await stub.inspect(); break;
        case '/sessions': result = await stub.sessions(); break;
        case '/start': result = await stub.start(input); break;
        case '/finish': result = await stub.finish(input); break;
        case '/clock': result = await stub.clock(input); break;
        case '/reject': result = await stub.reject(input); break;
        case '/allow': result = await stub.allow(); break;
        case '/fill-sessions': result = await stub.fillSessions(); break;
        case '/raw-enroll': result = await stub.enroll(input); break;
        default: return new Response('Not found', { status: 404 });
      }
      return Response.json({ ok: true, result });
    } catch (failure) {
      let detail;
      try { detail = JSON.parse(failure.message ?? String(failure)); }
      catch { detail = { reason: failure.message ?? String(failure) }; }
      return Response.json({ ok: false, error: detail }, { status: 400 });
    }
  },
};
