import assert from 'node:assert/strict';
import { test } from 'node:test';
import { newChatFixture } from './new-chat-react.fixtures.ts';

test('iOS creation is a header action and submits one trimmed group while a request is pending', async () => {
  const f = await newChatFixture();
  try {
    assert.equal(f.controls.has('createHeader'), false, 'direct picker has no create action');
    await f.choose('group');
    assert.equal(f.control('createHeader').disabled, true);
    assert.equal(
      f.controls.has('button:chat.createGroup'),
      false,
      'creation is outside scrolling form',
    );
    assert.equal(f.control('toolbar').placement, 'right');
    assert.equal(f.control('scroll').automaticallyAdjustKeyboardInsets, true);
    await f.title('   ');
    assert.equal(f.control('createHeader').disabled, true);
    await f.title('  QA Native Links  ');
    assert.equal(f.control('createHeader').disabled, false);
    const press = f.control('createHeader').onPress;
    await f.act(() => {
      press();
      press();
    });
    assert.deepEqual(f.calls, [{ op: 'create_group', title: 'QA Native Links', member_ids: [] }]);
    assert.equal(f.control('createHeader').disabled, true);
    assert.equal(f.control('field:chat.groupName').editable, false);
    await f.resolve('own-qa-group');
    assert.deepEqual(f.routes, [
      'dismiss',
      { pathname: '/chat/[id]', params: { id: 'own-qa-group' } },
    ]);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('community creation keeps its title and exposes an error without automatically retrying', async () => {
  const f = await newChatFixture();
  try {
    await f.choose('community');
    await f.title('My community', 'community');
    await f.act(() => f.control('createHeader').onPress());
    assert.deepEqual(f.calls, [{ op: 'create_community', title: 'My community' }]);
    await f.reject('Creation unavailable');
    assert.equal(f.control('field:chat.communityName').value, 'My community');
    assert.equal(f.control('text:Creation unavailable').accessibilityRole, 'alert');
    assert.equal(f.control('createHeader').disabled, false);
    assert.equal(f.calls.length, 1);
    assert.equal(f.routes.length, 0);
    await f.act(() => f.control('createHeader').onPress());
    assert.equal(f.calls.length, 2, 'only another explicit press retries');
    await f.resolve('new-community');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('non-iOS creation retains its explicit form action', async () => {
  const f = await newChatFixture('android');
  try {
    await f.choose('group');
    assert.equal(f.controls.has('createHeader'), false);
    assert.equal(f.control('button:chat.createGroup').disabled, true);
    await f.title('Group');
    await f.act(() => f.control('button:chat.createGroup').onPress());
    assert.deepEqual(f.calls, [{ op: 'create_group', title: 'Group', member_ids: [] }]);
    await f.resolve('group');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('group creation failure retains the title and selected participant until an explicit retry', async () => {
  const f = await newChatFixture('ios', [
    { id: 'owned-agent', name: 'QA Agent', handle: 'qa-agent', is_me: false },
  ]);
  try {
    await f.choose('group');
    await f.title('  Shared document  ');
    await f.act(() => f.control('person').onPress());
    assert.equal(f.control('person')['aria-checked'], true);
    await f.act(() => f.control('createHeader').onPress());
    const command = {
      op: 'create_group',
      title: 'Shared document',
      member_ids: ['owned-agent'],
    };
    assert.deepEqual(f.calls, [command]);
    await f.reject('Temporarily unavailable');
    assert.equal(f.control('field:chat.groupName').value, '  Shared document  ');
    assert.equal(f.control('person')['aria-checked'], true);
    assert.equal(f.control('text:Temporarily unavailable').accessibilityRole, 'alert');
    assert.equal(f.control('createHeader').disabled, false);
    assert.equal(f.calls.length, 1);
    assert.equal(f.routes.length, 0);
    await f.act(() => f.control('createHeader').onPress());
    assert.deepEqual(f.calls, [command, command]);
    await f.resolve('shared-document');
    assert.deepEqual(f.routes, [
      'dismiss',
      { pathname: '/chat/[id]', params: { id: 'shared-document' } },
    ]);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
