import test from 'node:test';
import assert from 'node:assert/strict';
import { App } from '@modelcontextprotocol/ext-apps';
import { z } from 'zod/v4';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import { MiniappSession, type HostOptions } from './protocol.ts';
import { stringifyMiniappRequest } from './json.ts';

const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
function fixture(overrides: Partial<HostOptions> = {}) {
  const messages: unknown[] = [];
  const calls: { name: string; args: Record<string, unknown> }[] = [];
  const shares: string[] = [];
  let transport: Transport | undefined;
  const host: HostOptions = {
    send: (message) => {
      messages.push(message);
      transport?.onmessage?.(message as JSONRPCMessage);
    },
    initialize: () => ({
      hostInfo: { name: 'zoen', version: '0.1.0' },
      hostCapabilities: {
        serverTools: {},
        serverResources: {},
        openLinks: {},
        message: { text: {} },
        updateModelContext: { text: {}, structuredContent: {} },
      },
      hostContext: {
        theme: 'light',
        displayMode: 'fullscreen',
        availableDisplayModes: ['inline', 'fullscreen'],
      },
    }),
    state: () => ({ content: [], structuredContent: { actual: true }, _meta: { local: 'kept' } }),
    arguments: () => ({ launch: 'actual input' }),
    context: () => ({ theme: 'light', displayMode: 'fullscreen' }),
    tools: new Set(['list_add']),
    resourceUri: 'ui://roda/list',
    call: async (name, args) => {
      calls.push({ name, args });
      return {
        content: [{ type: 'text', text: 'Done' }],
        structuredContent: { accepted: true },
        _meta: { drawing: 'kept' },
      };
    },
    resource: async () => ({
      contents: [
        { uri: 'ui://roda/list', mimeType: 'text/html;profile=mcp-app', text: '<html></html>' },
      ],
    }),
    openLink: async () => ({}),
    share: async (text) => {
      shares.push(text);
      return {};
    },
    updateModelContext: async () => ({}),
    native: async (_capability, params) => ({ allowed: params }),
    haptic: () => {},
    resize: () => {},
    close: () => {},
    ...overrides,
  };
  const session = new MiniappSession(host);
  const receive = (message: unknown) => session.receive(JSON.stringify(message));
  const initialize = async () => {
    await receive({
      jsonrpc: '2.0',
      id: 'init',
      method: 'ui/initialize',
      params: {
        protocolVersion: '2026-01-26',
        appInfo: { name: 'fixture', version: '1' },
        appCapabilities: {},
      },
    });
    await receive({ jsonrpc: '2.0', method: 'ui/notifications/initialized' });
    await tick();
  };
  const connect = async () => {
    const app = new App({ name: 'real-official-app', version: '1' }, {}, { autoResize: false });
    const notifications: { kind: string; value: unknown }[] = [];
    app.ontoolinput = (value) => notifications.push({ kind: 'input', value });
    app.ontoolresult = (value) => notifications.push({ kind: 'result', value });
    app.onteardown = async () => ({});
    transport = {
      start: async () => {},
      close: async () => {},
      send: async (message) => {
        void session.receive(JSON.stringify(message));
      },
    };
    await app.connect(transport);
    await tick();
    return { app, notifications };
  };
  return { session, messages, calls, shares, receive, initialize, connect };
}

test('official App connects to production AppBridge and receives actual input, result metadata and tool/resource results', async (t) => {
  const f = fixture();
  const { app, notifications } = await f.connect();
  t.after(() => {
    f.session.dispose();
    return app.close();
  });
  assert.equal(app.getHostVersion()?.name, 'zoen');
  assert.equal(app.getHostContext()?.displayMode, 'fullscreen');
  assert.deepEqual(
    notifications.map((n) => n.kind),
    ['input', 'result'],
  );
  assert.deepEqual(notifications[0].value, { arguments: { launch: 'actual input' } });
  assert.deepEqual(
    notifications[1].value,
    f.messages.find((m: any) => m.method === 'ui/notifications/tool-result') && {
      content: [],
      structuredContent: { actual: true },
      _meta: { local: 'kept' },
    },
  );
  const result = await app.callServerTool({ name: 'list_add', arguments: { text: 'Real' } });
  assert.deepEqual(result._meta, { drawing: 'kept' });
  assert.deepEqual(f.calls, [{ name: 'list_add', args: { text: 'Real' } }]);
  const resource = await app.readServerResource({ uri: 'ui://roda/list' });
  assert.equal(resource.contents[0].mimeType, 'text/html;profile=mcp-app');
  f.session.sync();
  await tick();
  assert.equal(notifications.filter((n) => n.kind === 'input').length, 1);
});

test('standard ui/message content array is confirmed once; old objects and mixed media cannot silently become chat text', async (t) => {
  const f = fixture();
  const { app } = await f.connect();
  t.after(() => {
    f.session.dispose();
    return app.close();
  });
  await app.sendMessage({
    role: 'user',
    content: [
      { type: 'text', text: 'First' },
      { type: 'text', text: 'Second' },
    ],
  });
  assert.deepEqual(f.shares, ['First\nSecond']);
  await f.receive({
    jsonrpc: '2.0',
    id: 'old',
    method: 'ui/message',
    params: { role: 'user', content: { type: 'text', text: 'Wrong' } },
  });
  await assert.rejects(
    app.sendMessage({
      role: 'user',
      content: [
        { type: 'image', data: 'YQ==', mimeType: 'image/png' },
        { type: 'text', text: 'Drop image?' },
      ],
    }),
  );
  assert.deepEqual(f.shares, ['First\nSecond']);
});

test('official requests keep signed tool/resource authority and the initialization gate', async (t) => {
  const f = fixture();
  t.after(() => f.session.dispose());
  await f.receive({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'list_add' } });
  assert.equal(f.calls.length, 0);
  await f.initialize();
  await f.receive({ jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'pet_release' } });
  await f.receive({
    jsonrpc: '2.0',
    id: 4,
    method: 'resources/read',
    params: { uri: 'ui://roda/hike' },
  });
  assert.equal(f.calls.length, 0);
  assert.ok(JSON.stringify(f.messages.at(-1)).includes('does not belong'));
  await f.receive({
    jsonrpc: '2.0',
    id: 5,
    method: 'tools/call',
    params: { name: 'list_add', arguments: { text: 'Real' } },
  });
  assert.equal(f.calls.length, 1);
});

test('official schemas preserve lossless BRL integer and decimal tool tokens through the Rust boundary', async (t) => {
  let encoded = '';
  const f = fixture({
    call: async (_name, args) => {
      encoded = stringifyMiniappRequest(args);
      return { content: [] };
    },
  });
  t.after(() => f.session.dispose());
  await f.initialize();
  await f.session.receive(
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_add","arguments":{"cents":9007199254740993,"fraction":0.123456789123456789,"quoted":"9007199254740993","normal":123}}}',
  );
  assert.equal(
    encoded,
    '{"cents":9007199254740993,"fraction":0.123456789123456789,"quoted":"9007199254740993","normal":123}',
  );
});

test('duplicate in-flight IDs and the sixteen-request bound cannot duplicate effects or publish after disposal', async () => {
  const finishes: (() => void)[] = [];
  let calls = 0;
  const f = fixture({
    call: async () => {
      calls++;
      await new Promise<void>((resolve) => finishes.push(resolve));
      return { content: [] };
    },
  });
  await f.initialize();
  const request = (id: number) => ({
    jsonrpc: '2.0',
    id,
    method: 'tools/call',
    params: { name: 'list_add' },
  });
  const first = f.receive(request(0));
  await tick();
  await f.receive(request(0));
  const rest = Array.from({ length: 15 }, (_, i) => f.receive(request(i + 1)));
  await tick();
  await f.receive(request(16));
  assert.equal(calls, 16);
  f.session.dispose();
  const count = f.messages.length;
  finishes.forEach((finish) => finish());
  await Promise.all([first, ...rest]);
  await tick();
  assert.equal(f.messages.length, count);
  await f.receive(request(17));
  assert.equal(calls, 16);
});

test('malformed initialization, unsafe IDs and oversized envelopes never admit native or chat effects', async (t) => {
  let native = 0;
  const f = fixture({
    native: async () => {
      native++;
      return {};
    },
  });
  t.after(() => f.session.dispose());
  await f.receive({ jsonrpc: '2.0', id: 1, method: 'ui/initialize' });
  await f.receive({ jsonrpc: '2.0', method: 'ui/notifications/initialized' });
  await f.receive({ jsonrpc: '2.0', id: 2, method: 'zoen/native/location', params: {} });
  assert.equal(native, 0);
  await f.initialize();
  await f.receive({
    jsonrpc: '2.0',
    id: 'x'.repeat(129),
    method: 'zoen/native/location',
    params: {},
  });
  await f.session.receive(
    '{"jsonrpc":"2.0","id":9007199254740993,"method":"zoen/native/location","params":{}}',
  );
  await f.receive({
    jsonrpc: '2.0',
    id: 3,
    method: 'zoen/native/location',
    params: { large: 'x'.repeat(1_500_000) },
  });
  assert.equal(native, 0);
});

test('official custom request carries native consent and ephemeral model context through their exact host callbacks', async (t) => {
  const contexts: unknown[] = [];
  const f = fixture({
    updateModelContext: async (p) => {
      contexts.push(p);
      return {};
    },
  });
  const { app } = await f.connect();
  t.after(() => {
    f.session.dispose();
    return app.close();
  });
  const native = await app.request(
    { method: 'zoen/native/calendar.freebusy', params: { startMs: 123, endMs: 456 } },
    z.record(z.string(), z.unknown()),
  );
  assert.deepEqual(native, { data: { allowed: { startMs: 123, endMs: 456 } } });
  await app.updateModelContext({
    content: [{ type: 'text', text: 'Local drawing description' }],
    structuredContent: { shape: 'rectangle' },
  });
  assert.deepEqual(contexts, [
    {
      content: [{ type: 'text', text: 'Local drawing description' }],
      structuredContent: { shape: 'rectangle' },
    },
  ]);
});

test('external display-mode policy can change presentation without invoking builtin close policy', async (t) => {
  const modes: string[] = [];
  let closed = 0;
  const f = fixture({
    close: () => {
      closed++;
    },
    requestDisplayMode: async (mode) => {
      modes.push(mode);
      return { mode };
    },
  });
  const { app } = await f.connect();
  t.after(() => {
    f.session.dispose();
    return app.close();
  });
  assert.deepEqual(await app.requestDisplayMode({ mode: 'inline' }), { mode: 'inline' });
  assert.deepEqual(await app.requestDisplayMode({ mode: 'fullscreen' }), { mode: 'fullscreen' });
  assert.deepEqual(modes, ['inline', 'fullscreen']);
  assert.equal(closed, 0);
});

test('context-only resize/theme/display changes do not replay the original drawing input or result', async (t) => {
  let context: Record<string, unknown> = { theme: 'light', displayMode: 'fullscreen' };
  const f = fixture({ context: () => context });
  const { app, notifications } = await f.connect();
  t.after(() => {
    f.session.dispose();
    return app.close();
  });
  const updates: unknown[] = [];
  app.onhostcontextchanged = (value) => updates.push(value);
  context = {
    theme: 'dark',
    displayMode: 'inline',
    containerDimensions: { width: 320, height: 480 },
  };
  f.session.syncContext();
  await tick();
  assert.deepEqual(
    notifications.map((item) => item.kind),
    ['input', 'result'],
  );
  assert.equal(app.getHostContext()?.theme, 'dark');
  assert.equal(app.getHostContext()?.displayMode, 'inline');
  assert.deepEqual(app.getHostContext()?.containerDimensions, { width: 320, height: 480 });
  assert.equal(updates.length, 1);
  f.session.syncContext();
  await tick();
  assert.equal(updates.length, 1, 'identical host geometry does not publish redundant context');
  assert.equal(notifications.length, 2, 'original create_view result stays single-delivery');
});
