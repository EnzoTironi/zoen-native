// Bounded synthetic protocol peer, not a production relay or authorization oracle.
import assert from 'node:assert/strict';
import { createHash, createPublicKey, verify } from 'node:crypto';
function fields(bytes) {
  let at = 0;
  const number = () => {
    let n = 0,
      shift = 0;
    for (let i = 0; i < 8; ++i) {
      assert.ok(at < bytes.length);
      const b = bytes[at++];
      n += (b & 127) * 2 ** shift;
      if (!(b & 128)) {
        assert.ok(Number.isSafeInteger(n));
        return n;
      }
      shift += 7;
    }
    throw Error('Synthetic varint overflow');
  };
  const out = [];
  while (at < bytes.length) {
    const tag = number(),
      wire = tag & 7,
      key = tag >>> 3;
    assert.ok(key > 0);
    if (wire === 0) out.push([key, number()]);
    else {
      assert.equal(wire, 2);
      const n = number();
      assert.ok(n <= bytes.length - at);
      out.push([key, bytes.subarray(at, (at += n))]);
    }
  }
  return out;
}
const value = (rows, key) => rows.find(([field]) => field === key)?.[1];
const text = (rows, key) => new TextDecoder().decode(value(rows, key));
function websocketFrame(bytes) {
  const b = Buffer.from(bytes);
  assert.ok(b.length <= 1_048_576);
  if (b.length < 126) return Buffer.concat([Buffer.from([0x82, b.length]), b]);
  if (b.length < 65_536) {
    const h = Buffer.alloc(4);
    h[0] = 0x82;
    h[1] = 126;
    h.writeUInt16BE(b.length, 2);
    return Buffer.concat([h, b]);
  }
  const h = Buffer.alloc(10);
  h[0] = 0x82;
  h[1] = 127;
  h.writeBigUInt64BE(BigInt(b.length), 2);
  return Buffer.concat([h, b]);
}
export function syntheticRelay(server, fixture, failures) {
  const sockets = new Set();
  const metrics = {
    connections: 0,
    authenticated: 0,
    pongs: 0,
    delayedPong: false,
    fullSync: [],
    publicationCopies: 0,
    publicationBytesIdentical: true,
  };
  let publication;
  const publicKey = createPublicKey({
    key: Buffer.concat([
      Buffer.from('302a300506032b6570032100', 'hex'),
      Buffer.from(fixture.account.device, 'hex'),
    ]),
    format: 'der',
    type: 'spki',
  });
  server.on('upgrade', (request, socket, head) => {
    if (request.url !== '/v1/sync' || typeof request.headers['sec-websocket-key'] !== 'string') {
      socket.destroy();
      return;
    }
    sockets.add(socket);
    ++metrics.connections;
    const connection = metrics.connections;
    let delayedPong;
    socket.once('close', () => {
      clearTimeout(delayedPong);
      sockets.delete(socket);
    });
    socket.on('error', () => {});
    const accept = createHash('sha1')
      .update(request.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11')
      .digest('base64');
    socket.write(
      `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
    );
    let buffer = Buffer.from(head),
      phase = 'hello',
      start = 0,
      scans = 0;
    const send = (bytes) => socket.write(websocketFrame(bytes));
    const receive = (bytes) => {
      const [[kind, body]] = fields(bytes);
      if (phase === 'hello') {
        assert.equal(kind, 1);
        const hello = fields(body);
        assert.equal(value(hello, 1), 4);
        assert.equal(text(hello, 3), fixture.account.identity);
        assert.equal(text(hello, 4), fixture.account.device);
        assert.equal(text(hello, 5), fixture.account.cert);
        phase = 'auth';
        send(fixture.challenge);
        return;
      }
      if (phase === 'auth') {
        assert.equal(kind, 2);
        assert.ok(
          verify(
            null,
            Buffer.from(fixture.auth_message),
            publicKey,
            Buffer.from(new TextDecoder().decode(body), 'hex'),
          ),
        );
        phase = 'ready';
        ++metrics.authenticated;
        start = performance.now();
        send(fixture.ready);
        return;
      }
      if (kind === 7) {
        const pong = () => {
          ++metrics.pongs;
          send(fixture.pong);
        };
        if (connection === 1 && metrics.pongs === 1) {
          // A bounded1s response delay stays inside the unchanged15s heartbeat
          // budget and queues Pong AFTER the60s repair while a save is held.
          metrics.delayedPong = true;
          delayedPong = setTimeout(pong, 1_000);
        } else pong();
        return;
      }
      if (kind === 4) {
        ++metrics.publicationCopies;
        if (publication) metrics.publicationBytesIdentical &&= publication.equals(body);
        else publication = Buffer.from(body);
        return;
      }
      if (kind !== 5) return; // Unrelated ordinary requests are not fabricated as success.
      const sync = fields(body);
      if (value(sync, 2) !== 1) return;
      const cursors = sync.filter(([key]) => key === 1).map(([, bytes]) => fields(bytes));
      assert.equal(cursors.length, 1);
      assert.equal(text(cursors[0], 1), fixture.space);
      const cursor = value(cursors[0], 2) ?? 0;
      ++scans;
      metrics.fullSync.push({
        connection,
        cursor,
        elapsedMs: Math.floor(performance.now() - start),
      });
      assert.equal(cursor, connection === 1 ? 3 : 4);
      if (connection === 1 && scans === 1) {
        send(fixture.sync_done);
        return;
      }
      assert.ok(connection === 2 || (connection === 1 && scans === 2));
      // The final event was never live-announced. Only a later actual full Sync
      // discovers it; duplicate delivery exercises the existing log admission.
      send(fixture.tail);
      send(fixture.tail);
      send(fixture.sync_done);
    };
    const consume = () => {
      while (buffer.length >= 2) {
        const opcode = buffer[0] & 15;
        assert.equal(buffer[0] & 0x80, 0x80);
        assert.equal(buffer[1] & 0x80, 0x80);
        let length = buffer[1] & 127,
          offset = 2;
        if (length === 126) {
          if (buffer.length < 4) return;
          length = buffer.readUInt16BE(2);
          offset = 4;
        } else if (length === 127) {
          if (buffer.length < 10) return;
          const n = buffer.readBigUInt64BE(2);
          assert.ok(n <= 1_048_576n);
          length = Number(n);
          offset = 10;
        }
        assert.ok(length <= 1_048_576);
        if (buffer.length < offset + 4 + length) return;
        const mask = buffer.subarray(offset, offset + 4),
          bytes = Buffer.from(buffer.subarray(offset + 4, offset + 4 + length));
        buffer = buffer.subarray(offset + 4 + length);
        for (let i = 0; i < bytes.length; ++i) bytes[i] ^= mask[i % 4];
        if (opcode === 8) {
          socket.end(Buffer.from([0x88, 0]));
          return;
        }
        assert.equal(opcode, 2);
        receive(bytes);
      }
    };
    socket.on('data', (data) => {
      try {
        buffer = Buffer.concat([buffer, data]);
        assert.ok(buffer.length <= 2_097_152);
        consume();
      } catch {
        failures.push('synthetic_protocol_peer');
        socket.destroy();
      }
    });
    try {
      consume();
    } catch {
      failures.push('synthetic_protocol_peer');
      socket.destroy();
    }
  });
  return {
    metrics,
    close: () => {
      for (const socket of sockets) socket.destroy();
    },
  };
}
