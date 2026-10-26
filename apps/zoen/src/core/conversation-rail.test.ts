import assert from 'node:assert/strict';
import test from 'node:test';
import { conversationRailFixture } from './conversation-rail.fixtures.ts';
import { CONVERSATION_RAIL_GRIP } from './conversation-rail-motion.ts';

test('up/down gestures and the accessible handle share geometry without remounting cards', async () => {
  const f = await conversationRailFixture();
  try {
    const pan = f.model().gesture as any;
    pan.onActivate();
    pan.onUpdate({ translationY: -90 });
    await f.render();
    assert.equal(f.style('conversation-rail').height, 90 + CONVERSATION_RAIL_GRIP);
    assert.equal(f.style('conversation-rail-spacer').height, f.style('conversation-rail').height);
    pan.onDeactivate({ translationY: -90, velocityY: -800, canceled: false });
    await f.flush();
    await f.render();
    assert.equal(f.style('conversation-rail').height, CONVERSATION_RAIL_GRIP);
    assert.equal(f.views.get('conversation-rail-toggle')!.accessibilityState.expanded, false);
    assert.equal(f.views.get('conversation-rail-cards')!.pointerEvents, 'none');
    const down = f.model().gesture as any;
    down.onActivate();
    down.onUpdate({ translationY: 100 });
    down.onDeactivate({ translationY: 100, velocityY: 700, canceled: false });
    await f.flush();
    await f.render();
    assert.equal(f.style('conversation-rail').height, 180 + CONVERSATION_RAIL_GRIP);
    assert.equal(f.views.get('conversation-rail-toggle')!.accessibilityState.expanded, true);
    assert.deepEqual(f.mounts(), { mounts: 1, unmounts: 0 });
  } finally {
    await f.close();
  }
});

test('manual collapse survives keyboard dismissal and canceled drags return to the prior detent', async () => {
  const f = await conversationRailFixture();
  try {
    f.model().toggle();
    await f.flush();
    f.keyboard.set(0.5);
    await f.render();
    assert.equal(f.style('conversation-rail').height, CONVERSATION_RAIL_GRIP / 2);
    assert.equal(f.style('conversation-rail-spacer').height, CONVERSATION_RAIL_GRIP / 2);
    f.keyboard.set(1);
    await f.render();
    assert.equal(f.style('conversation-rail').height, 0);
    f.keyboard.set(0);
    await f.render();
    assert.equal(f.model().collapsed, true);
    const pan = f.model().gesture as any;
    pan.onActivate();
    pan.onUpdate({ translationY: 150 });
    pan.onDeactivate({ translationY: 150, velocityY: 900, canceled: true });
    await f.flush();
    await f.render();
    assert.equal(f.style('conversation-rail').height, CONVERSATION_RAIL_GRIP);
    assert.equal(f.model().collapsed, true);
  } finally {
    await f.close();
  }
});

test('editing, empty rails, reduced motion and retired chat callbacks keep safe boundaries', async () => {
  const f = await conversationRailFixture();
  try {
    assert.equal(f.frames.length, 0, 'no startup animation');
    await f.act(() => f.model().onEditingChange(true));
    assert.equal((f.model().gesture as any).enabled, false);
    f.model().toggle();
    await f.flush();
    assert.equal(f.model().collapsed, false);
    await f.act(() => f.model().onEditingChange(false));
    await f.render({ reduced: true });
    const oldPan = f.model().gesture as any;
    oldPan.onActivate();
    oldPan.onUpdate({ translationY: -140 });
    oldPan.onDeactivate({ velocityY: -900, canceled: false });
    await f.flush();
    assert.equal(f.model().collapsed, true);
    assert.equal(f.frames.length, 0, 'reduced motion never starts a spring');
    await f.render({ scope: 'other-owner:other-chat' });
    oldPan.onUpdate({ translationY: -160 });
    oldPan.onDeactivate({ velocityY: -900, canceled: false });
    await f.flush();
    assert.equal(f.model().collapsed, false);
    assert.equal(f.model().expansion.get(), 1);
    await f.render({ height: 0, collapsible: false });
    assert.equal(f.style('conversation-rail').height, 0);
    assert.equal(f.views.has('conversation-rail-toggle'), false);
  } finally {
    await f.close();
  }
});

test('a retired activation cannot change a replacement chat or steal its active drag', async () => {
  const f = await conversationRailFixture();
  try {
    const retired = f.model().gesture as any;
    await f.render({ scope: 'replacement-owner:replacement-chat' });
    retired.onActivate();
    retired.onUpdate({ translationY: -150 });
    retired.onDeactivate({ velocityY: -900, canceled: false });
    await f.flush();
    assert.equal(f.model().collapsed, false);
    assert.equal(f.model().expansion.get(), 1);
    assert.equal(f.style('conversation-rail').height, 180 + CONVERSATION_RAIL_GRIP);

    const current = f.model().gesture as any;
    current.onActivate();
    current.onUpdate({ translationY: -60 });
    retired.onActivate();
    retired.onFinalize();
    current.onUpdate({ translationY: -140 });
    current.onDeactivate({ velocityY: -900, canceled: false });
    await f.flush();
    assert.equal(f.model().collapsed, true);
    assert.equal(f.model().expansion.get(), 0);

    await f.render({ scope: 'owner:chat' });
    retired.onActivate();
    retired.onUpdate({ translationY: -180 });
    retired.onFinalize();
    await f.flush();
    assert.equal(f.model().collapsed, false);
    assert.equal(f.model().expansion.get(), 1, 'returning to the same scope creates a new owner');
  } finally {
    await f.close();
  }
});
