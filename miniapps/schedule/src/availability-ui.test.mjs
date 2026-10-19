import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from '../../../tools/miniapp-build/node_modules/esbuild/lib/main.js';
import { JSDOM } from '../../../packages/zoen-ui/node_modules/jsdom/lib/api.js';

// Execute the actual Schedule React tree; only the host SDK, CSS and presentational
// buttons are ports. Calendar responses never pass through a fake Schedule model.
const here = fileURLToPath(new URL('.', import.meta.url));
const bundle = await build({
  entryPoints: [here + 'main.tsx'],
  bundle: true,
  write: false,
  format: 'iife',
  jsx: 'automatic',
  nodePaths: [fileURLToPath(new URL('../../../tools/miniapp-build/node_modules', import.meta.url))],
  define: { 'process.env.NODE_ENV': '"development"' },
  plugins: [
    {
      name: 'host-ports',
      setup(b) {
        b.onResolve({ filter: /^@zoen\/ui$/ }, () => ({ path: 'host', namespace: 'fixture' }));
        b.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({
          resolveDir: here,
          loader: 'tsx',
          contents: `
      import {act,useSyncExternalStore} from 'react';
      globalThis.__act=act;
      export const zoen=globalThis.__zoen;
      export const useZoenState=()=>useSyncExternalStore(globalThis.__subscribe,()=>globalThis.__schedule);
      export const useMembers=()=>[];
      export const Pill=({children,tone,...props})=><button {...props}>{children}</button>;
      export const GlassCircle=({children,label,...props})=><button aria-label={label} {...props}>{children}</button>;
    `,
        }));
        b.onLoad({ filter: /main\.tsx$/ }, async ({ path }) => ({
          loader: 'tsx',
          contents: (await readFile(process.env.SCHEDULE_UI_SOURCE || path, 'utf8')).replace(
            "createRoot(document.getElementById('root')!).render(<App />);",
            "globalThis.__root=createRoot(document.getElementById('root')!);globalThis.__act(()=>globalThis.__root.render(<App />));",
          ),
        }));
        b.onLoad({ filter: /\.css$/ }, () => ({ loader: 'empty', contents: '' }));
      },
    },
  ],
});

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function mount({ delayedHandshake = false, locale = 'en' } = {}) {
  const dom = new JSDOM('<div id="root"></div>', { runScripts: 'outside-only' });
  const w = dom.window;
  w.MessageChannel = class {
    port1 = { onmessage: null };
    port2 = { postMessage: () => setImmediate(() => this.port1.onmessage?.()) };
  };
  w.IS_REACT_ACT_ENVIRONMENT = true;
  w.Date.now = () => Date.parse('2026-10-19T08:00:00Z');
  w.structuredClone = structuredClone;
  const subscriptions = new Set();
  const reads = [],
    writes = [];
  const responses = [];
  const handshake = deferred();
  let calendarAvailable = !delayedHandshake;
  w.__subscribe = (listener) => {
    subscriptions.add(listener);
    return () => subscriptions.delete(listener);
  };
  w.__schedule = {
    title: 'Studio',
    business: 'Studio',
    timezone: 'America/Sao_Paulo',
    currency: 'BRL',
    services: [{ id: 'session', name: 'Session', priceCents: 9000, minutes: 50 }],
    weeklyHours: Object.fromEntries([0, 1, 2, 3, 4, 5, 6].map((d) => [d, [['09:00', '12:00']]])),
    slotMinutes: 30,
    horizonDays: 60,
    configured: true,
    bookings: [],
    events: [],
    busy: [],
    viewer: { id: 'alice', role: 'client' },
    next: 1,
  };
  w.__zoen = {
    locale,
    capabilities: { host: 'ios', has: (cap) => calendarAvailable && cap === 'calendar.freebusy' },
    ready() {
      return delayedHandshake ? handshake.promise : Promise.resolve();
    },
    close() {},
    haptics: { select() {}, tap() {}, success() {}, warning() {} },
    state: {
      call: async (...args) => {
        writes.push(args);
      },
    },
    native: {
      calendar: {
        freeBusy(...args) {
          reads.push(args);
          const reply = deferred();
          responses.push(reply);
          return reply.promise;
        },
      },
    },
  };
  w.eval(bundle.outputFiles[0].text);
  const act = (callback) =>
    w.__act(async () => {
      await callback();
    });
  const buttons = () => [...w.document.querySelectorAll('button')];
  const button = (text) => {
    const found = buttons().find((b) => b.textContent.trim() === text);
    assert.ok(found, `Missing button: ${text}`);
    return found;
  };
  const click = (text) => act(() => button(text).click());
  const slots = () => [...w.document.querySelectorAll('.slot')];
  const update = (change) =>
    act(() => {
      w.__schedule = { ...w.__schedule, ...change };
      for (const listener of subscriptions) listener();
    });
  const dispose = async () => {
    await act(() => w.__root.unmount());
    w.close();
  };
  const finishHandshake = () =>
    act(() => {
      calendarAvailable = true;
      handshake.resolve();
    });
  return {
    w,
    act,
    button,
    click,
    slots,
    reads,
    writes,
    responses,
    update,
    dispose,
    finishHandshake,
  };
}

test('native capability appears after handshake in Portuguese without an automatic read', async () => {
  const app = mount({ delayedHandshake: true, locale: 'pt-BR' });
  try {
    assert.doesNotMatch(app.w.document.body.textContent, /Consultar minha disponibilidade/);
    await app.finishHandshake();
    assert.ok(app.button('Consultar minha disponibilidade'));
    assert.match(app.w.document.body.textContent, /nomes e detalhes não são compartilhados/);
    assert.equal(app.reads.length, 0);
    await app.click('Consultar minha disponibilidade');
    await app.act(() => app.responses[0].reject(new app.w.Error('Nenhum calendário disponível.')));
    assert.match(
      app.w.document.body.textContent,
      /Nenhum filtro foi aplicado\. Nenhum calendário disponível\./,
    );
    assert.ok(app.button('Tentar de novo'));
  } finally {
    await app.dispose();
  }
});

test('actual opt-in UI marks conflicts privately and removal restores business times', async () => {
  const app = mount();
  try {
    const business = structuredClone(app.w.__schedule);
    assert.equal(app.reads.length, 0);
    assert.equal(app.slots().length, 5);
    await app.click('Check my availability');
    assert.equal(app.reads.length, 1);
    assert.ok(app.slots().every((b) => b.disabled));
    const [startMs, endMs] = app.reads[0];
    await app.act(() =>
      app.responses[0].resolve([{ startMs, endMs, title: 'Must never be displayed' }]),
    );
    assert.ok(
      app.slots().every((b) => b.disabled && b.textContent.includes('Busy in my calendar')),
    );
    assert.match(app.w.document.body.textContent, /Business times are available, but all conflict/);
    assert.doesNotMatch(app.w.document.body.textContent, /Must never be displayed/);
    assert.deepEqual(app.w.__schedule, business);
    assert.deepEqual(app.writes, []);
    await app.click('Check again');
    await app.act(() => app.responses[1].resolve([{ startMs, endMs: startMs + 1000 }]));
    assert.doesNotMatch(app.w.document.body.textContent, /Calendar conflicts are marked below/);
    assert.match(app.w.document.body.textContent, /None of these business times conflict/);
    assert.ok(app.slots().every((b) => !b.disabled));
    await app.click('Remove calendar check');
    assert.equal(app.reads.length, 2);
    assert.ok(app.slots().every((b) => !b.disabled));
  } finally {
    await app.dispose();
  }
});

test('actual UI separates denied, empty success, scope changes and late unmount responses', async () => {
  const app = mount();
  try {
    await app.click('Check my availability');
    await app.act(() => app.responses[0].reject(new app.w.Error('Permission denied')));
    assert.match(app.w.document.body.textContent, /No filter is applied\. Permission denied/);
    assert.doesNotMatch(app.w.document.body.textContent, /None of these business times conflict/);
    await app.click('Try again');
    await app.act(() => app.responses[1].resolve([]));
    assert.match(app.w.document.body.textContent, /None of these business times conflict/);
    await app.click('Check again');
    await app.act(() => app.w.document.querySelectorAll('.day')[1].click());
    assert.equal(app.reads.length, 3);
    await app.act(() =>
      app.responses[2].resolve([{ startMs: app.reads[2][0], endMs: app.reads[2][1] }]),
    );
    assert.doesNotMatch(app.w.document.body.textContent, /Calendar conflicts are marked below/);
    await app.update({ timezone: 'Europe/London', viewer: { id: 'bob', role: 'client' } });
    assert.equal(app.reads.length, 3);
    await app.click('Check my availability');
    await app.act(() => app.w.__root.unmount());
    await app.act(() => app.responses[3].reject(new Error('late denial')));
    assert.deepEqual(app.writes, []);
  } finally {
    await app.dispose();
  }
});

test('stale confirmation cannot book after an availability action, while ordinary booking stays canonical', async () => {
  const app = mount();
  try {
    await app.act(() => app.slots()[0].click());
    await app.click('Send request');
    const send = [...app.w.document.querySelectorAll('.sheet button')].find(
      (b) => b.textContent === 'Send request',
    );
    const propsKey = Object.keys(send).find((key) => key.startsWith('__reactProps$'));
    const staleSend = send[propsKey].onClick;
    await app.click('Check my availability');
    await app.act(() => app.responses[0].resolve([]));
    await app.act(() => staleSend());
    assert.deepEqual(app.writes, []);
    await app.act(() => app.slots()[0].click());
    await app.click('Send request');
    await app.act(() =>
      [...app.w.document.querySelectorAll('.sheet button')]
        .find((b) => b.textContent === 'Send request')
        .click(),
    );
    assert.equal(app.writes.length, 1);
    assert.equal(app.writes[0][0], 'schedule_book');
    assert.equal(app.writes[0][1].serviceId, 'session');
    assert.deepEqual(Object.keys(app.writes[0][1]).sort(), ['serviceId', 'startMs']);
  } finally {
    await app.dispose();
  }
});
