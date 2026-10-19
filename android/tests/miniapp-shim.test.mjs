// Browser proof of the shipped native transport with the pinned official SDK and
// actual mini-app bundles. This does not replace Android WebView instrumentation.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import '../../apps/zoen/src/i18n/test-setup.ts';
import { MiniappSession } from '../../apps/zoen/src/miniapps/protocol.ts';
import { buildApp } from '../../tools/miniapp-build/build.mjs';
const require = createRequire(new URL('../../scripts/expo-web-tests/package.json', import.meta.url));
const { chromium } = require('playwright-core');
const root = fileURLToPath(new URL('../../', import.meta.url));
const kotlin = await readFile(join(root, 'android/app/src/main/java/xyz/tironi/zoen/miniapps/MiniAppSandbox.kt'), 'utf8');
const shim = kotlin.match(/val shim = """([\s\S]*?)"""\.trimIndent\(\)/)?.[1];
assert.ok(shim, 'Use the exact production Kotlin shim');
const members = [{ id: 'qa', name: 'QA', initials: 'QA', color: '#557755', isMe: true }];
async function fixture(id, { missingSource = false } = {}) {
  const folder = await mkdtemp(join(tmpdir(), 'zoen-native-shim-'));
  const browser = await chromium.launch({ executablePath: process.env.ZOEN_CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true });
  let session;
  try {
    const source = join(root, 'miniapps', id === 'list' ? 'builtins' : 'hike');
    await buildApp(source, folder, id === 'list' ? { manifestPath: 'manifests/list.json' } : {});
    const html = await readFile(join(folder, `${id}.html`), 'utf8');
    const page = await browser.newPage({ viewport: { width: 400, height: 850 }, hasTouch: true, isMobile: true });
    const calls = [], messages = [], errors = [], debug = [];
    let active = true;
    let closeCount = 0;
    let state = id === 'list'
      ? { title: 'Native SDK list', items: [{ id: 'water', text: 'Water', done: false, by: null }], log: [] }
      : { title: 'Native SDK hike', area: 'Bay Area', day: 'Saturday', trails: ['tomales', 'steep', 'lands'].map(id => ({ id, votes: [] })), decided: null, itinerary: null, album: [] };
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'debug') debug.push(message.text()); });
    const context = () => ({ locale: 'en', theme: 'light', displayMode: 'fullscreen', zoen: { members } });
    session = new MiniappSession({
      send(message) {
        if (!active) return;
        void page.evaluate(data => window.zoenMcp.onmessage({ data: JSON.stringify(data) }), message).catch(error => { if (active) errors.push(error.message); });
      },
      initialize: () => ({ hostInfo: { name: 'zoen', version: 'native-shim-proof' },
        hostCapabilities: { serverTools: {}, message: { text: {} }, openLinks: {}, experimental: { zoen: { native: [], services: ['state', 'members', 'share', 'theme'] } } }, hostContext: context() }),
      context, state: () => ({ content: [], structuredContent: state }),
      tools: new Set(id === 'list' ? ['list_toggle', 'list_get_state'] : ['hike_vote', 'hike_get_state']),
      resourceUri: `ui://builtin/${id}`, resource: async () => ({ contents: [] }),
      async call(name, args) {
        calls.push({ name, args });
        if (name === 'list_toggle') state = { ...state, items: state.items.map(item => item.id === args.id ? { ...item, done: !item.done } : item) };
        else if (name === 'hike_vote') state = { ...state, trails: state.trails.map(trail => ({ ...trail, votes: trail.id === args.trail ? ['QA'] : [] })) };
        else throw Error(`Unexpected tool ${name}`);
        return { content: [], structuredContent: state };
      },
      share: async () => ({}), openLink: async () => ({}), native: async () => { throw Error('No native grants'); },
      haptic() {}, resize() {}, close() { closeCount++; },
    });
    await page.exposeFunction('__nativePost', async raw => { messages.push(JSON.parse(raw)); await session.receive(raw); });
    await page.addInitScript(() => {
      window.zoenMcp = { postMessage: raw => { void window.__nativePost(raw); } };
      window.nativeInput = [];
      document.addEventListener('click', event => window.nativeInput.push({ trusted: event.isTrusted, pointer: event.pointerType }), true);
    });
    const actualShim = missingSource ? shim.replace("Object.defineProperty(event, 'source', {value: host});", '') : shim;
    assert.ok(!missingSource || actualShim !== shim, 'Negative control must remove only native source attribution');
    const document = html.replace(/<head(?:\s[^>]*)?>/i, head => `${head}<script>${actualShim}</script>`);
    await page.route('**/*', route => route.request().url() === 'https://miniapp-native-proof.invalid/index.html'
      ? route.fulfill({ status: 200, contentType: 'text/html', body: document }) : route.abort());
    await page.goto('https://miniapp-native-proof.invalid/index.html');
    return { page, calls, messages, errors, debug, get closeCount() { return closeCount; },
      async dispose() { active = false; session.dispose(); await browser.close(); await rm(folder, { recursive: true, force: true }); } };
  } catch (error) { session?.dispose(); await browser.close(); await rm(folder, { recursive: true, force: true }); throw error; }
}

test('the real SDK rejects native events without source and accepts the exact shipped parent attribution', async () => {
  const before = await fixture('list', { missingSource: true });
  try {
    await before.page.waitForFunction(() => Boolean(window.zoenMcp.onmessage));
    for (let attempt = 0; attempt < 100 && !before.debug.some(message => message.includes('Ignoring message from unknown source')); attempt++) await new Promise(resolve => setTimeout(resolve, 10));
    assert.ok(before.debug.some(message => message.includes('Ignoring message from unknown source')));
    assert.ok(before.messages.some(message => message.method === 'ui/initialize'));
    assert.equal(before.messages.some(message => message.method === 'ui/notifications/initialized'), false);
    assert.equal(await before.page.locator('label.item').count(), 0);
  } finally { await before.dispose(); }
  const fixed = await fixture('list');
  try {
    const checkbox = fixed.page.getByRole('checkbox', { name: 'Water', exact: true });
    await checkbox.waitFor(); await checkbox.tap(); await fixed.page.waitForFunction(() => document.querySelector('label.item input').checked);
    assert.deepEqual(fixed.calls, [{ name: 'list_toggle', args: { id: 'water' } }]);
    assert.ok((await fixed.page.evaluate(() => window.nativeInput)).some(event => event.trusted && event.pointer === 'touch'));
    await fixed.page.evaluate(() => window.dispatchEvent(new MessageEvent('message', {
      source: window, data: { jsonrpc: '2.0', method: 'ui/notifications/tool-result', params: { content: [], structuredContent: { title: 'Spoofed', items: [], log: [] } } },
    })));
    assert.equal(await fixed.page.locator('h1').textContent(), 'Native SDK list');
    assert.ok(fixed.debug.some(message => message.includes('Ignoring message from unknown source')));
    assert.deepEqual(fixed.errors, []);
  } finally { await fixed.dispose(); }
});

test('the actual hosted Hike bundle initializes, votes and closes through the same attributed native transport', async () => {
  const f = await fixture('hike');
  try {
    await f.page.locator('.lead > .z-circle').waitFor();
    assert.equal(await f.page.locator('.card').count(), 3);
    await f.page.locator('.card').first().tap();
    await f.page.getByRole('button', { name: 'Vote', exact: true }).tap();
    await f.page.getByRole('button', { name: /^Voted/ }).waitFor();
    assert.deepEqual(f.calls, [{ name: 'hike_vote', args: { trail: 'tomales' } }]);
    await f.page.getByRole('button', { name: 'Back', exact: true }).tap();
    await f.page.locator('.lead > .z-circle').tap();
    for (let attempt = 0; attempt < 100 && !f.closeCount; attempt++) await new Promise(resolve => setTimeout(resolve, 10));
    assert.equal(f.closeCount, 1);
    assert.deepEqual(f.errors, []);
    assert.equal(f.debug.some(message => message.includes('Ignoring message from unknown source')), false);
  } finally { await f.dispose(); }
});
