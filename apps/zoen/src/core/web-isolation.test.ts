import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const { webIsolationHeaders } = createRequire(import.meta.url)(
  '../../config/web-isolation.cjs',
) as {
  webIsolationHeaders(url: string): Record<string, string>;
};

test('only the standalone sign-in document allows provider popups', () => {
  for (const path of ['/sign-in', '/sign-in/', '/sign-in.html', '/sign-in?return=%2Fchat']) {
    assert.deepEqual(webIsolationHeaders(path), {
      'Cross-Origin-Opener-Policy': 'same-origin-allow-popups',
      'Cross-Origin-Embedder-Policy': 'unsafe-none',
    });
  }
});

test('message, worker and lookalike routes retain the isolated MLS context', () => {
  for (const path of [
    '/',
    '/chat/123',
    '/worker.js',
    '/sign-in-other',
    '/sign-in/chat',
    '/%73ign-in',
  ]) {
    assert.deepEqual(webIsolationHeaders(path), {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    });
  }
});
