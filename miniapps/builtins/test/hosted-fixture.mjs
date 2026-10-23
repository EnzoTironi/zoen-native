import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import '../../../apps/zoen/src/i18n/test-setup.ts';
import { MiniappSession } from '../../../apps/zoen/src/miniapps/protocol.ts';
import { JSDOM, VirtualConsole } from '../../../packages/zoen-ui/node_modules/jsdom/lib/api.js';
import { buildApp } from '../../../tools/miniapp-build/build.mjs';
import { fileURLToPath } from 'node:url';
const source = fileURLToPath(new URL('..', import.meta.url));
export const tick = () => new Promise((resolve) => setImmediate(resolve));
export const held = () => {
  let resolve;
  const promise = new Promise((ok) => {
    resolve = ok;
  });
  return { promise, resolve };
};
const tools = {
  pet: [
    'pet_feed',
    'pet_nap',
    'pet_wake',
    'pet_play',
    'pet_rename',
    'pet_dash_score',
    'pet_get_state',
    'pet_release',
  ],
  poll: ['poll_vote', 'poll_add_option', 'poll_close', 'poll_get_state'],
  list: ['list_add', 'list_toggle', 'list_get_state'],
  maptap: ['maptap_guess', 'maptap_get_state'],
  recipe: ['recipe_servings', 'recipe_check', 'recipe_cook', 'recipe_get_state'],
  countdown: ['countdown_get_state'],
};
const members = [
  { id: 'identity_a', name: 'QA owner', initials: 'QA', color: '#557755', isMe: true },
  { id: 'identity_b', name: 'QA peer', initials: 'QB', color: '#775555', isMe: false },
];
export async function fixture(id, initial) {
  const folder = await mkdtemp(join(tmpdir(), 'zoen-builtin-host-'));
  const manifest = await buildApp(source, folder, { manifestPath: `manifests/${id}.json` });
  const html = await readFile(join(folder, `${id}.html`), 'utf8');
  let session,
    dom,
    state = structuredClone(initial);
  const calls = [],
    jobs = [],
    messages = [],
    errors = [],
    shares = [],
    links = [];
  const console = new VirtualConsole();
  console.on('jsdomError', (e) => errors.push(e.message));
  console.on('error', (...v) => errors.push(v.map(String).join(' ')));
  try {
    dom = new JSDOM(html, {
      runScripts: 'dangerously',
      pretendToBeVisual: true,
      virtualConsole: console,
      beforeParse(window) {
        window.TextEncoder = TextEncoder;
        window.TextDecoder = TextDecoder;
        window.matchMedia = () => ({
          matches: false,
          addEventListener() {},
          removeEventListener() {},
        });
        window.ResizeObserver = class {
          observe() {}
          disconnect() {}
        };
        const parent = {
          postMessage(message) {
            messages.push(message);
            void session.receive(JSON.stringify(message));
          },
        };
        const context = () => ({
          locale: 'en',
          theme: 'light',
          displayMode: 'fullscreen',
          zoen: { members },
        });
        session = new MiniappSession({
          send(message) {
            window.dispatchEvent(
              new window.MessageEvent('message', { data: message, source: parent }),
            );
          },
          initialize: () => ({
            hostInfo: { name: 'zoen', version: 'qa' },
            hostCapabilities: {
              serverTools: {},
              message: { text: {} },
              openLinks: {},
              experimental: {
                zoen: { native: [], services: ['state', 'members', 'share', 'theme'] },
              },
            },
            hostContext: context(),
          }),
          state: () => ({ content: [], structuredContent: state }),
          context,
          tools: new Set(tools[id]),
          resourceUri: `ui://builtin/${id}`,
          resource: async () => ({ contents: [] }),
          async call(name, args) {
            calls.push({ name, args });
            const job = jobs.shift();
            assert.ok(job, `Unexpected canonical tool ${name}`);
            assert.equal(name, job.name);
            assert.deepEqual(args, job.args);
            if (job.gate) await job.gate.promise;
            if (job.error) throw Error(job.error);
            state = structuredClone(job.next);
            return { content: [], structuredContent: state };
          },
          share: async (text) => {
            shares.push(text);
            return {};
          },
          openLink: async (url) => {
            links.push(url);
            return {};
          },
          native: async () => {
            throw Error('No native capability declared');
          },
          haptic() {},
          resize() {},
          close() {},
        });
        Object.defineProperty(window, 'parent', { value: parent });
      },
    });
  } catch (e) {
    session?.dispose();
    dom?.window.close();
    await rm(folder, { recursive: true, force: true });
    throw e;
  }
  const wait = async (predicate, label = 'expected UI') => {
    for (let i = 0; i < 200; i++) {
      assert.deepEqual(errors, []);
      if (predicate()) return;
      await new Promise((r) => setTimeout(r, 5));
    }
    throw Error(`Missing ${label}: ${dom.window.document.body.textContent.slice(0, 500)}`);
  };
  const buttons = () => [...dom.window.document.querySelectorAll('button')];
  const button = (label) => {
    const result = buttons().find(
      (b) => (b.getAttribute('aria-label') ?? b.textContent.trim()) === label,
    );
    assert.ok(result, `Missing button ${label}`);
    return result;
  };
  const readyButton = async (label) => {
    await wait(() => !button(label).disabled, `enabled ${label}`);
    return button(label);
  };
  const text = () => dom.window.document.getElementById('root').textContent;
  const input = (value, index = 0) => {
    const field = dom.window.document.querySelectorAll('input:not([type=checkbox])')[index];
    assert.ok(field);
    const set = Object.getOwnPropertyDescriptor(dom.window.HTMLInputElement.prototype, 'value').set;
    set.call(field, value);
    field.dispatchEvent(new dom.window.Event('input', { bubbles: true }));
    return field;
  };
  const expectedTitle = initial.name ?? initial.title ?? initial.question ?? 'MapTap';
  await wait(
    () =>
      dom.window.document.querySelector('h1')?.textContent === expectedTitle &&
      !!dom.window.document.querySelector('section'),
    'canonical first state',
  );
  await tick();
  assert.equal(calls.length, 0, 'opening a view must not call any domain tool');
  assert.equal(shares.length, 0);
  assert.equal(links.length, 0);
  return {
    dom,
    manifest,
    calls,
    messages,
    errors,
    shares,
    links,
    wait,
    text,
    button,
    readyButton,
    input,
    expect(name, args, next, options = {}) {
      jobs.push({ name, args, next, ...options });
    },
    sync(next) {
      state = structuredClone(next);
      session.sync();
    },
    async close() {
      session.dispose();
      dom.window.close();
      await rm(folder, { recursive: true, force: true });
    },
  };
}
