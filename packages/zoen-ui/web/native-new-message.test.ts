import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
const output = resolve(import.meta.dirname, '../node_modules/.cache/new-message-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-new-message.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic', loader: { '.css': 'empty' } });
const { NativeNewMessage, filterNewMessageContacts } = await import(pathToFileURL(output).href);
const contacts = [{ id: 'a', kind: 'person', name: 'Alice', handle: 'alice' }, { id: 'b', kind: 'person', name: 'Bia', handle: 'beatriz' }, { id: 'me', kind: 'person', name: 'Alice Me', isMine: true }, { id: 'bot', kind: 'agent', name: 'Alice Agent' }];
test('search uses provided people, native two-character threshold, name and handle', () => {
  assert.deepEqual(filterNewMessageContacts(contacts, 'a'), []);
  assert.deepEqual(filterNewMessageContacts(contacts, ' @AL ' ).map((p: { id: string }) => p.id), ['a']);
  assert.deepEqual(filterNewMessageContacts(contacts, 'beat').map((p: { id: string }) => p.id), ['b']);
  assert.deepEqual(filterNewMessageContacts(contacts, 'unknown'), []);
});
test('shared sheet exposes native modes without inventing people or offline state', () => {
  const html = renderToStaticMarkup(React.createElement(NativeNewMessage, { contacts: null, onClose() {} }));
  assert.match(html, /role="dialog"/); assert.match(html, /Search by @handle/);
  for (const mode of ['Chat', 'Group', 'Community', 'Join']) assert.match(html, new RegExp('>' + mode + '<'));
  assert.match(html, /People search is unavailable/); assert.doesNotMatch(html, /You’re offline/);
});
test('offline notice requires an actual supplied offline state', () => {
  const html = renderToStaticMarkup(React.createElement(NativeNewMessage, { contacts: [], online: false, onClose() {} }));
  assert.match(html, /You’re offline/); assert.doesNotMatch(html, /People search is unavailable/);
});
