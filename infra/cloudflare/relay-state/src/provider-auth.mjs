// Private HTTP/service-binding host for the shared Rust provider policy. This
// module owns I/O and SQLite transactions, never JWT or enrollment decisions.
import {
  providerChallengePolicy,
  providerVerifyPolicy,
  providerBindPolicy,
  providerBindDecision,
  providerProofHash,
  providerKeyId,
  providerKeySet,
  verifyProviderAssertion,
} from "../generated/wasm/admission.js";

const MAX_BODY = 32_768;
const MAX_KEYS = 65_536;
const CHALLENGE_MS = 300_000;
const PROOF_MS = 300_000;
const CLEANUP_DELAY_MS = 600_000;
const CLEANUP_BATCH = 500;
const KEY_URLS = Object.freeze({
  apple: "https://appleid.apple.com/auth/keys",
  google: "https://www.googleapis.com/oauth2/v3/certs",
});
const encoder = new TextEncoder();
const failure = (status, code) => Object.assign(new Error(code), { status, code });
const unavailable = () => failure(503, "authentication_unavailable");
function policy(fn, ...args) {
  try {
    return JSON.parse(fn(...args));
  } catch (cause) {
    let value;
    try {
      value = JSON.parse(typeof cause === "string" ? cause : cause.message);
    } catch {}
    if (Number.isInteger(value?.status) && typeof value.code === "string")
      throw failure(value.status, value.code);
    throw unavailable();
  }
}
function safeClock(value) {
  if (!Number.isSafeInteger(value) || value < 0 || value > Number.MAX_SAFE_INTEGER - PROOF_MS)
    throw unavailable();
  return value;
}
// Native str::trim/char::is_whitespace use the Unicode White_Space property.
const trim = (value) => value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
function cacheMaxAge(header) {
  for (const part of header?.split(",") ?? []) {
    const directive = trim(part);
    if (!directive.startsWith("max-age=")) continue;
    const literal = directive.slice(8);
    if (!/^\+?\d+$/.test(literal)) continue;
    const digits = literal.replace(/^\+/, "").replace(/^0+/, "") || "0";
    if (digits.length > 20) continue;
    const value = BigInt(digits);
    if (value > 18_446_744_073_709_551_615n) continue;
    return Number(value < 60n ? 60n : value > 3600n ? 3600n : value);
  }
  return 300;
}
function settingsFromEnv(env) {
  const audiences = [];
  for (const [provider, prefix] of [
    ["apple", "APPLE"],
    ["google", "GOOGLE"],
  ]) {
    for (const [client, suffix] of [
      ["ios", "IOS"],
      ["android", "ANDROID"],
      ["web", "WEB"],
    ]) {
      if (provider === "apple" && client === "android") continue;
      const name = `ZOEN_AUTH_${prefix}_${suffix}_AUDIENCE`;
      if (env[name] === undefined) continue;
      const validate = (input) => {
        if (typeof input !== "string") throw unavailable();
        const value = trim(input);
        const bytes = encoder.encode(value);
        if (
          !value ||
          bytes.byteLength > 512 ||
          /\p{White_Space}/u.test(value) ||
          new TextDecoder().decode(bytes) !== value
        )
          throw unavailable();
        return value;
      };
      const presenter = env[`ZOEN_AUTH_${prefix}_${suffix}_PRESENTER`];
      if (client === "android" && presenter === undefined) throw unavailable();
      audiences.push({
        provider,
        client,
        audience: {
          id: validate(env[name]),
          presenter: presenter === undefined ? null : validate(presenter),
        },
      });
    }
  }
  const rawOrigins = env.ZOEN_AUTH_WEB_ORIGINS ?? "";
  if (typeof rawOrigins !== "string") throw unavailable();
  const origins = rawOrigins.split(",").map(trim).filter(Boolean);
  for (const origin of origins) {
    let url;
    try {
      url = new URL(origin);
    } catch {
      throw unavailable();
    }
    if (
      !["https:", "http:"].includes(url.protocol) ||
      !url.hostname ||
      url.origin !== origin ||
      (url.protocol === "http:" && !["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))
    )
      throw unavailable();
  }
  return { audiences, origins };
}
async function boundedBody(response, maximum, signal) {
  const size = response.headers.get("Content-Length");
  if (size !== null && /^\d+$/.test(size) && Number(size) > maximum) {
    await response.body?.cancel();
    throw unavailable();
  }
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks = [];
  let length = 0;
  const cancel = () => {
    reader.cancel().catch(() => {});
  };
  signal?.addEventListener("abort", cancel, { once: true });
  try {
    if (signal?.aborted) throw unavailable();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > maximum) {
        await reader.cancel();
        throw unavailable();
      }
      chunks.push(value);
    }
    if (signal?.aborted) throw unavailable();
  } finally {
    signal?.removeEventListener("abort", cancel);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

export class ProviderAuthState {
  #ctx;
  #env;
  #hooks;
  #caches = new Map();
  constructor(ctx, env, hooks) {
    this.#ctx = ctx;
    this.#env = env;
    this.#hooks = hooks;
    const sql = ctx.storage.sql;
    sql.exec(`CREATE TABLE IF NOT EXISTS provider_challenges (
      id TEXT PRIMARY KEY, bytes TEXT NOT NULL, state TEXT NOT NULL,
      expires_ms INTEGER NOT NULL, consumed_ms INTEGER)`);
    sql.exec(
      "CREATE INDEX IF NOT EXISTS provider_challenges_expiry ON provider_challenges(expires_ms)",
    );
    sql.exec(`CREATE TABLE IF NOT EXISTS provider_proofs (
      token_hash TEXT PRIMARY KEY, challenge TEXT NOT NULL,
      provider TEXT NOT NULL, issuer TEXT NOT NULL, subject TEXT NOT NULL,
      origin TEXT, expires_ms INTEGER NOT NULL, consumed_ms INTEGER,
      bound_fingerprint TEXT, bound_identity TEXT, bound_device TEXT, bound_handle TEXT)`);
    sql.exec("CREATE INDEX IF NOT EXISTS provider_proofs_challenge ON provider_proofs(challenge)");
    sql.exec(`CREATE TABLE IF NOT EXISTS provider_bindings (
      provider TEXT NOT NULL, issuer TEXT NOT NULL, subject TEXT NOT NULL,
      identity TEXT NOT NULL REFERENCES directory_identities(id),
      PRIMARY KEY(provider,issuer,subject), UNIQUE(identity,provider))`);
  }
  #one(query, ...args) {
    return this.#ctx.storage.sql.exec(query, ...args).toArray()[0] ?? null;
  }
  #now() {
    return safeClock(this.#hooks.nowMs());
  }
  #settings() {
    return settingsFromEnv(this.#env);
  }
  #challenge(id) {
    const row = this.#one(
      "SELECT bytes FROM provider_challenges WHERE id=? AND consumed_ms IS NULL",
      id,
    );
    return row ? JSON.parse(row.bytes) : null;
  }
  #json(body, status, origin) {
    const headers = {
      "Content-Type": "application/json",
      "Cache-Control": "no-store",
      Pragma: "no-cache",
      "X-Content-Type-Options": "nosniff",
    };
    if (origin && this.#settings().origins.includes(origin)) {
      headers["Access-Control-Allow-Origin"] = origin;
      headers.Vary = "Origin";
    }
    return Response.json(body, { status, headers });
  }
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (
      !["/v1/auth/challenge", "/v1/auth/verify", "/v1/auth/bind"].includes(path) ||
      request.method !== "POST"
    )
      return new Response("Not found", { status: 404 });
    // The facade owns Origin. No origin, subject, or Verified object in the body
    // becomes a trusted fact, including when called through a private RPC.
    const origin = request.headers.get("Origin");
    const operation = new AbortController();
    const deadline = setTimeout(() => operation.abort(), 10_000);
    try {
      let text;
      try {
        text = new TextDecoder("utf-8", { fatal: true }).decode(
          await boundedBody(request, MAX_BODY, operation.signal),
        );
      } catch {
        throw operation.signal.aborted ? unavailable() : failure(400, "invalid_authentication");
      }
      // Preserve duplicate fields and the exact DTO input for Rust's parser.
      const body = path.endsWith("/challenge")
        ? this.#issueChallenge(text, origin)
        : path.endsWith("/verify")
          ? await this.#verify(text, origin, operation.signal)
          : this.#bind(text, origin);
      return this.#json(body, 200, origin);
    } catch (cause) {
      const status = Number.isInteger(cause?.status) ? cause.status : 503;
      const code = typeof cause?.code === "string" ? cause.code : "authentication_unavailable";
      // Never include a raw exception, assertion, proof, subject, or JWKS body.
      try {
        return this.#json({ error: code }, status, origin);
      } catch {
        return Response.json(
          { error: "authentication_unavailable" },
          {
            status: 503,
            headers: {
              "Cache-Control": "no-store",
              Pragma: "no-cache",
              "X-Content-Type-Options": "nosniff",
            },
          },
        );
      }
    } finally {
      clearTimeout(deadline);
    }
  }
  #issueChallenge(request, suppliedOrigin) {
    const admitted = policy(
      providerChallengePolicy,
      request,
      JSON.stringify({ settings: this.#settings(), origin: suppliedOrigin }),
    );
    const input = JSON.parse(request);
    const id = this.#hooks.randomToken(),
      nonce = this.#hooks.randomToken(),
      state = this.#hooks.randomToken();
    const now = this.#now();
    const expires = now + CHALLENGE_MS;
    const challenge = {
      provider: input.provider,
      client: input.client,
      audience: admitted.audience.id,
      presenter: admitted.audience.presenter,
      origin: admitted.origin,
      nonce,
      state,
      created_ms: String(now),
      expires_ms: String(expires),
    };
    // Native cleanup commits independently before challenge INSERT. Keep proof
    // cascade atomic without changing the existing Directory foreign-key mode.
    this.#ctx.storage.transactionSync(() => {
      this.#ctx.storage.sql.exec(
        `DELETE FROM provider_proofs WHERE challenge IN
        (SELECT id FROM provider_challenges WHERE expires_ms < ? LIMIT ${CLEANUP_BATCH})`,
        now - CLEANUP_DELAY_MS,
      );
      this.#ctx.storage.sql.exec(
        `DELETE FROM provider_challenges WHERE id IN
        (SELECT id FROM provider_challenges WHERE expires_ms < ? LIMIT ${CLEANUP_BATCH})`,
        now - CLEANUP_DELAY_MS,
      );
    });
    this.#ctx.storage.sql.exec(
      "INSERT INTO provider_challenges VALUES (?,?,?,?,NULL)",
      id,
      JSON.stringify(challenge),
      state,
      expires,
    );
    return { id, nonce, state, audience: admitted.audience.id, expires_ms: expires };
  }
  async #key(provider, token, signal) {
    let kid;
    try {
      kid = providerKeyId(token);
    } catch {
      throw failure(401, "invalid_provider_assertion");
    }
    if (!Object.hasOwn(KEY_URLS, provider)) throw unavailable();
    let cache = this.#caches.get(provider);
    if (!cache) {
      cache = { keys: [], expires: 0, refreshed: null, tail: Promise.resolve() };
      this.#caches.set(provider, cache);
    }
    // Match the native per-provider mutex, including retry after a failed refresh.
    const run = cache.tail
      .catch(() => {})
      .then(async () => {
        if (signal.aborted) throw unavailable();
        const now = this.#hooks.cacheNow();
        const retained = cache.keys.find((key) => key.kid === kid);
        if (cache.expires <= now || !retained) {
          if (cache.refreshed !== null && now - cache.refreshed < 10_000)
            throw failure(401, "invalid_provider_assertion");
          cache.refreshed = now;
          const abort = new AbortController();
          const timer = setTimeout(() => abort.abort(), 5_000);
          const cancel = () => abort.abort();
          signal.addEventListener("abort", cancel, { once: true });
          try {
            const response = await this.#hooks.fetchKeys(KEY_URLS[provider], {
              redirect: "manual",
              signal: abort.signal,
            });
            if (!response.ok) throw unavailable();
            const bytes = await boundedBody(response, MAX_KEYS, abort.signal);
            let keys;
            try {
              keys = JSON.parse(providerKeySet(bytes));
            } catch {
              throw unavailable();
            }
            const ttl = cacheMaxAge(response.headers.get("Cache-Control"));
            cache.keys = keys;
            cache.expires = now + ttl * 1000;
          } catch {
            throw signal.aborted ? unavailable() : failure(401, "invalid_provider_assertion");
          } finally {
            signal.removeEventListener("abort", cancel);
            clearTimeout(timer);
            abort.abort();
          }
        }
        const key = cache.keys.find((key) => key.kid === kid);
        if (!key) throw failure(401, "invalid_provider_assertion");
        return key;
      });
    cache.tail = run;
    return run;
  }
  async #verify(request, suppliedOrigin, signal) {
    const settings = this.#settings();
    const policyNow = this.#now();
    // Native rejects malformed VerifyRequest before its first database lookup.
    // With a null challenge this same pure policy either rejects that prefix or
    // reaches the expected challenge_expired gate; no JS token parser is needed.
    try {
      policy(
        providerVerifyPolicy,
        request,
        JSON.stringify({
          settings,
          origin: suppliedOrigin,
          challenge: null,
          now_ms: String(policyNow),
        }),
      );
    } catch (cause) {
      if (cause.code !== "challenge_expired") throw cause;
    }
    const input = JSON.parse(request);
    const initial = policy(
      providerVerifyPolicy,
      request,
      JSON.stringify({
        settings,
        origin: suppliedOrigin,
        challenge: this.#challenge(input?.challenge_id),
        now_ms: String(policyNow),
      }),
    );
    // Native JWT exp/iat validation captures time BEFORE an awaited JWKS refresh.
    const verificationNow = this.#now();
    const key = await this.#key(initial.challenge.provider, input.id_token, signal);
    if (signal.aborted) throw unavailable();
    let verified;
    try {
      verified = JSON.parse(
        verifyProviderAssertion(
          input.id_token,
          JSON.stringify(key),
          JSON.stringify({
            provider: initial.challenge.provider,
            audience: initial.challenge.audience,
            presenter: initial.challenge.presenter,
            nonce: initial.challenge.nonce,
            created_ms: safeClock(Number(initial.challenge.created_ms)),
          }),
          String(verificationNow),
        ),
      );
    } catch {
      throw failure(401, "invalid_provider_assertion");
    }
    // Native requests generate entropy only after assertion verification succeeds.
    const proof = this.#hooks.randomToken();
    return this.#ctx.storage.transactionSync(() => {
      const issueNow = this.#now();
      // Native verification uses the immutable challenge captured before await.
      // Fresh expiry/consumption is checked by the conditional UPDATE below; a
      // challenge expiring during JWKS fetch must yield challenge_expired.
      const changed = this.#ctx.storage.sql.exec(
        `UPDATE provider_challenges SET consumed_ms=?
        WHERE id=? AND consumed_ms IS NULL AND expires_ms>? AND state=?`,
        issueNow,
        input.challenge_id,
        issueNow,
        input.state,
      ).rowsWritten;
      if (changed !== 1) throw failure(401, "challenge_expired");
      const expires = issueNow + PROOF_MS;
      let hash;
      try {
        hash = providerProofHash(proof);
      } catch {
        throw unavailable();
      }
      this.#ctx.storage.sql.exec(
        `INSERT INTO provider_proofs
        (token_hash,challenge,provider,issuer,subject,origin,expires_ms) VALUES (?,?,?,?,?,?,?)`,
        hash,
        input.challenge_id,
        initial.challenge.provider,
        verified.issuer,
        verified.subject,
        initial.origin,
        expires,
      );
      const bound = this.#one(
        `SELECT b.identity,i.handle FROM provider_bindings b
        JOIN directory_identities i ON i.id=b.identity WHERE provider=? AND issuer=? AND subject=?`,
        initial.challenge.provider,
        verified.issuer,
        verified.subject,
      );
      return {
        proof_token: proof,
        expires_ms: expires,
        account: bound
          ? { status: "bound", identity_id: bound.identity, handle: bound.handle }
          : { status: "unbound" },
        ...(verified.name === null || verified.name === undefined ? {} : { name: verified.name }),
      };
    });
  }
  #bind(request, suppliedOrigin) {
    const admitted = policy(
      providerBindPolicy,
      request,
      JSON.stringify({ settings: this.#settings(), origin: suppliedOrigin }),
    );
    let changed = false;
    const response = this.#ctx.storage.transactionSync(() => {
      const lookup = admitted.lookup;
      const row = this.#one("SELECT * FROM provider_proofs WHERE token_hash=?", lookup.token_hash);
      const proof = row && {
        token_hash: row.token_hash,
        provider: row.provider,
        issuer: row.issuer,
        subject: row.subject,
        origin: row.origin,
        expires_ms: String(row.expires_ms),
        consumed: row.consumed_ms !== null,
        bound_fingerprint: row.bound_fingerprint,
        bound_identity: row.bound_identity,
        bound_device: row.bound_device,
        bound_handle: row.bound_handle,
      };
      const binding = proof
        ? this.#one(
            "SELECT provider,issuer,subject,identity FROM provider_bindings WHERE provider=? AND issuer=? AND subject=?",
            proof.provider,
            proof.issuer,
            proof.subject,
          )
        : null;
      const slot = proof
        ? this.#one(
            "SELECT identity,provider,subject FROM provider_bindings WHERE identity=? AND provider=?",
            lookup.identity,
            proof.provider,
          )
        : null;
      const stored = this.#one(
        "SELECT profile,handle FROM directory_identities WHERE id=?",
        lookup.identity,
      );
      const device = this.#one(
        "SELECT device,identity,cert,revoked FROM directory_devices WHERE device=?",
        lookup.device,
      );
      const owner = this.#one("SELECT id FROM directory_identities WHERE handle=?", lookup.handle);
      const facts = {
        lookup,
        proof,
        provider_binding: binding,
        identity_provider: slot,
        identity: stored
          ? {
              identity: lookup.identity,
              kind: JSON.parse(stored.profile).kind,
              handle: stored.handle,
            }
          : null,
        device: device ? { ...device, revoked: Boolean(device.revoked) } : null,
        handle_owner: owner?.id ?? null,
      };
      const now = this.#now();
      const plan = policy(
        providerBindDecision,
        request,
        JSON.stringify({ facts, origin: admitted.origin, now_ms: String(now) }),
      );
      if (plan.action === "retry") return plan.response;
      if (plan.action === "register") {
        const { profile, device, certificate } = plan.registration;
        this.#ctx.storage.sql.exec(
          "INSERT INTO directory_identities VALUES (?,?,?)",
          profile.id,
          profile.handle,
          JSON.stringify(profile),
        );
        // The shared decision refuses a revoked/conflicting device and permits
        // an exact active enrollment retry without mutating its stored row.
        this.#ctx.storage.sql.exec(
          "INSERT INTO directory_devices VALUES (?,?,?,0) ON CONFLICT(device) DO NOTHING",
          device,
          profile.id,
          certificate,
        );
        changed = true;
      } else if (plan.action !== "existing") throw unavailable();
      this.#ctx.storage.sql.exec(
        "INSERT INTO provider_bindings VALUES (?,?,?,?) ON CONFLICT(provider,issuer,subject) DO NOTHING",
        proof.provider,
        proof.issuer,
        proof.subject,
        lookup.identity,
      );
      this.#ctx.storage.sql.exec(
        `UPDATE provider_proofs SET consumed_ms=?,bound_fingerprint=?,bound_identity=?,bound_device=?,bound_handle=?
        WHERE token_hash=?`,
        now,
        plan.fingerprint,
        plan.response.identity_id,
        lookup.device,
        plan.response.handle,
        plan.token_hash,
      );
      return plan.response;
    });
    if (changed) this.#hooks.authorizationChanged();
    return response;
  }
}
