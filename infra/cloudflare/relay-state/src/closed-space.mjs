import { DurableObject } from "cloudflare:workers";
import {
  initSync,
  verifyPublish,
  closedPublish,
  closedReplay,
  closedInvite,
  closedPruneEntry,
  closedPrunePlan,
  closedPruneState,
  closedVisibility,
  closedCoMembers,
  closedReadPlan,
  relayPublicationMetadata,
  closedRelayPreview,
} from "../generated/wasm/admission.js";
import wasmModule from "../generated/wasm/admission_bg.wasm";
import { SpaceStorage } from "./space-storage.mjs";
import { RelayPublications } from "./relay-publications.mjs";

initSync({ module: wasmModule });

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const MAX_BYTES = 1024 * 1024;

function internal(reason) {
  return new Error(JSON.stringify({ code: "invalid_storage_context", reason, permanent: false }));
}

function encode(value) {
  const bytes = encoder.encode(JSON.stringify(value));
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_BYTES)
    throw internal("Stored projection or invite exceeds its byte bound");
  return bytes;
}

function decode(bytes) {
  return bytes === null ? null : JSON.parse(decoder.decode(bytes));
}

function rust(operation, ...args) {
  try {
    return operation(...args);
  } catch (error) {
    // Preserve the Rust error code/permanence across the Durable Object RPC boundary.
    if (typeof error === "string") throw new Error(error);
    throw error;
  }
}

function invoke(operation, ...args) {
  return JSON.parse(rust(operation, ...args));
}

export function verifiedPublication(input) {
  if (
    !(input?.frame instanceof Uint8Array) ||
    input.frame.byteLength === 0 ||
    input.frame.byteLength > MAX_BYTES
  )
    throw new TypeError("Invalid publication frame bytes");
  const session = JSON.stringify(input.session);
  return { session, verified: invoke(verifyPublish, input.frame, session) };
}

// Only the local test module imports this function; no production WorkerEntrypoint
// or Durable Object RPC method exposes it. It cannot bypass composed-Space mode.
const fixturePublications = new WeakMap();
export function fixturePublication(object, input) {
  const publish = fixturePublications.get(object);
  if (!publish) throw internal("fixture Space instance unavailable");
  return publish(input);
}

/** Private, trusted-service-only Direct/Group admission and MLS adapter. No raw append RPC exists. */
export class ClosedSpace extends DurableObject {
  #store;
  #relay;

  constructor(ctx, env) {
    super(ctx, env);
    this.#store = new SpaceStorage(ctx.storage.sql);
    ctx.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS closed_invites (code_hash TEXT PRIMARY KEY, bytes BLOB NOT NULL)",
    );
    this.#relay = new RelayPublications(ctx, {
      nowMs: () => this._nowMs(),
      dependencies: () => {
        if (
          !this.env.RELAY_DIRECTORY ||
          typeof this.env.RELAY_DIRECTORY.relayDispatch !== "function"
        )
          throw internal("Directory relay callback binding is unavailable");
      },
      verify: (input) =>
        invoke(relayPublicationMetadata, input.frame, JSON.stringify(input.session)),
      preview: (input) => this.#previewRelay(input),
      publish: (input) => this.#publishInTransaction(input),
      snapshot: (space) => this.#store.snapshot(space, { bind: false }),
      event: (space, seq) => this.#store.eventAt(space, seq),
      dispatch: (packet) => this.env.RELAY_DIRECTORY.relayDispatch(packet),
    });
    fixturePublications.set(this, (input) => {
      const { verified } = verifiedPublication(input);
      const outcome = this.ctx.storage.transactionSync(() => {
        this.#relay.assertFixtureOnly(verified.space_id);
        return this.#publishInTransaction(input);
      });
      if (outcome.error) throw outcome.error;
      return outcome.result;
    });
  }

  // Read-only production clock; local fixture subclasses override it without a production setter.
  _nowMs() {
    return String(Date.now());
  }

  #invite(codeHash) {
    if (codeHash === null) return null;
    const rows = this.ctx.storage.sql
      .exec("SELECT bytes FROM closed_invites WHERE code_hash = ?", codeHash)
      .toArray();
    return rows.length ? decode(new Uint8Array(rows[0].bytes)) : null;
  }

  #saveInvite(invite) {
    this.ctx.storage.sql.exec(
      "INSERT INTO closed_invites(code_hash, bytes) VALUES (?, ?) ON CONFLICT(code_hash) DO UPDATE SET bytes = excluded.bytes",
      invite.code_hash,
      encode(invite),
    );
  }

  #prune(projection, range) {
    if (!range) return projection;
    const scan = this.#store.pruneAdmitted(range, (bytes) => rust(closedPruneEntry, bytes));
    const state = invoke(
      closedPruneState,
      JSON.stringify(projection),
      range.from,
      range.to_exclusive,
      scan.count,
      scan.last ?? "",
    );
    this.#store.replaceAdmittedState(encode(state));
    return state;
  }

  #compact(projection) {
    if (!projection) return;
    const plan = invoke(
      closedPrunePlan,
      JSON.stringify(projection),
      this._nowMs(),
      this.env.ZOEN_PRUNE_CEILING_SECS ?? "",
    );
    this.#prune(projection, plan);
  }

  #publicationContext(input, verified, projection) {
    const lookup = verified.lookup;
    if (input.target != null) {
      if (
        input.target.identity !== lookup.target_identity ||
        typeof input.target.registered !== "boolean"
      )
        throw internal("Directory fact does not match the verified target");
    }
    return {
      session: input.session,
      projection,
      space_id: verified.space_id,
      history: this.#store.hashes([lookup.seen_seq, lookup.upto_seq].filter((seq) => seq !== null)),
      target: input.target ?? null,
      invite: this.#invite(lookup.invite_hash),
      now_ms: this._nowMs(),
      prune_ceiling_secs: this.env.ZOEN_PRUNE_CEILING_SECS ?? null,
    };
  }

  #previewRelay(input) {
    const { verified } = verifiedPublication(input);
    const original = this.#store.receipt(verified.author, verified.client_id);
    const snapshot = this.#store.snapshot(verified.space_id, { bind: false });
    const context = this.#publicationContext(input, verified, decode(snapshot.admissionState));
    try {
      return invoke(
        closedRelayPreview,
        input.frame,
        JSON.stringify(context),
        original?.bytes ?? new Uint8Array(),
      );
    } catch (error) {
      let detail;
      try {
        detail = JSON.parse(error.message);
      } catch {}
      if (detail?.permanent !== true) throw error;
      // Like native append admission, a signed refusal can safely compact old
      // ciphertext. Its rejection still creates no event or membership change.
      this.#compact(context.projection);
      return { error };
    }
  }

  #publishInTransaction(input) {
    const { session, verified } = verifiedPublication(input);
    const original = this.#store.receipt(verified.author, verified.client_id);
    const snapshot = this.#store.snapshot(verified.space_id, { bind: false });
    const projection = decode(snapshot.admissionState);
    if (original) {
      const result = invoke(
        closedReplay,
        input.frame,
        session,
        JSON.stringify(projection),
        original.bytes,
      );
      this.#compact(projection);
      return { result };
    }

    const context = this.#publicationContext(input, verified, projection);
    let accepted;
    try {
      accepted = invoke(closedPublish, input.frame, JSON.stringify(context));
    } catch (error) {
      let detail;
      try {
        detail = JSON.parse(error.message);
      } catch {}
      if (detail?.permanent !== true) throw error;
      // Native compacts safe old ciphertext even when this valid publication
      // is refused. Commit compaction before propagating its original refusal.
      this.#compact(projection);
      return { error };
    }
    const event = accepted.event;
    const appended = this.#store.appendAdmitted({
      spaceId: verified.space_id,
      author: verified.author,
      clientId: verified.client_id,
      expectedHead: snapshot.head,
      event: {
        seq: event.seq,
        prev: event.prev,
        hash: event.hash,
        bytes: Uint8Array.from(atob(event.bytes_base64), (char) => char.charCodeAt(0)),
      },
      admissionState: encode(accepted.projection),
    });
    if (accepted.kind !== "new" || appended.kind !== "new")
      throw internal("Atomic append did not match Rust acceptance");
    if (accepted.invite_update !== null) this.#saveInvite(accepted.invite_update);
    accepted.projection = this.#prune(accepted.projection, accepted.prune);
    return { result: accepted };
  }

  prepareRelayPublication(input) {
    return this.#relay.prepare(input);
  }
  commitRelayPublication(input) {
    return this.#relay.commit(input);
  }
  alarm() {
    return this.#relay.alarm();
  }

  createInvite(input) {
    return this.ctx.storage.transactionSync(() => {
      const projection = decode(
        this.#store.snapshot(input?.spaceId, { bind: false }).admissionState,
      );
      const request = {
        who: input.who,
        code: input.code,
        role: input.role,
        max_uses: input.max_uses,
        ttl_secs: input.ttl_secs,
        now_ms: this._nowMs(),
        existing: null,
      };
      // Rust supplies the normalized key; the host never hashes or interprets a capability.
      const candidate = invoke(closedInvite, JSON.stringify(projection), JSON.stringify(request));
      request.existing = this.#invite(candidate.code_hash);
      const invite =
        request.existing === null
          ? candidate
          : invoke(closedInvite, JSON.stringify(projection), JSON.stringify(request));
      this.#saveInvite(invite);
      return invite;
    });
  }

  snapshot(input) {
    return this.ctx.storage.transactionSync(() => {
      const snapshot = this.#store.snapshot(input?.spaceId, { bind: false });
      return { head: snapshot.head, projection: decode(snapshot.admissionState) };
    });
  }

  read(input) {
    return this.ctx.storage.transactionSync(() => this.#store.read(input, { bind: false }));
  }

  #readProjection(spaceId) {
    const snapshot = this.#store.snapshot(spaceId, { bind: false });
    const projection = decode(snapshot.admissionState);
    if (
      snapshot.head?.seq !== projection?.head?.seq ||
      snapshot.head?.hash !== projection?.head?.hash
    )
      throw internal("Space projection does not match its authoritative head");
    return projection;
  }
  #readContext(input) {
    return JSON.stringify({
      space_id: input.spaceId,
      session: input.session,
      projection: this.#readProjection(input.spaceId),
    });
  }

  /** Private Directory-derived session only. This is not active-device auth. */
  visibility(input) {
    return this.ctx.storage.transactionSync(() =>
      invoke(closedVisibility, this.#readContext(input)),
    );
  }

  /** Current membership only; unknown Spaces are not bound by this read. */
  coMembers(input) {
    return this.ctx.storage.transactionSync(() => ({
      // Server-only membership subject, not active-device authorization.
      members: invoke(
        closedCoMembers,
        JSON.stringify({
          space_id: input.spaceId,
          identity: input.identity,
          projection: this.#readProjection(input.spaceId),
        }),
      ),
    }));
  }

  readAuthorized(input) {
    if (!Number.isSafeInteger(input?.limit) || input.limit < 1 || input.limit > 256)
      throw internal("Invalid authorized Space read limit");
    return this.ctx.storage.transactionSync(() => {
      const plan = invoke(closedReadPlan, this.#readContext(input), input.from, input.limit);
      const page = this.#store.readAuthorizedPage({
        spaceId: input.spaceId,
        from: plan.from,
        through: plan.through,
        limit: plan.limit,
      });
      return { ...page, through: plan.through, visibility: plan.visibility };
    });
  }
}
