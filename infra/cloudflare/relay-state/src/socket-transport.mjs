// Actual ordinary WebSocket handles; no session or identity authority.
// Directory owns SQL authorization, durable queues, expiry, and application bytes.
export class SocketTransport {
  #bySocket = new Map();
  #byId = new Map();
  #limit;

  constructor(limit) {
    if (!Number.isSafeInteger(limit) || limit < 1) throw new TypeError("Invalid socket bound");
    this.#limit = limit;
  }

  accept(socket, id, handlers) {
    if (typeof id !== "string" || !id || this.#byId.has(id) || this.#bySocket.has(socket))
      throw new TypeError("Invalid or duplicate connection identity");
    if (this.#bySocket.size >= this.#limit) throw new Error("Socket capacity reached");
    for (const name of ["message", "close", "error"])
      if (typeof handlers[name] !== "function") throw new TypeError(`Missing ${name} handler`);

    const entry = { socket, id, closing: false, listeners: null };
    const current = () => this.#bySocket.get(socket) === entry;
    const listeners = {
      message: (event) => {
        if (current() && this.isOpen(socket)) handlers.message(socket, event.data);
      },
      close: (event) => {
        if (!current()) return;
        entry.closing = true;
        let completed = false;
        try {
          // Retain the actual id through durable terminal bookkeeping. If it
          // throws, a real alarm may retry this still-retained CLOSED handle.
          handlers.close(socket, event.code, event.reason, event.wasClean);
          completed = true;
        } finally {
          if (completed) this.releaseTerminal(socket);
        }
      },
      error: (event) => {
        if (!current()) return;
        entry.closing = true;
        let completed = false;
        try {
          handlers.error(socket, event);
          completed = true;
        } finally {
          if (completed) this.releaseTerminal(socket);
        }
      },
    };
    entry.listeners = listeners;
    this.#bySocket.set(socket, entry);
    this.#byId.set(id, socket);
    try {
      // The native protocol reads bytes synchronously. Ordinary WebSockets
      // default to Blob on current compatibility dates; set this before accept.
      socket.binaryType = "arraybuffer";
      for (const [name, listener] of Object.entries(listeners))
        socket.addEventListener(name, listener);
      // Listener/registry installation and acceptance are one synchronous turn.
      socket.accept();
    } catch (failure) {
      entry.closing = true;
      try {
        socket.close(1011, "connection acceptance failed");
      } catch {}
      // An unpublished, never-accepted CONNECTING handle has no live connection.
      // An actual OPEN/CLOSING handle retains its reservation until terminal.
      if (socket.readyState === 0 || socket.readyState === 3) this.#forget(entry);
      throw failure;
    }
  }

  sockets() {
    // Includes CLOSING and unswept CLOSED handles. Directory retires terminal rows
    // before admitting another connection; no live handle is removed by a timer.
    return [...this.#bySocket.keys()];
  }

  id(socket) {
    return this.#bySocket.get(socket)?.id ?? null;
  }

  has(id) {
    return this.#byId.has(id);
  }

  forId(id) {
    const socket = this.#byId.get(id);
    return socket && this.isOpen(socket) ? socket : null;
  }

  isOpen(socket) {
    const entry = this.#bySocket.get(socket);
    return Boolean(entry && !entry.closing && socket.readyState === 1);
  }

  markClosing(socket) {
    const entry = this.#bySocket.get(socket);
    if (entry) entry.closing = true;
  }

  close(socket, code, reason) {
    this.markClosing(socket);
    // Close initiation is not SQL terminalization or permission to erase an id.
    // Only the real close/error callback or a real CLOSED sweep releases it.
    if (socket.readyState !== 3) socket.close(code, reason);
  }

  releaseTerminal(socket) {
    const entry = this.#bySocket.get(socket);
    if (!entry || socket.readyState !== 3) return false;
    this.#forget(entry);
    return true;
  }

  #forget(entry) {
    if (this.#bySocket.get(entry.socket) !== entry) return;
    for (const [name, listener] of Object.entries(entry.listeners))
      entry.socket.removeEventListener(name, listener);
    this.#bySocket.delete(entry.socket);
    if (this.#byId.get(entry.id) === entry.socket) this.#byId.delete(entry.id);
  }
}
