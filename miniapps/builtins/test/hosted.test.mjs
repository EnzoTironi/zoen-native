import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';
import '../../../apps/zoen/src/i18n/test-setup.ts';
import { MiniappSession } from '../../../apps/zoen/src/miniapps/protocol.ts';
import { JSDOM, VirtualConsole } from '../../../packages/zoen-ui/node_modules/jsdom/lib/api.js';
import { buildApp } from '../../../tools/miniapp-build/build.mjs';
import { fileURLToPath } from 'node:url';
const source = fileURLToPath(new URL('..', import.meta.url));
const tick = () => new Promise((resolve) => setImmediate(resolve));
const held = () => {
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
async function fixture(id, initial) {
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
const pet = {
  name: 'QA donkey',
  fullness: 72,
  energy: 72,
  joy: 76,
  mood: 'Curious',
  asleep: false,
  released: false,
  log: [],
  dash: { runs: 0, best: {} },
};
test('Pet uses actual official SDK/UI: canonical ACK, single-flight, asleep/peer updates and denied rename retention', async () => {
  const f = await fixture('pet', pet);
  try {
    const gate = held(),
      fed = { ...pet, fullness: 94 };
    f.expect('pet_feed', {}, fed, { gate });
    f.button('Feed a carrot').click();
    f.button('Feed a carrot').click();
    await f.wait(() => f.calls.length === 1);
    assert.ok(f.text().includes('72'));
    assert.equal(f.text().includes('94'), false);
    gate.resolve();
    await f.wait(() => f.text().includes('94'));
    const sleeping = { ...fed, asleep: true };
    f.sync(sleeping);
    await f.wait(() => f.button('Feed a carrot').disabled);
    assert.equal(f.button('Give affection').disabled, true);
    assert.equal(f.button('Wake up').disabled, false);
    f.expect('pet_rename', { name: 'Still mine' }, sleeping, { error: 'QA rename denied' });
    f.input('Still mine');
    await tick();
    f.button('Rename').click();
    await f.wait(() => f.text().includes('QA rename denied'));
    assert.equal(
      f.dom.window.document.querySelector('input:not([type=checkbox])').value,
      'Still mine',
    );
    assert.equal(f.messages.filter((m) => m.method === 'ui/initialize').length, 1);
    assert.equal(f.calls.length, 2);
  } finally {
    await f.close();
  }
});
test('Pet preserves its local run across tabs, pauses on leaving Play and keeps canonical leaderboard updates', async () => {
  const f = await fixture('pet', pet);
  try {
    const document = f.dom.window.document;
    const panel = (id) => document.getElementById(`pet-panel-${id}`);
    assert.equal(panel('care').hidden, false);
    assert.equal(panel('play').hidden, true);
    f.button('Donkey Dash').click();
    await f.wait(() => panel('play').hidden === false);
    f.button('Start run').click();
    await f.wait(
      () => !![...document.querySelectorAll('button')].find((b) => b.textContent === 'Pause'),
    );
    f.button('Care').click();
    await f.wait(
      () =>
        panel('play').hidden &&
        !![...document.querySelectorAll('button')].find((b) => b.textContent === 'Resume'),
    );
    f.button('Play').click();
    await f.wait(() => !panel('play').hidden);
    assert.ok(f.button('Resume'));
    assert.equal(f.calls.length, 0, 'private tab/game navigation must not publish a score');
    f.button('Leaderboard').click();
    await f.wait(() => !panel('board').hidden);
    const ranked = { ...pet, dash: { runs: 1, best: { 'QA peer': { meters: 120, carrots: 4 } } } };
    f.sync(ranked);
    await f.wait(() => panel('board').textContent.includes('120 m'));
    assert.ok(panel('board').textContent.includes('QA peer'));
    const tab = document.getElementById('pet-tab-board');
    tab.dispatchEvent(new f.dom.window.KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    await f.wait(() => panel('care').hidden === false);
    assert.equal(document.activeElement.id, 'pet-tab-care');
    assert.equal(f.calls.length, 0);
  } finally {
    await f.close();
  }
});
const poll = {
  question: 'Which path?',
  options: [
    { id: 'o1', text: 'Sea', votes: [] },
    { id: 'o2', text: 'Forest', votes: [] },
  ],
  closed: false,
  winner: null,
  log: [],
};
test('Poll posts exact vote/add/close tools and renders peer canonical results instead of a local voting model', async () => {
  const f = await fixture('poll', poll);
  try {
    assert.equal(f.button('Close and add to plan').disabled, true);
    const voted = {
      ...poll,
      options: [{ ...poll.options[0], votes: ['QA owner'] }, poll.options[1]],
    };
    f.expect('poll_vote', { option: 'o1' }, voted);
    const option = [...f.dom.window.document.querySelectorAll('.poll-option')][0];
    option.click();
    await f.wait(() => f.button('Close and add to plan').disabled === false);
    assert.equal(
      f.dom.window.document.querySelector('.poll-option').getAttribute('aria-pressed'),
      'true',
    );
    f.expect('poll_add_option', { text: 'Hill' }, voted, { error: 'QA add denied' });
    f.input('Hill');
    await tick();
    f.button('Add').click();
    await f.wait(() => f.text().includes('QA add denied'));
    assert.equal(f.dom.window.document.querySelector('input:not([type=checkbox])').value, 'Hill');
    const closed = { ...voted, closed: true, winner: 'Sea' };
    f.expect('poll_close', {}, closed);
    f.button('Close and add to plan').click();
    await f.wait(() => f.text().includes('Voting closed'));
    assert.ok([...f.dom.window.document.querySelectorAll('.poll-option')].every((b) => b.disabled));
  } finally {
    await f.close();
  }
});
const list = {
  title: 'Packing',
  items: [{ id: 'l1', text: 'Water', done: false, by: null }],
  log: [],
};
test('List accepts Enter through MCP without submitting a sandboxed HTML form or committing composed input', async () => {
  const f = await fixture('list', list);
  try {
    f.input('Hat');
    await tick();
    const input = f.dom.window.document.querySelector('input:not([type=checkbox])');
    input.dispatchEvent(
      new f.dom.window.KeyboardEvent('keydown', {
        key: 'Enter',
        bubbles: true,
        cancelable: true,
        isComposing: true,
      }),
    );
    await tick();
    assert.equal(f.calls.length, 0);
    f.expect(
      'list_add',
      { text: 'Hat' },
      {
        ...list,
        items: [...list.items, { id: 'l2', text: 'Hat', done: false, by: null }],
      },
    );
    input.dispatchEvent(
      new f.dom.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    );
    await f.wait(() => input.value === '');
    assert.equal(f.calls.filter((call) => call.name === 'list_add').length, 1);
  } finally {
    await f.close();
  }
});
test('List preserves failed add text, toggles canonical state, and shares through standard MCP without simulated delivery', async () => {
  const f = await fixture('list', list);
  try {
    const done = { ...list, items: [{ ...list.items[0], done: true, by: 'QA owner' }] };
    f.expect('list_toggle', { id: 'l1' }, done);
    f.dom.window.document.querySelector('input[type=checkbox]').click();
    await f.wait(
      () =>
        f.dom.window.document.querySelector('input[type=checkbox]').getAttribute('aria-checked') ===
        'true',
    );
    f.expect('list_add', { text: 'Hat' }, done, { error: 'QA list denied' });
    f.input('Hat');
    await tick();
    f.button('Add').click();
    await f.wait(() => f.text().includes('QA list denied'));
    assert.equal(f.dom.window.document.querySelector('input:not([type=checkbox])').value, 'Hat');
    const added = {
      ...done,
      items: [...done.items, { id: 'l2', text: 'Hat', done: false, by: null }],
    };
    f.expect('list_add', { text: 'Hat' }, added);
    f.button('Add').click();
    await f.wait(
      () => f.dom.window.document.querySelector('input:not([type=checkbox])').value === '',
    );
    f.button('In this chat').click();
    await f.wait(() => f.shares.length === 1);
    assert.equal(f.shares[0], 'Packing\n☑ Water\n☐ Hat');
    f.button('Open in WhatsApp').click();
    await f.wait(() => f.links.length === 1);
    assert.equal(new URL(f.links[0]).hostname, 'wa.me');
    assert.equal(new URL(f.links[0]).searchParams.get('text'), f.shares[0]);
    assert.equal(
      f.calls.some((c) => c.name === 'list_send_whatsapp'),
      false,
    );
  } finally {
    await f.close();
  }
});
const recipe = {
  title: 'QA dinner',
  subtitle: 'Together',
  baseServings: 2,
  servings: 2,
  ingredients: [{ id: 'i1', name: 'Beans', qty: 100, unit: 'g', done: false }],
  steps: [
    { text: 'Boil water', minutes: 2 },
    { text: 'Add beans', minutes: 0 },
  ],
  cooking: null,
  log: [],
};
test('Recipe scales display only and publishes exact checkbox/serving/zero-based completion tools', async () => {
  const f = await fixture('recipe', recipe);
  try {
    const scaled = { ...recipe, servings: 3 };
    f.expect('recipe_servings', { servings: 3 }, scaled);
    f.button('One more serving').click();
    await f.wait(() => f.text().includes('150 g'));
    assert.equal(scaled.ingredients[0].qty, 100);
    const checked = { ...scaled, ingredients: [{ ...scaled.ingredients[0], done: true }] };
    f.expect('recipe_check', { id: 'i1' }, checked);
    f.dom.window.document.querySelector('input[type=checkbox]').click();
    await f.wait(
      () =>
        f.dom.window.document.querySelector('input[type=checkbox]').getAttribute('aria-checked') ===
        'true',
    );
    const started = { ...checked, cooking: { who: 'QA owner', step: 0, at: 1 } };
    f.expect('recipe_cook', { step: 0 }, started);
    f.button('Start cooking').click();
    await f.wait(() => f.text().includes('Boil water'));
    const peerFinished = { ...started, cooking: { who: 'QA peer', step: 2, at: 2 } };
    f.sync(peerFinished);
    await f.wait(() => f.text().includes('Finished cooking') && !f.text().includes('Boil water'));
    assert.equal(
      [...f.dom.window.document.querySelectorAll('button')].some(
        (button) => button.textContent.trim() === 'Finish',
      ),
      false,
    );
    f.sync(started);
    await f.wait(() => f.text().includes('Boil water'));
    const second = { ...started, cooking: { ...started.cooking, step: 1 } };
    f.expect('recipe_cook', { step: 1 }, second);
    f.button('Next').click();
    await f.wait(() => f.text().includes('Add beans'));
    const finished = { ...second, cooking: { ...second.cooking, step: 2 } };
    f.expect('recipe_cook', { step: 2 }, finished);
    f.button('Finish').click();
    await f.wait(() => f.text().includes('Finished cooking'));
  } finally {
    await f.close();
  }
});
const map = {
  places: [
    { name: 'QA destination', country: 'QA country', hint: 'A northern place', lat: 45, lon: 20 },
  ],
  guesses: { identity_a: { 0: { lat: 0, lon: 0, km: 9000, points: 1 } } },
  order: [],
  log: [],
};
test('MapTap uses actor display name, bounded real coordinates and canonical scoring without revealing target first', async () => {
  const f = await fixture('maptap', map);
  try {
    assert.equal(f.button('Lock in guess').disabled, true);
    assert.equal(f.text().includes('45.00°'), false);
    const document = f.dom.window.document;
    const globe = document.querySelector('[data-globe="orthographic"]');
    const land = document.querySelector('[data-land="natural-earth-110m"]');
    assert.ok(land.getAttribute('d').length > 1000, 'render the actual continents');
    assert.equal(document.querySelector('[data-marker="target"]'), null);
    const firstLand = land.getAttribute('d');
    f.button('Rotate east').click();
    await f.wait(() => land.getAttribute('d') !== firstLand);
    globe.getBoundingClientRect = () => ({ left: 0, top: 0, width: 360, height: 360 });
    const pointer = (type, x, y) => {
      const event = new f.dom.window.MouseEvent(type, {
        bubbles: true,
        clientX: x,
        clientY: y,
        button: 0,
      });
      Object.defineProperty(event, 'pointerId', { value: 1 });
      Object.defineProperty(event, 'isPrimary', { value: true });
      globe.dispatchEvent(event);
    };
    const beforeDrag = land.getAttribute('d');
    pointer('pointerdown', 180, 180);
    pointer('pointermove', 250, 200);
    pointer('pointerup', 250, 200);
    await f.wait(() => land.getAttribute('d') !== beforeDrag);
    assert.equal(f.button('Lock in guess').disabled, true, 'rotation must not also choose a guess');
    pointer('pointerdown', 0, 0);
    pointer('pointerup', 0, 0);
    await tick();
    assert.equal(
      f.button('Lock in guess').disabled,
      true,
      'outside-sphere taps are not coordinates',
    );
    pointer('pointerdown', 180, 180);
    pointer('pointerup', 180, 180);
    await f.wait(() => !f.button('Lock in guess').disabled);
    assert.equal(f.calls.length, 0, 'rotation and picking remain private until Lock');
    f.input('100', 0);
    f.input('20', 1);
    await f.wait(() => f.text().includes('Use latitude'));
    assert.equal(f.button('Lock in guess').disabled, true);
    f.input('40', 0);
    f.input('-70', 1);
    await f.wait(() => !f.button('Lock in guess').disabled);
    const accepted = {
      ...map,
      guesses: { ...map.guesses, 'QA owner': { 0: { lat: 40, lon: -70, km: 8100, points: 20 } } },
      order: ['QA owner'],
    };
    f.expect('maptap_guess', { round: 0, lat: 40, lon: -70 }, accepted);
    f.button('Lock in guess').click();
    await f.wait(
      () => f.text().includes('45.00°') && !!document.querySelector('[data-marker="target"]'),
    );
    assert.ok(document.querySelector('[data-result-arc="true"]').getAttribute('d').length > 0);
    assert.ok(f.text().includes('20'));
    assert.equal(
      [...f.dom.window.document.querySelectorAll('button')].some(
        (b) => b.textContent === 'Lock in guess',
      ),
      false,
    );
  } finally {
    await f.close();
  }
});
test('Countdown renders canonical date, follows a peer update and never fabricates tools or writes while ticking', async () => {
  const initial = {
    title: 'QA trip',
    place: 'QA island',
    target: Date.now() + 172_800_000,
    log: [],
  };
  const f = await fixture('countdown', initial);
  try {
    assert.ok(f.dom.window.document.querySelector('time').dateTime);
    assert.ok(f.text().includes('Date and time use this device'));
    f.sync({ ...initial, target: 0 });
    await f.wait(() => f.text().includes('The time has arrived'));
    await tick();
    assert.equal(f.calls.length, 0);
    assert.equal(
      f.messages.some((m) => m.method === 'zoen/native/calendar.add'),
      false,
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
