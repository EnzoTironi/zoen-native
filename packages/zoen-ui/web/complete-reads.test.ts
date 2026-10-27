import test from "node:test";
import assert from "node:assert/strict";
import {
  install,
  unitCodec,
  vault,
  login,
  socket,
  settle,
  challenge,
  ready,
  messagingCore,
  linked,
} from "./transport-tests/fixture.ts";
import { AuthenticatedTransport } from "./transport.ts";
import { Messages } from "./messages.ts";
import type { CoreOperation } from "./worker-client.ts";

test("complete-read capability belongs only to the authenticated connection generation", async (t) => {
  install(t);
  const transport = new AuthenticatedTransport(
    unitCodec(),
    vault,
    login,
    () => {},
    async () => {},
  );
  t.after(() => transport.stop());
  transport.start();
  socket().open();
  await settle();
  socket().frame({ ...challenge, capabilities: ["complete-set-reads-v1"] });
  await settle();
  socket().frame(ready);
  await settle();
  assert.equal(transport.completeReads, true);
  socket().close();
  t.mock.timers.tick(900);
  await settle();
  assert.equal(transport.completeReads, false);
  socket().open();
  await settle();
  socket().frame({ ...challenge, capabilities: [] });
  await settle();
  socket().frame(ready);
  await settle();
  assert.equal(transport.completeReads, false);
});
test("the real message controller supplies negotiated read capability before each bootstrap", async (t) => {
  install(t);
  const base = messagingCore();
  const calls: CoreOperation[] = [];
  const core = {
    ...base,
    text: async (op: CoreOperation) => {
      calls.push(op);
      return base.text(op);
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  socket().open();
  await settle();
  socket().frame({ ...challenge, capabilities: ["complete-set-reads-v1"] });
  await settle();
  socket().frame(ready);
  await settle();
  const enabled = calls.findIndex((op) => op.kind === "read_capability");
  const boot = calls.findIndex((op) => op.kind === "bootstrap");
  assert.ok(enabled >= 0 && enabled < boot);
  assert.deepEqual(calls[enabled], { kind: "read_capability", enabled: true });
  calls.length = 0;
  socket().close();
  t.mock.timers.tick(900);
  await settle();
  socket().open();
  await settle();
  socket().frame({ ...challenge, capabilities: [] });
  await settle();
  socket().frame(ready);
  await settle();
  assert.deepEqual(
    calls.find((op) => op.kind === "read_capability"),
    { kind: "read_capability", enabled: false },
  );
});
