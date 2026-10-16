import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
const output = resolve(import.meta.dirname, '../node_modules/.cache/home-notifications-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-widget.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic', loader: { '.css': 'empty' } });
const { NativeHomeStrip } = await import(pathToFileURL(output).href);
test('Notifications is the first home card even at zero; unavailable counts are not fabricated', () => {
  for (const count of [0, 3, null]) {
    const html = renderToStaticMarkup(React.createElement(NativeHomeStrip, { snapshots: [], notificationCount: count, onNotifications() {} }));
    assert.match(html, /<section[^>]*><button[^>]*aria-label="Notifications"/);
    assert.match(html, count === null ? /Unavailable on this client/ : count === 0 ? /View activity/ : /Pending requests/);
    assert.match(html, new RegExp('nw-value">' + (count ?? '—') + '<'));
  }
});
