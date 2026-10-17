import test from 'node:test';
import assert from 'node:assert/strict';
import { MiniappSession, type HostOptions } from './protocol.ts';

function fixture(overrides: Partial<HostOptions> = {}) {
  const messages: unknown[] = [];
  const calls: string[] = [];
  const host: HostOptions = {
    send: (message) => messages.push(message),
    initialize: () => ({ protocolVersion: '2026-01-26' }),
    state: () => ({ structuredContent: { actual: true } }),
    context: () => ({ theme: 'light' }),
    tools: new Set(['list_add']),
    resourceUri: 'ui://roda/list',
    call: async (name) => {
      calls.push(name);
      return { isError: false };
    },
    resource: async () => ({ contents: [] }),
    openLink: async () => ({}),
    share: async () => ({}),
    native: async () => ({}),
    haptic: () => {},
    resize: () => {},
    close: () => {},
    ...overrides,
  };
  const session = new MiniappSession(host);
  const receive = (message: unknown) => session.receive(JSON.stringify(message));
  const initialize = async () => {
    await receive({ jsonrpc: '2.0', id: 1, method: 'ui/initialize' });
    await receive({ jsonrpc: '2.0', method: 'ui/notifications/initialized' });
  };
  return { session, messages, calls, receive, initialize };
}
test('a frame must initialize and cannot invoke another miniapp tool or resource', async () => {
  const f = fixture();
  await f.receive({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'list_add' } });
  assert.equal(f.calls.length, 0);
  await f.initialize();
  await f.receive({ jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'pet_release' } });
  assert.equal(f.calls.length, 0);
  await f.receive({
    jsonrpc: '2.0',
    id: 4,
    method: 'resources/read',
    params: { uri: 'ui://roda/hike' },
  });
  assert.ok(JSON.stringify(f.messages.at(-1)).includes('não pertence'));
  await f.receive({
    jsonrpc: '2.0',
    id: 5,
    method: 'tools/call',
    params: { name: 'list_add', arguments: { text: 'Real' } },
  });
  assert.deepEqual(f.calls, ['list_add']);
});
test('duplicate in-flight ids execute once and disposed frames cannot publish late responses', async () => {
  let finish: ((value: unknown) => void) | undefined;
  let calls = 0;
  const f = fixture({
    call: () => {
      calls++;
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  });
  await f.initialize();
  const request = { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'list_add' } };
  const first = f.receive(request);
  await f.receive(request);
  assert.equal(calls, 1);
  f.session.dispose();
  const count = f.messages.length;
  finish!({ isError: false });
  await first;
  assert.equal(f.messages.length, count);
  await f.receive({ ...request, id: 3 });
  assert.equal(calls, 1);
});
