// Local lifecycle qualification only. Never imported by production entrypoints.
// Authentication, registration, SQL sessions and presence use production methods.
import { DirectoryState as ProductionDirectory } from "../src/directory-state.mjs";
export { RelayIngress, RelayDispatch } from "../src/directory-index.mjs";

export class DirectoryState extends ProductionDirectory {
  constructor(ctx, env) {
    // Read only, before production retirement: prove an actual killed generation
    // left its durable row. This fixture never seeds a row or grants authority.
    const existing =
      ctx.storage.sql
        .exec("SELECT name FROM sqlite_master WHERE type='table' AND name='directory_connections'")
        .toArray().length > 0;
    const prior = existing
      ? {
          connections: ctx.storage.sql
            .exec("SELECT count(*) AS n FROM directory_connections")
            .toArray()[0].n,
          ready: ctx.storage.sql
            .exec("SELECT count(*) AS n FROM directory_connections WHERE stage='ready'")
            .toArray()[0].n,
          sessions: ctx.storage.sql
            .exec("SELECT count(*) AS n FROM directory_sessions")
            .toArray()[0].n,
          attached: ctx.storage.sql.exec("SELECT count(*) AS n FROM relay_attached").toArray()[0].n,
        }
      : null;
    super(ctx, env);
    ctx.storage.sql.exec("CREATE TABLE IF NOT EXISTS lifecycle_config(nonce TEXT NOT NULL)");
    ctx.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS lifecycle_audit(id INTEGER PRIMARY KEY AUTOINCREMENT,phase TEXT NOT NULL,body TEXT NOT NULL)",
    );
    if (prior)
      ctx.storage.sql.exec(
        "INSERT INTO lifecycle_audit(phase,body) VALUES (?,?)",
        "constructor-before-retirement",
        JSON.stringify(prior),
      );
  }
  _nonce() {
    return this.ctx.storage.sql.exec("SELECT nonce FROM lifecycle_config LIMIT 1").toArray()[0]
      ?.nonce;
  }
  configure(input) {
    if (typeof input.nonce !== "string" || !/^[a-f0-9]{64}$/.test(input.nonce))
      throw new TypeError("Require native signed challenge fixture");
    this.ctx.storage.sql.exec("DELETE FROM lifecycle_config");
    this.ctx.storage.sql.exec("INSERT INTO lifecycle_config VALUES (?)", input.nonce);
  }
  #count(table, where = "1=1") {
    return this.ctx.storage.sql
      .exec(`SELECT count(*) AS n FROM ${table} WHERE ${where}`)
      .toArray()[0].n;
  }
  #snapshot() {
    return {
      connections: this.#count("directory_connections"),
      ready: this.#count("directory_connections", "stage='ready'"),
      ended: this.#count("directory_connections", "stage='ended'"),
      challenges: this.#count("directory_challenges"),
      sessions: this.#count("directory_sessions"),
      identities: this.#count("directory_identities"),
      devices: this.#count("directory_devices"),
      attached: this.#count("relay_attached"),
      online: this.#count("relay_presence_jobs", "kind='online'"),
      offline: this.#count("relay_presence_jobs", "kind='offline'"),
      realStates: this._sockets().map((socket) => socket.readyState),
    };
  }
  #record(phase) {
    this.ctx.storage.sql.exec(
      "INSERT INTO lifecycle_audit(phase,body) VALUES (?,?)",
      phase,
      JSON.stringify(this.#snapshot()),
    );
  }
  async inspect() {
    return {
      ...this.#snapshot(),
      nextAlarm: await this.ctx.storage.getAlarm(),
      audit: this.ctx.storage.sql
        .exec("SELECT phase,body FROM lifecycle_audit ORDER BY id")
        .toArray()
        .map((row) => ({ phase: row.phase, ...JSON.parse(row.body) })),
    };
  }
  faultOfflineInsert() {
    // Fail the ACTUAL production presence queue INSERT in its own SQL transaction.
    // The trigger stays installed until the next genuine runtime alarm callback.
    this.ctx.storage.sql
      .exec(`CREATE TRIGGER lifecycle_fail_offline BEFORE INSERT ON relay_presence_jobs
      WHEN NEW.kind='offline' BEGIN SELECT RAISE(ABORT,'owned offline queue fault'); END`);
  }
  webSocketClose(socket, code) {
    try {
      return super.webSocketClose(socket, code);
    } catch (error) {
      this.#record("actual-close-sql-fault");
      throw error;
    }
  }
  async alarm() {
    // This override is called by the runtime's durable alarm, never a fixture RPC.
    this.#record("actual-alarm-before");
    this.ctx.storage.sql.exec("DROP TRIGGER IF EXISTS lifecycle_fail_offline");
    try {
      await super.alarm();
    } finally {
      this.#record("actual-alarm-after");
    }
  }
}
export default {
  async fetch(request, env) {
    const stub = env.DIRECTORY.getByName("ordinary-lifecycle-owned");
    const path = new URL(request.url).pathname;
    if (path === "/socket")
      return stub.fetch(new Request("https://owned-directory.test/v1/sync", request));
    if (path === "/configure") {
      await stub.configure(await request.json());
      return Response.json({ ok: true });
    }
    if (path === "/inspect") return Response.json(await stub.inspect());
    if (path === "/fault-offline") {
      await stub.faultOfflineInsert();
      return Response.json({ ok: true });
    }
    // No synthetic Auth/session, close/error, alarm, membership, or event endpoint.
    return new Response("Not found", { status: 404 });
  },
};
