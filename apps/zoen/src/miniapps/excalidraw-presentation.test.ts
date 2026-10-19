import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { excalidrawPresentation } from './excalidraw-presentation.ts';
import html from './vendor/excalidraw-ui.ts';
import { readFileSync } from 'node:fs';
import type { RemoteAppResource, RemoteMcpDescriptor } from './remote-session.ts';

const manifest = JSON.parse(
  readFileSync(new URL('./vendor/excalidraw-ui.manifest.json', import.meta.url), 'utf8'),
);
const descriptor: RemoteMcpDescriptor = {
  server: {
    id: 'excalidraw',
    name: 'Excalidraw',
    endpoint: 'https://mcp.excalidraw.com/mcp',
    protocolVersion: '2025-11-25',
  },
  tools: [],
  appTools: [],
  mainToolName: 'create_view',
  resourceUri: 'ui://excalidraw/mcp-app.html',
};
const upstream: RemoteAppResource = {
  html: '<p>Actual remote resource</p>',
  mimeType: 'text/html;profile=mcp-app',
  resourceUri: descriptor.resourceUri,
  csp: {
    connectDomains: ['https://esm.sh'],
    resourceDomains: ['https://esm.sh'],
    frameDomains: [],
    baseUriDomains: [],
  },
  permissions: { clipboardWrite: {} },
  sha256: 'remote-resource-sha',
  bytes: 28,
};

test('pinned Excalidraw renderer has truthful integrity and its own resource identity', () => {
  const shown = excalidrawPresentation(descriptor, upstream);
  assert.equal(shown.resourceUri, 'ui://zoen/excalidraw.html');
  assert.equal(shown.html, html);
  assert.equal(shown.bytes, Buffer.byteLength(html));
  assert.equal(shown.sha256, createHash('sha256').update(html).digest('hex'));
  assert.equal(shown.sha256, manifest.sha256);
  assert.deepEqual(shown.permissions, {});
  assert.deepEqual(shown.csp, {
    connectDomains: [],
    resourceDomains: [],
    frameDomains: [],
    baseUriDomains: [],
  });
  assert.equal(upstream.sha256, 'remote-resource-sha');
  assert.equal(upstream.html, '<p>Actual remote resource</p>');
  assert.equal(upstream.resourceUri, descriptor.resourceUri);
  assert.match(manifest.provenance.upstreamCommit, /^[a-f0-9]{40}$/);
  assert.match(manifest.provenance.patchCommit, /^[a-f0-9]{40}$/);
});

test('presentation cannot substitute an unrelated server/resource or widen declared origins', () => {
  assert.throws(() =>
    excalidrawPresentation(
      { ...descriptor, server: { ...descriptor.server, endpoint: 'https://example.com/mcp' } },
      upstream,
    ),
  );
  assert.throws(() => excalidrawPresentation({ ...descriptor, mainToolName: 'other' }, upstream));
  assert.throws(() =>
    excalidrawPresentation({ ...descriptor, resourceUri: 'ui://other' }, upstream),
  );
  assert.throws(() =>
    excalidrawPresentation(descriptor, { ...upstream, resourceUri: 'ui://other' }),
  );
  const offline = excalidrawPresentation(descriptor, {
    ...upstream,
    csp: { connectDomains: [], resourceDomains: [], frameDomains: [], baseUriDomains: [] },
  });
  assert.deepEqual(offline.csp, {
    connectDomains: [],
    resourceDomains: [],
    frameDomains: [],
    baseUriDomains: [],
  });
  const narrower = excalidrawPresentation(descriptor, {
    ...upstream,
    csp: { ...upstream.csp, resourceDomains: ['https://example.com'] },
  });
  assert.deepEqual(narrower.csp, offline.csp);
});
