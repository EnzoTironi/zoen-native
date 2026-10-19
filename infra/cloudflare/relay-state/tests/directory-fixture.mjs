// Local test bridge only. Production config never imports clocks, raw inspection,
// actor aliases, or failure injection from this module.
import { DirectoryState as ProductionDirectory } from "../src/directory-state.mjs";
const bytes = (value) => Uint8Array.from(atob(value), (char) => char.charCodeAt(0));
const base64 = (value) => {
  const data = new Uint8Array(value);
  let text = "";
  for (let index = 0; index < data.length; index += 8192)
    text += String.fromCharCode(...data.subarray(index, index + 8192));
  return btoa(text);
};
const compare = (left, right) => (left < right ? -1 : left > right ? 1 : 0);

export class DirectoryState extends ProductionDirectory {
  #clock = 1_700_000_000_000;
  #nonce = "0".repeat(64);
  #singleClock = false;
  #clockReads = 0;
  #socketCloseTransitions = {};
  constructor(ctx, env) {
    super(ctx, env);
    ctx.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS directory_test_actors (actor TEXT PRIMARY KEY, session TEXT NOT NULL)",
    );
  }
  _nowMs() {
    if (this.#singleClock && ++this.#clockReads > 1)
      throw new Error("Operation read the transaction clock twice");
    return this.#clock;
  }
  _nonce() {
    return this.#nonce;
  }
  // Fixed native fixture clocks predate the host. Exercise alarms explicitly in
  // socket tests instead of scheduling them against the host wall clock.
  _scheduleSocketAlarm() {}
  clock(input) {
    this.#clock = Number(input.now_ms);
  }
  socketConfig(input) {
    this.#clock = Number(input.now_ms ?? 1_700_000_000_000);
    this.#nonce = input.nonce;
  }
  socketDeliver(input) {
    return super.deliver({
      identity: input.identity,
      device: input.device,
      frame: bytes(input.frame_base64),
    });
  }
  _socketClose(socket, code = 1008) {
    const before = socket.readyState;
    try {
      return super._socketClose(socket, code);
    } finally {
      const transition = `${before}->${socket.readyState}`;
      this.#socketCloseTransitions[transition] =
        (this.#socketCloseTransitions[transition] ?? 0) + 1;
    }
  }
  async socketAlarm(input) {
    this.#clock = Number(input.now_ms);
    await super.alarm();
    const serverReadyStates = {};
    for (const socket of this._sockets())
      serverReadyStates[socket.readyState] = (serverReadyStates[socket.readyState] ?? 0) + 1;
    return {
      ...this.socketState(),
      serverReadyStates,
      closeTransitions: { ...this.#socketCloseTransitions },
      scheduledAlarm: (await this.ctx.storage.getAlarm()) !== null,
    };
  }
  socketState() {
    const sql = this.ctx.storage.sql;
    return {
      connections: sql
        .exec(
          "SELECT stage,expires_ms,challenge_id IS NOT NULL AS challenge,session_id IS NOT NULL AS session FROM directory_connections ORDER BY stage,expires_ms",
        )
        .toArray()
        .map((row) => ({
          stage: row.stage,
          expires_ms: String(row.expires_ms),
          challenge: Boolean(row.challenge),
          session: Boolean(row.session),
        })),
      challenges: sql.exec("SELECT count(*) AS count FROM directory_challenges").toArray()[0].count,
      sessions: sql.exec("SELECT count(*) AS count FROM directory_sessions").toArray()[0].count,
    };
  }
  socketAttachments() {
    // Return shape only; opaque connection/session/challenge IDs stay inside DO.
    return this._sockets().map((socket) => {
      const value = { version: 1, connection_id: this._socketId(socket) };
      return {
        keys: Object.keys(value).sort(),
        version: value.version,
        opaqueConnectionId:
          typeof value.connection_id === "string" && /^[a-f0-9]{64}$/.test(value.connection_id),
      };
    });
  }
  start(input) {
    this.#nonce = input.nonce;
    this.#clock = Number(input.now_ms ?? 1_700_000_000_000);
    return super.hello({ frame: bytes(input.hello_base64) });
  }
  finish(input) {
    const result = super.authenticate({
      challengeId: input.challenge_id,
      frame: input.frame_null ? null : bytes(input.auth_base64),
    });
    if (input.actor)
      this.ctx.storage.sql.exec(
        "INSERT INTO directory_test_actors VALUES (?,?) ON CONFLICT(actor) DO UPDATE SET session=excluded.session",
        input.actor,
        result.session_id,
      );
    return { kind: "session", ...result.session };
  }
  step(input) {
    if (input.op === "login") {
      const challenge = this.start(input);
      return this.finish({ ...input, ...challenge });
    }
    if (input.op !== "request") throw new Error("Unsupported fixture step");
    this.#clock = Number(input.now_ms ?? 1_700_000_000_000);
    const row = this.ctx.storage.sql
      .exec("SELECT session FROM directory_test_actors WHERE actor=?", input.actor)
      .toArray()[0];
    if (!row) throw new Error("Fixture actor is not logged in");
    this.#singleClock = Boolean(input.enforce_single_clock);
    this.#clockReads = 0;
    let result;
    try {
      result = super.request({ sessionId: row.session, frame: bytes(input.frame_base64) });
    } finally {
      this.#singleClock = false;
    }
    return { kind: "reply", ...result };
  }
  inspect() {
    const sql = this.ctx.storage.sql;
    return {
      identities: sql
        .exec("SELECT profile FROM directory_identities ORDER BY id")
        .toArray()
        .map((row) => JSON.parse(row.profile)),
      devices: sql
        .exec("SELECT device,identity,cert,revoked FROM directory_devices ORDER BY device")
        .toArray()
        .map((row) => ({ ...row, revoked: Boolean(row.revoked) })),
      boxes: sql
        .exec("SELECT id,sealed FROM directory_boxes ORDER BY id")
        .toArray()
        .map((row) => ({ id: row.id, sealed_base64: base64(row.sealed) })),
      packages: sql
        .exec("SELECT identity,device,data,last_resort FROM directory_key_packages")
        .toArray()
        .map((row) => ({
          identity: row.identity,
          device: row.device,
          data_base64: base64(row.data),
          last_resort: Boolean(row.last_resort),
        }))
        .sort((a, b) =>
          compare(
            [a.identity, a.device, Number(a.last_resort), a.data_base64].join("\0"),
            [b.identity, b.device, Number(b.last_resort), b.data_base64].join("\0"),
          ),
        ),
      publications: sql
        .exec(
          "SELECT identity,device,package_hash,CAST(expires_secs AS TEXT) AS expires_secs FROM directory_publications ORDER BY identity,device,package_hash",
        )
        .toArray(),
      claims: sql
        .exec(
          "SELECT source_identity,source_device,operation_id,targets,response,CAST(expires_ms AS TEXT) AS expires_ms FROM directory_claims ORDER BY source_identity,source_device,operation_id",
        )
        .toArray()
        .map((row) => ({
          source_identity: row.source_identity,
          source_device: row.source_device,
          operation_id: row.operation_id,
          targets: JSON.parse(row.targets),
          response_base64: base64(row.response),
          expires_ms: row.expires_ms,
        })),
    };
  }
  sessions() {
    return this.ctx.storage.sql
      .exec("SELECT bytes FROM directory_sessions ORDER BY id")
      .toArray()
      .map((row) => JSON.parse(row.bytes));
  }
  encryptedState() {
    const sql = this.ctx.storage.sql;
    return {
      agreements: sql
        .exec(
          "SELECT identity,public,device,sig,cert FROM directory_agreement_keys ORDER BY identity",
        )
        .toArray()
        .map((row) => ({
          identity: row.identity,
          public: row.public,
          signed: { device: row.device, sig: row.sig, cert: row.cert },
        })),
      profiles: sql
        .exec(
          "SELECT identity,version,ciphertext,device,sig,cert FROM directory_sealed_profiles ORDER BY identity",
        )
        .toArray()
        .map((row) => ({
          identity: row.identity,
          version: row.version,
          ciphertext_base64: base64(row.ciphertext),
          signed: { device: row.device, sig: row.sig, cert: row.cert },
        })),
    };
  }
  readCapacity(input) {
    // Trusted-storage capacity faults only. Accounts below were registered by
    // normal fixture requests; fabricated inventory is not device-enrollment proof.
    const sql = this.ctx.storage.sql;
    if (input.kind === "profiles") {
      if (
        !Number.isInteger(input.bytes) ||
        input.bytes < 0 ||
        input.bytes > 900 * 1024 ||
        !Array.isArray(input.identities) ||
        input.identities.length > 2
      )
        throw new Error("Invalid read capacity fixture");
      this.ctx.storage.transactionSync(() => {
        for (const identity of input.identities) {
          const row = sql
            .exec("SELECT profile FROM directory_identities WHERE id=?", identity)
            .toArray()[0];
          if (!row) throw new Error("Real registered fixture profile required");
          const profile = JSON.parse(row.profile);
          if (profile.id !== identity || profile.kind !== "Person")
            throw new Error("Valid registered Person required");
          profile.glyph = input.bytes ? "x".repeat(input.bytes) : null;
          sql.exec(
            "UPDATE directory_identities SET profile=? WHERE id=?",
            JSON.stringify(profile),
            identity,
          );
        }
      });
    } else if (input.kind === "devices") {
      if (!Number.isInteger(input.count) || input.count < 0 || input.count > 7000)
        throw new Error("Invalid device read capacity fixture");
      if (
        !sql.exec("SELECT id FROM directory_identities WHERE id=?", input.identity).toArray().length
      )
        throw new Error("Real registered fixture identity required");
      this.ctx.storage.transactionSync(() => {
        for (let index = 0; index < input.count; index++)
          sql.exec(
            "INSERT INTO directory_devices VALUES (?,?,?,0)",
            "c".repeat(56) + index.toString(16).padStart(8, "0"),
            input.identity,
            "test-only-read-capacity",
          );
      });
    } else throw new Error("Unsupported read capacity fixture");
    return {
      identities: sql.exec("SELECT count(*) AS count FROM directory_identities").toArray()[0].count,
      devices: sql.exec("SELECT count(*) AS count FROM directory_devices").toArray()[0].count,
    };
  }
  reject(input) {
    if (input.table === "device")
      this.ctx.storage.sql
        .exec(`CREATE TRIGGER directory_test_failure BEFORE INSERT ON directory_devices
      BEGIN SELECT RAISE(ABORT, 'test-only device failure'); END`);
    else if (input.table === "box")
      this.ctx.storage.sql
        .exec(`CREATE TRIGGER directory_test_failure BEFORE INSERT ON directory_boxes
      BEGIN SELECT RAISE(ABORT, 'test-only box failure'); END`);
    else if (input.table === "publication")
      this.ctx.storage.sql
        .exec(`CREATE TRIGGER directory_test_failure BEFORE INSERT ON directory_key_packages
      BEGIN SELECT RAISE(ABORT, 'test-only publication failure'); END`);
    else if (input.table === "receipt")
      this.ctx.storage.sql
        .exec(`CREATE TRIGGER directory_test_failure BEFORE INSERT ON directory_claims
      BEGIN SELECT RAISE(ABORT, 'test-only receipt failure'); END`);
    else if (input.table === "unlink")
      this.ctx.storage.sql
        .exec(`CREATE TRIGGER directory_test_failure BEFORE DELETE ON directory_key_packages
      BEGIN SELECT RAISE(ABORT, 'test-only unlink failure'); END`);
    else if (input.table === "profile" || input.table === "agreement") {
      const table =
        input.table === "profile" ? "directory_sealed_profiles" : "directory_agreement_keys";
      this.ctx.storage.sql.exec(`CREATE TRIGGER directory_test_failure BEFORE INSERT ON ${table}
        BEGIN SELECT RAISE(ABORT, 'test-only encrypted directory failure'); END`);
    } else if (input.table === "socket-auth" || input.table === "socket-ready") {
      const stage = input.table === "socket-auth" ? "auth" : "ready";
      this.ctx.storage.sql
        .exec(`CREATE TRIGGER directory_test_failure BEFORE UPDATE ON directory_connections
        WHEN NEW.stage='${stage}' BEGIN SELECT RAISE(ABORT, 'test-only socket stage failure'); END`);
    } else throw new Error("Unsupported failure table");
  }
  allow() {
    this.ctx.storage.sql.exec("DROP TRIGGER directory_test_failure");
  }
  // Trusted-storage stress setup only: exercises bounded encoding/selection,
  // not MLS publication or enrolling real devices with fabricated certificates.
  capacityInventory(input) {
    if (!Number.isInteger(input.count) || input.count < 0 || input.count > 129)
      throw new Error("Invalid capacity fixture count");
    const sql = this.ctx.storage.sql;
    this.ctx.storage.transactionSync(() => {
      sql.exec(
        "DELETE FROM directory_key_packages WHERE identity=? AND device IN (SELECT device FROM directory_devices WHERE cert='test-only-capacity')",
        input.identity,
      );
      sql.exec(
        "DELETE FROM directory_devices WHERE identity=? AND cert='test-only-capacity'",
        input.identity,
      );
      for (let index = 0; index < input.count; index++) {
        const device = "f".repeat(56) + index.toString(16).padStart(8, "0");
        sql.exec(
          "INSERT INTO directory_devices VALUES (?,?,?,0)",
          device,
          input.identity,
          "test-only-capacity",
        );
        sql.exec(
          "INSERT INTO directory_key_packages(identity,device,data,last_resort) VALUES (?,?,?,0)",
          input.identity,
          device,
          new Uint8Array(4096).fill(7),
        );
      }
    });
  }
  latePackage(input) {
    this.ctx.storage.sql.exec(
      "INSERT INTO directory_key_packages(identity,device,data,last_resort) VALUES (?,?,?,0)",
      input.identity,
      input.device,
      bytes(input.data_base64),
    );
  }
  fillReceipts(input) {
    if (!Number.isInteger(input.count) || input.count < 0 || input.count > 4096)
      throw new Error("Invalid receipt fixture count");
    const sql = this.ctx.storage.sql;
    const row = sql
      .exec(
        "SELECT * FROM directory_claims WHERE source_identity=? AND source_device=? ORDER BY operation_id LIMIT 1",
        input.source_identity,
        input.source_device,
      )
      .toArray()[0];
    if (!row) throw new Error("Existing real canonical receipt required");
    this.ctx.storage.transactionSync(() => {
      for (let index = 0; index < input.count; index++)
        sql.exec(
          "INSERT INTO directory_claims VALUES (?,?,?,?,?,?)",
          row.source_identity,
          row.source_device,
          row.operation_id.slice(0, 10) +
            (index + 1000).toString(16).toUpperCase().padStart(16, "0"),
          row.targets,
          row.response,
          row.expires_ms,
        );
    });
  }
  fillSessions() {
    for (let index = 0; index < 256; index++)
      this.ctx.storage.sql.exec(
        "INSERT INTO directory_sessions VALUES (?,?,?)",
        index.toString(16).padStart(64, "0"),
        "{}",
        this.#clock + 900_000,
      );
  }
}

export default {
  async fetch(request, env) {
    const stub = env.DIRECTORY.getByName("private-native-fixture");
    const pathname = new URL(request.url).pathname;
    if (pathname === "/socket")
      return stub.fetch(new Request("https://directory-private.test/v1/sync", request));
    const input = await request.json();
    try {
      let result;
      switch (pathname) {
        case "/step":
          result = await stub.step(input);
          break;
        case "/inspect":
          result = await stub.inspect();
          break;
        case "/sessions":
          result = await stub.sessions();
          break;
        case "/encrypted-state":
          result = await stub.encryptedState();
          break;
        case "/read-capacity":
          result = await stub.readCapacity(input);
          break;
        case "/start":
          result = await stub.start(input);
          break;
        case "/finish":
          result = await stub.finish(input);
          break;
        case "/clock":
          result = await stub.clock(input);
          break;
        case "/reject":
          result = await stub.reject(input);
          break;
        case "/allow":
          result = await stub.allow();
          break;
        case "/capacity-inventory":
          result = await stub.capacityInventory(input);
          break;
        case "/late-package":
          result = await stub.latePackage(input);
          break;
        case "/fill-receipts":
          result = await stub.fillReceipts(input);
          break;
        case "/fill-sessions":
          result = await stub.fillSessions();
          break;
        case "/socket-config":
          result = await stub.socketConfig(input);
          break;
        case "/socket-deliver":
          result = await stub.socketDeliver(input);
          break;
        case "/socket-alarm":
          result = await stub.socketAlarm(input);
          break;
        case "/socket-state":
          result = await stub.socketState();
          break;
        case "/socket-attachments":
          result = await stub.socketAttachments();
          break;
        case "/raw-enroll":
          result = await stub.enroll(input);
          break;
        default:
          return new Response("Not found", { status: 404 });
      }
      return Response.json({ ok: true, result });
    } catch (failure) {
      let detail;
      try {
        detail = JSON.parse(failure.message ?? String(failure));
      } catch {
        detail = { reason: failure.message ?? String(failure) };
      }
      return Response.json({ ok: false, error: detail }, { status: 400 });
    }
  },
};
