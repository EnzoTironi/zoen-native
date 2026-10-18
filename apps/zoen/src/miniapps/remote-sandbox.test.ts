import test from 'node:test';
import assert from 'node:assert/strict';
import { remoteAppCsp, remoteAppHtml } from './remote-sandbox.ts';

test('standard resource and connection domains stay in their declared CSP channels', () => {
  const csp = remoteAppCsp({
    connectDomains: ['https://api.excalidraw.com', 'wss://realtime.excalidraw.com'],
    resourceDomains: ['https://*.excalidraw.com', 'https://fonts.gstatic.com/'],
  });
  assert.match(csp, /connect-src https:\/\/api.excalidraw.com wss:\/\/realtime.excalidraw.com;/);
  assert.match(csp, /font-src data: https:\/\/\*.excalidraw.com https:\/\/fonts.gstatic.com;/);
  assert.match(csp, /frame-src 'none';/);
  assert.match(csp, /base-uri 'none';/);
  assert.doesNotMatch(csp.split('script-src')[1]!.split(';')[0]!, /api\.excalidraw/);
  for (const domain of [
    'https://good.com; connect-src *',
    'http://good.com',
    'https://*',
    'https://good.com/path',
    'https://127.0.0.1',
    'https://host.local',
    'https://user:pass@good.com',
    'https://good.com#x',
  ]) {
    assert.throws(() => remoteAppCsp({ resourceDomains: [domain] }));
  }
});

test('provider HTML cannot hide the host policy inside its own commented head', () => {
  const source = '<!-- <head> --><html><head><script>window.example = true;</script></head></html>';
  const secured = remoteAppHtml(source, {});
  assert.ok(secured.indexOf('Content-Security-Policy') < secured.indexOf(source));
  assert.ok(secured.endsWith(source));
  assert.match(secured, /connect-src 'none';/);
});
