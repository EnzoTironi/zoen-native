// Local-only provider fixture. Production configs never import seeding, clocks,
// synthetic entropy, JWKS responses or SQL failure injection from this file.
import { DirectoryState as ProductionDirectory } from "../src/directory-state.mjs";

export class DirectoryState extends ProductionDirectory {
  #configuration;
  #clock = 1_000_000;
  #clockSequence = [];
  #cacheClock = 0;
  #entropy = [];
  #jwks = { keys: [] };
  #fetches = [];
  #response = {};
  constructor(ctx, env) {
    const configuration = { ...env };
    super(ctx, configuration);
    this.#configuration = configuration;
  }
  _nowMs() {
    return this.#clockSequence.length ? this.#clockSequence.shift() : this.#clock;
  }
  _providerCacheNow() {
    return this.#cacheClock;
  }
  _scheduleSocketAlarm() {}
  _randomToken() {
    if (!this.#entropy.length) throw new Error("Synthetic provider entropy exhausted");
    return this.#entropy.shift();
  }
  async _fetchProviderKeys(url, options) {
    this.#fetches.push({ url, redirect: options.redirect });
    if (this.#response.waitForAbort)
      return new Promise((_, reject) => {
        options.signal.addEventListener("abort", () => reject(new Error("Synthetic timeout")), {
          once: true,
        });
      });
    const headers = {
      "Cache-Control": this.#response.cacheControl ?? "max-age=300",
      ...this.#response.headers,
    };
    return new Response(this.#response.body ?? JSON.stringify(this.#jwks), {
      status: this.#response.status ?? 200,
      headers,
    });
  }
  async configure(input) {
    for (const key of Object.keys(this.#configuration))
      if (key.startsWith("ZOEN_AUTH_")) delete this.#configuration[key];
    for (const value of input.settings.audiences) {
      const prefix = `ZOEN_AUTH_${value.provider.toUpperCase()}_${value.client.toUpperCase()}`;
      this.#configuration[`${prefix}_AUDIENCE`] = value.audience.id;
      if (value.audience.presenter !== null)
        this.#configuration[`${prefix}_PRESENTER`] = value.audience.presenter;
    }
    this.#configuration.ZOEN_AUTH_WEB_ORIGINS = input.settings.origins.join(",");
    this.#clock = Number(input.now_ms ?? 1_000_000);
    this.#cacheClock = Number(input.cache_now_ms ?? 0);
    this.#entropy = [...(input.entropy ?? [])];
    this.#jwks = input.jwks;
    this.#response = input.response ?? {};
    this.#clockSequence = [];
    // Initializes only the normal production schema; this GET is not supported.
    await super.provider(new Request("https://private.fixture/initialize"));
    return { configured: true };
  }
  stepClock(input) {
    this.#clock = Number(input.verification_now_ms ?? input.now_ms);
    if (input.cache_now_ms !== undefined) this.#cacheClock = Number(input.cache_now_ms);
    // Model the explicit native policy, JWT and SQL issue timestamps even when
    // a retry reuses cached keys and therefore performs no HTTP fetch.
    this.#clockSequence =
      input.op === "issue"
        ? [
            Number(input.now_ms),
            Number(input.verification_now_ms ?? input.now_ms),
            Number(input.issue_now_ms ?? input.now_ms),
          ]
        : [];
    this.#entropy = [...input.entropy];
  }
  seed(state) {
    const sql = this.ctx.storage.sql;
    this.ctx.storage.transactionSync(() => {
      for (const table of [
        "directory_identities",
        "directory_devices",
        "provider_challenges",
        "provider_proofs",
        "provider_bindings",
      ])
        if (sql.exec(`SELECT count(*) AS count FROM ${table}`).toArray()[0].count)
          throw new Error("Provider seed requires a fresh local object");
      for (const profile of state.identities)
        sql.exec(
          "INSERT INTO directory_identities VALUES (?,?,?)",
          profile.id,
          profile.handle,
          JSON.stringify(profile),
        );
      for (const row of state.devices)
        sql.exec(
          "INSERT INTO directory_devices VALUES (?,?,?,?)",
          row.device,
          row.identity,
          row.cert,
          Number(row.revoked),
        );
      for (const row of state.challenges) {
        const { id, consumed_ms, ...challenge } = row;
        sql.exec(
          "INSERT INTO provider_challenges VALUES (?,?,?,?,?)",
          id,
          JSON.stringify(challenge),
          row.state,
          Number(row.expires_ms),
          consumed_ms === null ? null : Number(consumed_ms),
        );
      }
      for (const row of state.proofs)
        sql.exec(
          "INSERT INTO provider_proofs VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
          row.token_hash,
          row.challenge,
          row.provider,
          row.issuer,
          row.subject,
          row.origin,
          Number(row.expires_ms),
          row.consumed_ms === null ? null : Number(row.consumed_ms),
          row.bound_fingerprint,
          row.bound_identity,
          row.bound_device,
          row.bound_handle,
        );
      for (const row of state.bindings)
        sql.exec(
          "INSERT INTO provider_bindings VALUES (?,?,?,?)",
          row.provider,
          row.issuer,
          row.subject,
          row.identity,
        );
    });
    return { seeded: true };
  }
  inspect() {
    const sql = this.ctx.storage.sql;
    return {
      identities: sql
        .exec("SELECT profile FROM directory_identities ORDER BY id")
        .toArray()
        .map((row) => JSON.parse(row.profile)),
      devices: sql
        .exec("SELECT * FROM directory_devices ORDER BY device")
        .toArray()
        .map((row) => ({ ...row, revoked: Boolean(row.revoked) })),
      challenges: sql
        .exec("SELECT * FROM provider_challenges ORDER BY id")
        .toArray()
        .map((row) => ({
          id: row.id,
          ...JSON.parse(row.bytes),
          consumed_ms: row.consumed_ms === null ? null : String(row.consumed_ms),
        })),
      proofs: sql
        .exec("SELECT * FROM provider_proofs ORDER BY token_hash")
        .toArray()
        .map((row) => ({
          ...row,
          expires_ms: String(row.expires_ms),
          consumed_ms: row.consumed_ms === null ? null : String(row.consumed_ms),
        })),
      bindings: sql
        .exec("SELECT * FROM provider_bindings ORDER BY provider,issuer,subject")
        .toArray(),
    };
  }
  fetches() {
    return this.#fetches;
  }
  reject(input) {
    if (!["proof-insert", "proof-update"].includes(input.kind))
      throw new Error("Unknown local failure fixture");
    const action = input.kind === "proof-insert" ? "INSERT" : "UPDATE";
    this.ctx.storage.sql.exec(
      `CREATE TRIGGER provider_test_reject BEFORE ${action} ON provider_proofs BEGIN SELECT RAISE(ABORT,'local provider rollback fixture'); END`,
    );
  }
  allow() {
    this.ctx.storage.sql.exec("DROP TRIGGER IF EXISTS provider_test_reject");
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const object = env.DIRECTORY.getByName(url.searchParams.get("name") ?? "provider-fixture");
    // This entire module is local-only. Native Origin headers are forwarded as
    // real Request headers, not added to the provider request's JSON body.
    if (url.pathname.startsWith("/v1/auth/")) return object.provider(request);
    const input = await request.json();
    switch (url.pathname) {
      case "/fixture/configure":
        return Response.json(await object.configure(input));
      case "/fixture/clock":
        return Response.json((await object.stepClock(input)) ?? null);
      case "/fixture/seed":
        return Response.json(await object.seed(input));
      case "/fixture/inspect":
        return Response.json(await object.inspect());
      case "/fixture/fetches":
        return Response.json(await object.fetches());
      case "/fixture/reject":
        return Response.json((await object.reject(input)) ?? null);
      case "/fixture/allow":
        return Response.json((await object.allow()) ?? null);
      default:
        return new Response("Not found", { status: 404 });
    }
  },
};
