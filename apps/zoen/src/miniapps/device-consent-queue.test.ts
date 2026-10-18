import assert from 'node:assert/strict';
import test from 'node:test';
import {
  createDeviceConsentQueue,
  type ConsentChoice,
  type DeviceConsentQueue,
} from './device-consent-queue.ts';

function shown(queue: DeviceConsentQueue) {
  const current = queue.getSnapshot().current;
  assert.ok(current, 'a real presentation must exist');
  return current;
}
const turn = () => Promise.resolve();

test('a choice cannot grant device access or promote another request before actual native dismissal', async () => {
  const queue = createDeviceConsentQueue();
  const completed: [string, ConsentChoice][] = [];
  const calendar = queue.ask('calendar.freebusy', 'Read busy intervals');
  const first = shown(queue);
  const camera = queue.ask('camera.capture', 'Take a photo');
  void calendar.then((choice) => completed.push(['calendar', choice]));
  void camera.then((choice) => completed.push(['camera', choice]));

  assert.equal(queue.choose(first.id, 'once'), true);
  assert.equal(queue.getSnapshot().phase, 'dismissing');
  assert.equal(shown(queue), first);
  await turn();
  assert.deepEqual(completed, [], 'EventKit/camera cannot start behind the closing consent sheet');
  assert.equal(queue.choose(first.id, 'always'), false);
  assert.equal(
    queue.choose(first.id, 'deny'),
    false,
    'programmatic presentation=false is not a new choice',
  );
  assert.equal(queue.dismissed(first.id + 1000), false);
  assert.equal(queue.dismissed(first.id), true);
  assert.equal(await calendar, 'once');

  const second = shown(queue);
  assert.equal(second.capability, 'camera.capture');
  assert.notEqual(second.id, first.id);
  assert.equal(queue.dismissed(first.id), false, 'a duplicate native callback cannot drain camera');
  assert.equal(queue.choose(first.id, 'always'), false);
  await turn();
  assert.deepEqual(completed, [['calendar', 'once']]);
  queue.choose(second.id, 'deny');
  queue.dismissed(second.id);
  assert.equal(await camera, 'deny');
  assert.deepEqual(completed, [
    ['calendar', 'once'],
    ['camera', 'deny'],
  ]);
  assert.deepEqual(queue.getSnapshot(), { current: null, phase: 'idle' });
});

for (const choice of ['once', 'always', 'deny'] as const) {
  test(`the first ${choice} choice wins repeated taps and preserves the permission duration`, async () => {
    const queue = createDeviceConsentQueue();
    const response = queue.ask('calendar.freebusy', 'Read busy intervals');
    const request = shown(queue);
    assert.equal(queue.choose(request.id, choice), true);
    for (const duplicate of ['once', 'always', 'deny'] as const)
      assert.equal(queue.choose(request.id, duplicate), false);
    queue.dismissed(request.id);
    assert.equal(await response, choice);
    assert.equal(queue.dismissed(request.id), false);
  });
}

test('native swipe, Back or presentation cancellation without a button choice denies only that request', async () => {
  const queue = createDeviceConsentQueue();
  const first = queue.ask('photos.pick', 'Choose a photo');
  const request = shown(queue);
  const second = queue.ask('calendar.freebusy', 'Read busy intervals');
  assert.equal(queue.dismissed(request.id), true);
  assert.equal(await first, 'deny');
  const next = shown(queue);
  assert.equal(next.capability, 'calendar.freebusy');
  queue.choose(next.id, 'always');
  queue.dismissed(next.id);
  assert.equal(await second, 'always');
});

test('background revokes a staged grant and all queued requests while preserving the closing-sheet barrier', async () => {
  const queue = createDeviceConsentQueue();
  const first = queue.ask('calendar.freebusy', 'Read busy intervals');
  const request = shown(queue);
  const second = queue.ask('camera.capture', 'Take a photo');
  queue.choose(request.id, 'always');
  queue.cancelAll();
  assert.deepEqual(await Promise.all([first, second]), ['deny', 'deny']);
  assert.equal(shown(queue), request);
  assert.equal(queue.getSnapshot().phase, 'dismissing');

  const foreground = queue.ask('contacts.pick', 'Choose a contact');
  let settled = false;
  void foreground.then(() => (settled = true));
  assert.equal(
    shown(queue),
    request,
    'a new foreground request waits for the old native completion',
  );
  assert.equal(queue.choose(request.id, 'once'), false);
  await turn();
  assert.equal(settled, false);
  queue.dismissed(request.id);
  const next = shown(queue);
  assert.equal(next.capability, 'contacts.pick');
  assert.equal(queue.dismissed(request.id), false);
  assert.equal(queue.choose(request.id, 'always'), false);
  assert.equal(shown(queue), next);
  queue.choose(next.id, 'once');
  queue.dismissed(next.id);
  assert.equal(await foreground, 'once');
});

test('repeated background or host cancellation drains new queued work without promoting it', async () => {
  const queue = createDeviceConsentQueue();
  const first = queue.ask('photos.pick', 'Choose a photo');
  const request = shown(queue);
  queue.cancelAll();
  assert.equal(await first, 'deny');
  const later = queue.ask('calendar.freebusy', 'Read busy intervals');
  queue.cancelAll();
  queue.cancelAll();
  assert.equal(await later, 'deny');
  assert.equal(shown(queue), request);
  queue.dismissed(request.id);
  assert.equal(queue.getSnapshot().phase, 'idle');
  const fresh = queue.ask('camera.capture', 'Take a photo');
  const next = shown(queue);
  queue.dismissed(next.id);
  assert.equal(await fresh, 'deny');
});

test('route, frame or account disposal denies everything immediately and never presents queued requests', async () => {
  const queue = createDeviceConsentQueue();
  const visible: string[] = [];
  queue.subscribe(() => {
    const current = queue.getSnapshot().current;
    if (current) visible.push(current.capability);
  });
  const first = queue.ask('calendar.freebusy', 'Read busy intervals');
  const request = shown(queue);
  const second = queue.ask('camera.capture', 'Take a photo');
  queue.choose(request.id, 'once');
  queue.dispose();
  assert.deepEqual(await Promise.all([first, second]), ['deny', 'deny']);
  assert.deepEqual(queue.getSnapshot(), { current: null, phase: 'idle' });
  assert.equal(visible.includes('camera.capture'), false);
  assert.equal(queue.choose(request.id, 'always'), false);
  assert.equal(queue.dismissed(request.id), false);
  const late = queue.ask('contacts.pick', 'Choose a contact');
  assert.deepEqual(queue.getSnapshot(), { current: null, phase: 'idle' });
  assert.equal(await late, 'deny');
  queue.cancelAll();
  queue.dispose();
  assert.equal(queue.getSnapshot().phase, 'idle');
});

test('callbacks from a retired owner cannot match a new controller presentation', async () => {
  const old = createDeviceConsentQueue();
  const first = old.ask('calendar.freebusy', 'Read busy intervals');
  const stale = shown(old);
  old.dispose();
  assert.equal(await first, 'deny');
  const current = createDeviceConsentQueue();
  const next = current.ask('camera.capture', 'Take a photo');
  const request = shown(current);
  assert.notEqual(request.id, stale.id);
  assert.equal(current.choose(stale.id, 'always'), false);
  assert.equal(current.dismissed(stale.id), false);
  assert.equal(shown(current), request);
  current.dismissed(request.id);
  assert.equal(await next, 'deny');
});

test('subscriptions expose stable immutable snapshots and stale callbacks do not republish', async () => {
  const queue = createDeviceConsentQueue();
  let changes = 0;
  const unsubscribe = queue.subscribe(() => ++changes);
  const first = queue.ask('calendar.freebusy', 'Read busy intervals');
  const snapshot = queue.getSnapshot();
  const request = shown(queue);
  assert.ok(Object.isFrozen(snapshot));
  assert.ok(Object.isFrozen(request));
  const second = queue.ask('camera.capture', 'Take a photo');
  assert.equal(
    queue.getSnapshot(),
    snapshot,
    'queuing hidden work does not rebuild the native sheet',
  );
  assert.equal(changes, 1);
  assert.equal(queue.choose(NaN, 'always'), false);
  assert.equal(queue.dismissed(request.id + 1), false);
  assert.equal(queue.getSnapshot(), snapshot);
  queue.choose(request.id, 'once');
  const closing = queue.getSnapshot();
  assert.equal(snapshot.phase, 'presented');
  assert.equal(queue.choose(request.id, 'always'), false);
  assert.equal(queue.getSnapshot(), closing);
  assert.equal(changes, 2);
  unsubscribe();
  queue.dispose();
  assert.equal(changes, 2);
  assert.deepEqual(await Promise.all([first, second]), ['deny', 'deny']);
});

test('grant completion can request another capability without skipping a previously queued presentation', async () => {
  const queue = createDeviceConsentQueue();
  const first = queue.ask('calendar.freebusy', 'Read busy intervals');
  const request = shown(queue);
  const second = queue.ask('camera.capture', 'Take a photo');
  const afterGrant = first.then(() => queue.ask('contacts.pick', 'Choose a contact'));
  queue.choose(request.id, 'once');
  queue.dismissed(request.id);
  await turn();
  const camera = shown(queue);
  assert.equal(camera.capability, 'camera.capture');
  queue.dismissed(camera.id);
  assert.equal(await second, 'deny');
  const contact = shown(queue);
  assert.equal(contact.capability, 'contacts.pick');
  queue.choose(contact.id, 'always');
  queue.dismissed(contact.id);
  assert.equal(await afterGrant, 'always');
});

test('a canceled empty queue is reusable and has no artificial dismissal requirement', async () => {
  const queue = createDeviceConsentQueue();
  const before = queue.getSnapshot();
  queue.cancelAll();
  assert.equal(queue.getSnapshot(), before);
  const response = queue.ask('calendar.freebusy', 'Read busy intervals');
  queue.dismissed(shown(queue).id);
  assert.equal(await response, 'deny');
});
