import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MutedChatsSession } from '../notifications/muted-chats.ts';
import { fixture, nodes, target } from './telegram-profile-ui.fixtures.ts';
import { mutedSnapshot, scope } from './muted-chats.fixtures.ts';

for (const kind of ['Direct', 'Group'])
  test(`actual ${kind} details opens canonical duration sheet independently of member management`, async () => {
    const snapshot = mutedSnapshot();
    Object.assign(snapshot.conversations[0]!, { id: 'space', muted: false, muted_until_ms: null });
    const writes: unknown[] = [];
    const session = new MutedChatsSession(
      {
        scope: () => scope,
        read: async () => structuredClone(snapshot),
        unmute: async () => {},
        mute: async (_id, mode) => {
          writes.push(mode);
          Object.assign(snapshot.conversations[0]!, {
            muted: mode.kind !== 'off',
            muted_until_ms: mode.kind === 'off' ? null : '9223372036854775807',
          });
        },
      },
      'space',
    );
    session.activate(scope);
    await session.load(session.ticket());
    const f = fixture('space', {
      'expo-router': { useLocalSearchParams: () => ({ id: 'space' }) },
      '../../notifications/use-muted-chats': {
        useMutedChats: () => ({ accountRequired: false, session, state: session.getSnapshot() }),
      },
    });
    f.state.snapshot.conversations[0]!.kind = kind;
    // A stale cached mute must not override Core's current effective projection.
    f.state.snapshot.conversations[0]!.muted = true;
    let tree = f.render();
    assert.equal(target(tree, 'details-preference-muted').props.value, false);
    target(tree, 'details-preference-muted').props.onValueChange(true);
    tree = f.render();
    const sheet = nodes(tree).find((node) => node.type === 'MuteDurationSheet')!;
    assert.ok(sheet, 'Duration controls are available without group mutation authority');
    assert.equal(sheet.props.chat.id, 'space');
    assert.deepEqual(writes, [], 'Opening a duration picker does not change notifications');
    await sheet.props.onChange({ kind: 'forever' });
    for (let n = 0; n < 12; ++n) await Promise.resolve();
    assert.deepEqual(writes, [{ kind: 'forever' }]);
    tree = f.render();
    nodes(tree)
      .find((node) => node.type === 'ProfileHeader')!
      .props.actions.find((action: any) => action.id === 'details-mute')
      .onPress();
    for (let n = 0; n < 12; ++n) await Promise.resolve();
    assert.deepEqual(
      JSON.parse(JSON.stringify(writes)),
      [{ kind: 'forever' }, { kind: 'off' }],
      'The Unmute header action honors its label and immediately restores notifications',
    );
    assert.deepEqual(f.requests, [], 'No legacy boolean write bypasses the duration controller');
  });
