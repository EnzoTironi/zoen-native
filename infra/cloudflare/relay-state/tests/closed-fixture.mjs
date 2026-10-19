// Local Miniflare bridge only; never referenced by the private deployment config.
import {
  ClosedSpace as ProductionClosedSpace,
  verifiedPublication,
  fixturePublication,
} from "../src/closed-space.mjs";

function base64(bytes) {
  let result = "";
  for (let offset = 0; offset < bytes.byteLength; offset += 8192)
    result += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
  return btoa(result);
}

export class ClosedSpace extends ProductionClosedSpace {
  #clock = "0";
  _nowMs() {
    return this.#clock;
  }

  publishAt(input) {
    this.#clock = input.test_now_ms;
    return fixturePublication(this, input);
  }

  inviteAt(input) {
    this.#clock = input.test_now_ms;
    return super.createInvite(input);
  }

  inspect() {
    const sql = this.ctx.storage.sql;
    return {
      binding: sql.exec("SELECT * FROM space_binding ORDER BY singleton").toArray(),
      entries: sql
        .exec("SELECT seq_key, author, client_id, prev, hash, bytes FROM entries ORDER BY seq_key")
        .toArray()
        .map((row) => ({ ...row, bytes: Array.from(new Uint8Array(row.bytes)) })),
      state: sql
        .exec("SELECT bytes FROM admission_state ORDER BY singleton")
        .toArray()
        .map((row) => Array.from(new Uint8Array(row.bytes))),
      invites: sql
        .exec("SELECT code_hash, bytes FROM closed_invites ORDER BY code_hash")
        .toArray()
        .map((row) => ({ code_hash: row.code_hash, bytes: Array.from(new Uint8Array(row.bytes)) })),
    };
  }

  rejectInviteWrites() {
    this.ctx.storage.sql.exec(`CREATE TRIGGER reject_invite_update BEFORE UPDATE ON closed_invites
      BEGIN SELECT RAISE(ABORT, 'test-only invite write failure'); END`);
  }

  rejectPruneWrites() {
    this.ctx.storage.sql.exec(`CREATE TRIGGER reject_prune_update BEFORE UPDATE OF bytes ON entries
      BEGIN SELECT RAISE(ABORT, 'test-only pruning write failure'); END`);
  }
  allowPruneWrites() {
    this.ctx.storage.sql.exec("DROP TRIGGER reject_prune_update");
  }

  allowInviteWrites() {
    this.ctx.storage.sql.exec("DROP TRIGGER reject_invite_update");
  }
}

export default {
  async fetch(request, env) {
    const input = await request.json();
    const stub = env.CLOSED_SPACE.getByName(input.space_id);
    try {
      let result;
      switch (new URL(request.url).pathname) {
        case "/step": {
          const step = input.step;
          if (step.op === "invite") {
            const { now_ms, ...fields } = step.input;
            result = await stub.inviteAt({
              spaceId: input.space_id,
              ...fields,
              test_now_ms: now_ms,
            });
          } else {
            const publish = {
              frame: Uint8Array.from(atob(step.frame_base64), (char) => char.charCodeAt(0)),
              session: step.session,
              test_now_ms: step.now_ms,
            };
            publish.target = input.target_override ?? null;
            if (input.target_override == null && step.target_registered !== null) {
              const { verified } = verifiedPublication(publish);
              publish.target = {
                identity: verified.lookup.target_identity,
                registered: step.target_registered,
              };
            }
            result = await stub.publishAt(publish);
          }
          break;
        }
        case "/snapshot":
          result = await stub.snapshot({ spaceId: input.space_id });
          break;
        case "/inspect":
          result = await stub.inspect();
          break;
        case "/read":
          result = await stub.read({
            spaceId: input.space_id,
            from: input.from ?? "0",
            limit: 256,
          });
          break;
        case "/raw-append":
          result = await stub.appendAdmitted({});
          break;
        case "/raw-publish":
          result = await stub.publish({});
          break;
        case "/read-authorized":
          result = await stub.readAuthorized({
            spaceId: input.space_id,
            session: input.session,
            from: input.from,
            limit: input.limit,
          });
          // The test-only HTTP bridge encodes typed arrays compactly. Production
          // RPC retains binary Uint8Array; numeric-key JSON can amplify 4MiB pages.
          result.events = result.events.map((event) => ({ ...event, bytes: base64(event.bytes) }));
          break;
        case "/co-members":
          result = await stub.coMembers({ spaceId: input.space_id, session: input.session });
          break;
        case "/visibility":
          result = await stub.visibility({ spaceId: input.space_id, session: input.session });
          break;
        case "/reject-invite-writes":
          await stub.rejectInviteWrites();
          result = null;
          break;
        case "/allow-invite-writes":
          await stub.allowInviteWrites();
          result = null;
          break;
        case "/reject-prune-writes":
          await stub.rejectPruneWrites();
          result = null;
          break;
        case "/allow-prune-writes":
          await stub.allowPruneWrites();
          result = null;
          break;
        default:
          return new Response("Not found", { status: 404 });
      }
      return Response.json({ ok: true, result });
    } catch (error) {
      let detail;
      try {
        detail = JSON.parse(error.message ?? String(error));
      } catch {
        detail = { reason: error.message ?? String(error) };
      }
      return Response.json({ ok: false, error: detail }, { status: 400 });
    }
  },
};
