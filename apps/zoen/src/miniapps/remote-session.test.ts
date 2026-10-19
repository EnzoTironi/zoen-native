import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';
import type { FetchLike, Tool } from '@modelcontextprotocol/client';
import { createRemoteMcpSession, parseRemoteResourcePolicy } from './remote-session.ts';

const uri = 'ui://provider/main.html';
const StreamingResponse = globalThis.Response;
const tools: Tool[] = [
  {
    name: 'paint',
    inputSchema: { type: 'object' },
    _meta: { ui: { resourceUri: uri, visibility: ['model'] } },
  },
  { name: 'save', inputSchema: { type: 'object' }, _meta: { ui: { visibility: ['app'] } } },
  { name: 'query', inputSchema: { type: 'object' }, annotations: { readOnlyHint: true } },
];
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
function fixture(
  options: {
    resource?: Record<string, unknown>;
    toolList?: Tool[];
    readWait?: Promise<void>;
    callWait?: Promise<void>;
    auth?: boolean;
    sse?: boolean;
  } = {},
) {
  const requests: {
    method: string;
    params?: Record<string, unknown>;
    credentials?: RequestCredentials;
    redirect?: RequestRedirect;
  }[] = [];
  let active = true;
  const fetcher: FetchLike = async (input, init) => {
    assert.equal(String(input), 'https://mcp.excalidraw.com/mcp');
    if (options.auth) return new StreamingResponse('Authorization required', { status: 401 });
    if (init?.method === 'GET') return new StreamingResponse(null, { status: 405 });
    const request = JSON.parse(String(init?.body)) as {
      id?: number;
      method: string;
      params?: Record<string, unknown>;
    };
    requests.push({
      method: request.method,
      params: request.params,
      credentials: init?.credentials,
      redirect: init?.redirect,
    });
    const result = (value: unknown) => {
      const json = JSON.stringify({ jsonrpc: '2.0', id: request.id, result: value });
      return new StreamingResponse(options.sse ? `event: message\ndata: ${json}\n\n` : json, {
        headers: { 'content-type': options.sse ? 'text/event-stream' : 'application/json' },
      });
    };
    if (request.method === 'server/discover')
      return new StreamingResponse('Legacy endpoint', { status: 404 });
    if (request.method === 'initialize')
      return result({
        protocolVersion: '2025-11-25',
        serverInfo: { name: 'Synthetic remote provider', version: '1' },
        capabilities: { tools: {}, resources: {} },
      });
    if (
      request.method === 'notifications/initialized' ||
      request.method === 'notifications/cancelled'
    )
      return new StreamingResponse(null, { status: 202 });
    if (request.method === 'tools/list') return result({ tools: options.toolList ?? tools });
    if (request.method === 'resources/read') {
      await options.readWait;
      return result({
        contents: [
          {
            uri,
            mimeType: 'text/html;profile=mcp-app',
            text: '<!doctype html><p>Provider app</p>',
            _meta: {
              ui: {
                csp: {
                  resourceDomains: ['https://assets.example.com'],
                  connectDomains: ['https://api.example.com'],
                },
                permissions: { clipboardWrite: {} },
              },
            },
            ...options.resource,
          },
        ],
      });
    }
    if (request.method === 'tools/call') {
      await options.callWait;
      return result({ content: [{ type: 'text', text: 'Synthetic result' }] });
    }
    throw new Error(`Unexpected synthetic RPC ${request.method}`);
  };
  const session = createRemoteMcpSession(
    'excalidraw',
    () => {
      if (!active) throw new Error('Account changed');
    },
    { fetch: fetcher },
  );
  return {
    session,
    requests,
    retireOwner: () => {
      active = false;
    },
  };
}

test('official SDK negotiates a declared server and reads standard App metadata without tool calls', async () => {
  const f = fixture();
  try {
    const [a, b] = await Promise.all([f.session.connect(), f.session.connect()]);
    assert.equal(a, b);
    assert.equal(a.server.name, 'Synthetic remote provider');
    assert.equal(a.server.protocolVersion, '2025-11-25');
    assert.equal(a.mainToolName, 'paint');
    assert.equal(a.resourceUri, uri);
    assert.deepEqual(
      a.appTools.map((tool) => tool.name),
      ['save', 'query'],
    );
    assert.equal(a.tools[2].annotations?.readOnlyHint, true);
    const [resource, same] = await Promise.all([f.session.readApp(), f.session.readApp()]);
    assert.equal(resource, same);
    assert.deepEqual(resource.csp.connectDomains, ['https://api.example.com']);
    assert.deepEqual(resource.csp.resourceDomains, ['https://assets.example.com']);
    assert.deepEqual(resource.permissions, { clipboardWrite: {} });
    assert.match(resource.sha256, /^[a-f0-9]{64}$/);
    assert.equal(resource.bytes, new TextEncoder().encode(resource.html).length);
    assert.equal(f.requests.filter((request) => request.method === 'initialize').length, 1);
    assert.equal(f.requests.filter((request) => request.method === 'resources/read').length, 1);
    assert.equal(f.requests.filter((request) => request.method === 'tools/call').length, 0);
    for (const request of f.requests) {
      assert.equal(request.credentials, 'omit');
      assert.equal(request.redirect, 'error');
    }
    const input = f.requests.find((request) => request.method === 'initialize')?.params;
    assert.ok(input && !('members' in input) && !('history' in input) && !('identityId' in input));
    assert.throws(() => {
      (a.tools as Tool[]).push(tools[0]);
    });
  } finally {
    await f.session.close();
  }
});

test('official SDK reads Expo streaming responses with the installed React Native Response constructor', async () => {
  // Expo SDK 58 installs streaming fetch, while RN retains whatwg-fetch.Response.
  // That constructor turns a ReadableStream body into "[object ReadableStream]".
  const { Response: NativeResponse } = createRequire(import.meta.url)('whatwg-fetch') as {
    Response: typeof Response;
  };
  const previous = globalThis.Response;
  globalThis.Response = NativeResponse;
  try {
    for (const sse of [false, true]) {
      const f = fixture({ sse });
      try {
        const descriptor = await f.session.connect();
        assert.equal(descriptor.mainToolName, 'paint');
        assert.equal((await f.session.readApp()).html, '<!doctype html><p>Provider app</p>');
        assert.equal((await f.session.call('save', {})).content[0].type, 'text');
      } finally {
        await f.session.close();
      }
    }
  } finally {
    globalThis.Response = previous;
  }
});

test('host can launch its model-only UI tool while iframe calls stay within app-visible server tools', async () => {
  const f = fixture();
  try {
    await f.session.connect();
    await assert.rejects(f.session.call('paint', {}), /not available/);
    await assert.rejects(f.session.call('foreign-server-tool', {}), /not available/);
    await assert.rejects(f.session.call('save', {}, { origin: 'host' }), /not available/);
    await f.session.call('paint', {}, { origin: 'host' });
    await f.session.call('save', { text: 'Synthetic only' });
    assert.deepEqual(
      f.requests.filter((r) => r.method === 'tools/call').map((r) => r.params?.name),
      ['paint', 'save'],
    );
  } finally {
    await f.session.close();
  }
});

test('owner switch rejects a late resource and cannot revive the session', async () => {
  const gate = deferred<void>();
  const f = fixture({ readWait: gate.promise });
  await f.session.connect();
  const pending = f.session.readApp();
  await new Promise<void>((resolve) => setImmediate(resolve));
  f.retireOwner();
  gate.resolve();
  await assert.rejects(pending, /Account changed|closed|abort/i);
  assert.throws(() => f.session.connect(), /closed/);
  await f.session.close();
});

test('explicit close cancels late tool completion and is idempotent', async () => {
  const gate = deferred<void>();
  const f = fixture({ callWait: gate.promise });
  await f.session.connect();
  const pending = f.session.call('save', {});
  await new Promise<void>((resolve) => setImmediate(resolve));
  await f.session.close();
  gate.resolve();
  await assert.rejects(pending, /closed|abort/i);
  await f.session.close();
  assert.throws(() => f.session.readApp(), /closed/);
});

test('resource identity, MIME and complete size must match actual declared content', async () => {
  for (const resource of [
    { uri: 'ui://foreign/app' },
    { mimeType: 'text/html' },
    { text: 'x'.repeat(8 * 1024 * 1024 + 1) },
  ]) {
    const f = fixture({ resource });
    try {
      await assert.rejects(f.session.readApp(), /resource|limit/i);
    } finally {
      await f.session.close();
    }
  }
});

test('unsupported authentication fails honestly without credentials or browser flow', async () => {
  const f = fixture({ auth: true });
  await assert.rejects(f.session.connect(), /authorization|sign-in/i);
  assert.throws(() => f.session.connect(), /closed/);
});

test('standard resource policy accepts only bounded exact HTTPS origins and no raw directives', () => {
  for (const domain of [
    'https://*.example.com',
    'http://api.example.com',
    'https://user:password@api.example.com',
    'https://api.example.com/path',
    'https://api.example.com; script-src *',
    'https://127.0.0.1',
  ]) {
    assert.throws(() => parseRemoteResourcePolicy({ ui: { csp: { connectDomains: [domain] } } }));
  }
  assert.throws(() =>
    parseRemoteResourcePolicy({
      ui: { csp: { connectDomains: Array(21).fill('https://api.example.com') } },
    }),
  );
  assert.throws(() =>
    parseRemoteResourcePolicy({ ui: { permissions: { camera: { arbitrary: true } } } }),
  );
  assert.deepEqual(parseRemoteResourcePolicy(undefined).csp.connectDomains, []);
});

test('duplicate tool descriptors and oversized arguments fail before external execution', async () => {
  const duplicate = fixture({ toolList: [tools[0], tools[0]] });
  await assert.rejects(duplicate.session.connect(), /duplicate/);
  const f = fixture();
  try {
    await f.session.connect();
    await assert.rejects(f.session.call('save', { text: 'x'.repeat(1_000_001) }), /limit/);
    assert.equal(f.requests.filter((request) => request.method === 'tools/call').length, 0);
  } finally {
    await f.session.close();
  }
});
