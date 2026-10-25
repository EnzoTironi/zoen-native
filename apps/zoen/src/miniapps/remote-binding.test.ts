import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import type { Tool } from '@modelcontextprotocol/client';
import { resolveRemoteApp, type RemoteAppBinding } from './remote-binding.ts';
import { deferred, fixture, tools, uri } from './remote-session.fixture.ts';

const html = '<!doctype html><p>Provider app</p>';
const endpoint = 'https://apps.example.com/mcp';
function binding(): RemoteAppBinding {
  return {
    id: 'external-editor',
    name: 'External editor',
    endpoint,
    release: '2026.10.25',
    launchTool: 'paint',
    resource: {
      uri,
      sha256: createHash('sha256').update(html).digest('hex'),
      bytes: Buffer.byteLength(html),
      csp: {
        connectDomains: ['https://api.example.com'],
        resourceDomains: ['https://assets.example.com'],
        frameDomains: [],
        baseUriDomains: [],
      },
      permissions: { clipboardWrite: {} },
    },
  };
}

test('a host-owned external release uses the official SDK and exact launch tool, not discovery order', async () => {
  const decoy: Tool = { ...tools[0], name: 'other-editor' };
  const f = fixture({ selection: binding(), endpoint, toolList: [decoy, ...tools] });
  try {
    const descriptor = await f.session.connect();
    assert.equal(descriptor.mainToolName, 'paint');
    assert.equal(descriptor.server.id, 'external-editor');
    assert.equal(descriptor.server.release, '2026.10.25');
    assert.equal(descriptor.server.endpoint, endpoint);
    assert.equal(descriptor.server.name, 'External editor');
    // Even host launch must admit this release's exact UI bytes and policy first.
    await f.session.call('paint', { document: 'synthetic' }, { origin: 'host' });
    await f.session.call('save', { document: 'synthetic' });
    assert.deepEqual(
      f.requests
        .filter((r) => ['resources/read', 'tools/call'].includes(r.method))
        .map((r) => r.method),
      ['resources/read', 'tools/call', 'tools/call'],
    );
    assert.equal((await f.session.readApp()).html, html);
    await assert.rejects(f.session.call('other-editor', {}, { origin: 'host' }), /not available/);
    await assert.rejects(f.session.call('paint', {}), /not available/);
  } finally {
    await f.session.close();
  }
});

test('legacy Excalidraw proof viewer goes through the same resolver without claiming a pinned release', async () => {
  const legacy = resolveRemoteApp('excalidraw');
  assert.equal(legacy.endpoint, 'https://mcp.excalidraw.com/mcp');
  assert.equal(legacy.binding, undefined);
  const f = fixture();
  try {
    assert.equal((await f.session.connect()).server.release, undefined);
    assert.equal((await f.session.readApp()).html, html);
  } finally {
    await f.session.close();
  }
});

test('captured release policy is detached from mutable host input', async () => {
  const input = structuredClone(binding());
  const f = fixture({ selection: input, endpoint });
  // Mutating a caller's configuration after session creation cannot authorize new bytes.
  (input as { endpoint: string }).endpoint = 'https://changed.example.com/mcp';
  (input.resource as { sha256: string }).sha256 = '0'.repeat(64);
  (input.resource.csp.connectDomains as string[]).push('https://untrusted.example.com');
  (input.resource.permissions as Record<string, object>).camera = {};
  try {
    const resource = await f.session.readApp();
    assert.equal(resource.sha256, binding().resource.sha256);
    assert.deepEqual(resource.permissions, { clipboardWrite: {} });
    assert.deepEqual(resource.csp.connectDomains, ['https://api.example.com']);
    assert.ok(Object.isFrozen(resource.csp.connectDomains));
  } finally {
    await f.session.close();
  }
});

test('missing, app-only, and changed-URI launch descriptors refuse before any tool executes', async () => {
  for (const toolList of [
    tools.filter((tool) => tool.name !== 'paint'),
    [{ ...tools[0], _meta: { ui: { resourceUri: uri, visibility: ['app'] } } }],
    [{ ...tools[0], _meta: { ui: { resourceUri: 'ui://provider/changed.html' } } }],
  ]) {
    const f = fixture({ selection: binding(), endpoint, toolList });
    try {
      await assert.rejects(
        f.session.call('paint', {}, { origin: 'host' }),
        /selected|resource changed/,
      );
      assert.equal(f.requests.filter((r) => r.method === 'tools/call').length, 0);
      assert.equal(f.requests.filter((r) => r.method === 'resources/read').length, 0);
    } finally {
      await f.session.close();
    }
  }
});

test('release hash, byte count, CSP and permissions refuse drift before HTML or tools reach consumers', async () => {
  const baseMeta = {
    ui: {
      csp: {
        connectDomains: ['https://api.example.com'],
        resourceDomains: ['https://assets.example.com'],
      },
      permissions: { clipboardWrite: {} },
    },
  };
  const cases = [
    { resource: { text: html.replace('Provider', 'Modified') } },
    {
      selection: {
        ...binding(),
        resource: { ...binding().resource, bytes: Buffer.byteLength(html) + 1 },
      },
    },
    {
      resource: {
        _meta: { ui: { ...baseMeta.ui, csp: { connectDomains: ['https://new.example.com'] } } },
      },
    },
    { resource: { _meta: { ui: { ...baseMeta.ui, permissions: { camera: {} } } } } },
    { resource: { _meta: { ui: { ...baseMeta.ui, permissions: {} } } } },
    { resource: { _meta: { ui: { ...baseMeta.ui, permissions: { futurePermission: {} } } } } },
    {
      resource: {
        _meta: { ui: { ...baseMeta.ui, csp: { ...baseMeta.ui.csp, futureDirective: [] } } },
      },
    },
  ];
  for (const changed of cases) {
    const f = fixture({ selection: binding(), endpoint, ...changed });
    try {
      await assert.rejects(f.session.readApp(), /changed|Unsupported/);
      await assert.rejects(f.session.call('paint', {}, { origin: 'host' }), /changed|Unsupported/);
      assert.equal(f.requests.filter((r) => r.method === 'tools/call').length, 0);
      assert.equal(f.requests.filter((r) => r.method === 'resources/read').length, 1);
    } finally {
      await f.session.close();
    }
  }
});

test('owner retirement during bound resource admission cannot publish HTML or execute launch', async () => {
  const gate = deferred<void>();
  const f = fixture({ selection: binding(), endpoint, readWait: gate.promise });
  const pending = f.session.call('paint', {}, { origin: 'host' });
  await f.session.connect();
  await new Promise<void>((resolve) => setImmediate(resolve));
  f.retireOwner();
  gate.resolve();
  await assert.rejects(pending, /Account changed|closed|abort/i);
  assert.equal(f.requests.filter((r) => r.method === 'tools/call').length, 0);
  await f.session.close();
});

test('malformed trusted bindings are refused synchronously before any transport', () => {
  for (const changed of [
    { endpoint: 'http://apps.example.com/mcp' },
    { endpoint: 'https://user:secret@apps.example.com/mcp' },
    { endpoint: 'https://127.0.0.1/mcp' },
    { endpoint: `${endpoint}?token=secret` },
    { endpoint: `${endpoint}#fragment` },
    { release: '' },
    { name: 'Invalid\nname' },
    { launchTool: 'paint\n' },
    { resource: { ...binding().resource, sha256: 'wrong' } },
    { resource: { ...binding().resource, bytes: 0 } },
    { resource: { ...binding().resource, bytes: 8 * 1024 * 1024 + 1 } },
    { resource: { ...binding().resource, uri: 'https://apps.example.com/app.html' } },
    { resource: { ...binding().resource, permissions: { arbitrary: {} } } },
    { extra: 'unreviewed-field' },
  ]) {
    assert.throws(() => resolveRemoteApp({ ...binding(), ...changed } as RemoteAppBinding));
  }
});
