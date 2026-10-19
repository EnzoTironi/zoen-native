// Registry unit doubles only. Actual workerd/close proof is socket-lifecycle.test.
import assert from "node:assert/strict";
import test from "node:test";
import { SocketTransport } from "../src/socket-transport.mjs";
class Handle extends EventTarget {
  readyState = 0;
  binaryType = "blob";
  accept() {
    assert.equal(this.binaryType, "arraybuffer", "Native byte mode must precede actual acceptance");
    this.readyState = 1;
  }
  close() {
    if (this.readyState !== 3) this.readyState = 2;
  }
}
const handlers = (overrides = {}) => ({ message() {}, close() {}, error() {}, ...overrides });
test("a closing handle retains its bounded reservation until actual terminal state", () => {
  const transport = new SocketTransport(1),
    socket = new Handle();
  transport.accept(socket, "one", handlers());
  transport.close(socket, 1000, "close initiation");
  assert.equal(socket.readyState, 2);
  assert.equal(transport.forId("one"), null);
  assert.equal(transport.id(socket), "one");
  assert.equal(transport.releaseTerminal(socket), false);
  assert.throws(() => transport.accept(new Handle(), "two", handlers()), /capacity/);
  socket.readyState = 3;
  socket.dispatchEvent(new Event("close"));
  assert.equal(transport.id(socket), null);
  transport.accept(new Handle(), "two", handlers());
});
test("a failed terminal callback preserves its exact handle id for a later real sweep", () => {
  // EventTarget exceptions become detached Node failures. Capture the installed
  // callback directly here; this is a registry unit, never a physical-close claim.
  const listeners = [];
  const second = new Handle();
  second.addEventListener = (name, listener) => listeners.push({ name, listener });
  const other = new SocketTransport(1);
  other.accept(
    second,
    "failed",
    handlers({
      close() {
        assert.equal(other.id(second), "failed", "Id is retained during durable callback");
        throw new Error("owned storage failure");
      },
    }),
  );
  second.readyState = 3;
  assert.throws(
    () => listeners.find((entry) => entry.name === "close").listener({ code: 1000 }),
    /storage failure/,
  );
  assert.equal(other.id(second), "failed");
  assert.equal(other.releaseTerminal(second), true);
  assert.equal(other.id(second), null);
});
test("accept failure releases an unpublished handle without leaking its reservation", () => {
  const transport = new SocketTransport(1),
    socket = new Handle();
  socket.accept = () => {
    throw new Error("owned accept failure");
  };
  socket.close = () => {};
  const knownId = "exact-server-owned-key";
  assert.throws(() => transport.accept(socket, knownId, handlers()), /accept failure/);
  assert.equal(transport.id(socket), null);
  transport.accept(new Handle(), "next", handlers());
});
test("closing generations cannot dispatch another queued message or become open after a send fence", () => {
  const transport = new SocketTransport(1),
    socket = new Handle();
  let count = 0;
  transport.accept(
    socket,
    "one",
    handlers({
      message() {
        count++;
      },
    }),
  );
  socket.dispatchEvent(new MessageEvent("message", { data: new Uint8Array([1]) }));
  transport.markClosing(socket);
  socket.dispatchEvent(new MessageEvent("message", { data: new Uint8Array([2]) }));
  assert.equal(count, 1);
  assert.equal(transport.isOpen(socket), false);
});
