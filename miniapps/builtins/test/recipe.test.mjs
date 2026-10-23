import assert from 'node:assert/strict';
import test from 'node:test';
import { fixture, held, tick } from './hosted-fixture.mjs';

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
    (await f.readyButton('One more serving')).click();
    await f.wait(() => f.text().includes('150 g'));
    assert.equal(scaled.ingredients[0].qty, 100);
    const checked = { ...scaled, ingredients: [{ ...scaled.ingredients[0], done: true }] };
    f.expect('recipe_check', { id: 'i1' }, checked);
    await f.wait(() => !f.dom.window.document.querySelector('input[type=checkbox]').disabled);
    f.dom.window.document.querySelector('input[type=checkbox]').click();
    await f.wait(
      () =>
        f.dom.window.document.querySelector('input[type=checkbox]').getAttribute('aria-checked') ===
        'true',
    );
    const started = { ...checked, cooking: { who: 'QA owner', step: 0, at: 1 } };
    f.expect('recipe_cook', { step: 0 }, started);
    (await f.readyButton('Start cooking')).click();
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
    (await f.readyButton('Next')).click();
    await f.wait(() => f.text().includes('Add beans'));
    const finished = { ...second, cooking: { ...second.cooking, step: 2 } };
    f.expect('recipe_cook', { step: 2 }, finished);
    (await f.readyButton('Finish')).click();
    await f.wait(() => f.text().includes('Finished cooking'));
  } finally {
    await f.close();
  }
});
test('Recipe waits for canonical acknowledgement before admitting the next step', async () => {
  const started = { ...recipe, cooking: { who: 'QA owner', step: 0, at: 1 } };
  const f = await fixture('recipe', started);
  const gate = held();
  try {
    (await f.readyButton('Continue cooking')).click();
    await f.wait(() => f.text().includes('Boil water'));
    const second = { ...started, cooking: { ...started.cooking, step: 1 } };
    f.expect('recipe_cook', { step: 1 }, second, { gate });
    (await f.readyButton('Next')).click();
    await f.wait(() => f.calls.length === 1);

    // A live canonical update may arrive before the pending tool response.
    f.sync(second);
    await f.wait(() => f.text().includes('Add beans'));
    assert.equal(f.button('Finish').disabled, true);
    assert.equal(f.dom.window.document.querySelector('main').getAttribute('aria-busy'), 'true');
    f.button('Finish').click();
    await tick();
    assert.equal(f.calls.length, 1, 'an in-flight step must not admit a second mutation');
    assert.equal(f.text().includes('Finished cooking'), false);

    gate.resolve();
    const finish = await f.readyButton('Finish');
    const finished = { ...second, cooking: { ...second.cooking, step: 2 } };
    f.expect('recipe_cook', { step: 2 }, finished);
    finish.click();
    await f.wait(() => f.text().includes('Finished cooking'));
    assert.deepEqual(f.calls, [
      { name: 'recipe_cook', args: { step: 1 } },
      { name: 'recipe_cook', args: { step: 2 } },
    ]);
  } finally {
    gate.resolve();
    await f.close();
  }
});
