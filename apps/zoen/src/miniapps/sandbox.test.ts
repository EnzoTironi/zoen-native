import test from 'node:test';
import assert from 'node:assert/strict';
import { sha256 } from '@noble/hashes/sha2.js';
import { miniappCsp, safeMiniappLink, sandboxHtml, verifyMiniappResource } from './sandbox.ts';
import { photoArguments } from './photo-tokens.ts';

test('resource identity, exact bytes and manifest hash are checked before script injection', () => {
  const text = '<html><head><title>Árvore 🪴</title></head><body></body></html>';
  const bytes = new TextEncoder().encode(text);
  const manifest = {
    id: 'hike',
    name: 'Trilha',
    capabilities: [],
    allowedDomains: ['tiles.openfreemap.org'],
    linkDomains: ['www.nps.gov'],
    sha256: Array.from(sha256(bytes), (byte) => byte.toString(16).padStart(2, '0')).join(''),
    bytes: bytes.length,
  };
  const resource = {
    uri: 'ui://roda/hike',
    mime_type: 'text/html;profile=mcp-app',
    text,
    manifest_json: JSON.stringify(manifest),
    prefers_border: false,
  };
  const verified = verifyMiniappResource(resource, 'hike');
  assert.equal(verified?.bytes, bytes.length);
  assert.throws(() => verifyMiniappResource({ ...resource, text: `${text} ` }, 'hike'));
  assert.throws(() => verifyMiniappResource(resource, 'another'));
  assert.throws(() => verifyMiniappResource({ ...resource, manifest_json: '{' }, 'hike'));
  assert.equal(safeMiniappLink('https://www.nps.gov/path', verified), 'https://www.nps.gov/path');
  assert.throws(() => safeMiniappLink('https://evil.example/path', verified));
  assert.throws(() => safeMiniappLink('javascript:alert(1)', null));
  const secured = sandboxHtml(text, [], true);
  assert.ok(secured.indexOf('Content-Security-Policy') < secured.indexOf('<title>'));
});
test('CSP opens only exact declared hosts and rejects directive injection', () => {
  assert.match(miniappCsp([]), /connect-src 'none'/);
  assert.match(
    miniappCsp(['tiles.openfreemap.org']),
    /connect-src https:\/\/tiles.openfreemap.org;/,
  );
  for (const host of [
    '*.example.com',
    'example.com:8080',
    'example.com/path',
    'example.com; script-src *',
    'example.com"',
  ])
    assert.throws(() => miniappCsp([host]));
});
test('photo pixels reach tools only after item-bound token resolution', () => {
  const resolve = (token: string, item: string) =>
    token === 'zoen-photo:one' && item === 'it_one' ? 'data:image/jpeg;base64,AA' : null;
  assert.throws(() => photoArguments({ photos: ['data:image/jpeg;base64,AA'] }, 'it_one', resolve));
  assert.throws(() => photoArguments({ photos: ['zoen-photo:one'] }, 'it_other', resolve));
  assert.deepEqual(photoArguments({ photos: ['zoen-photo:one'] }, 'it_one', resolve).tokens, [
    'zoen-photo:one',
  ]);
  assert.deepEqual(photoArguments({ photos: ['zoen-photo:one'] }, 'it_one', resolve, true).value, {
    photos: ['data:image/jpeg;base64,AA'],
  });
});
