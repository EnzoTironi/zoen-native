import assert from 'node:assert/strict';
import test from 'node:test';
import { fixture, held, tick } from './hosted-fixture.mjs';

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
