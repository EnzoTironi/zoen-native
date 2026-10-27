import assert from 'node:assert/strict';
import { test } from 'node:test';
import { peerInfo } from '../i18n/catalogs/peer-info.ts';
import { fixture, nodes, target } from './telegram-profile-ui.fixtures.ts';

for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: compiled profile decision actions retain full accessible names and activation while allowing text to grow`, () => {
    const copy = peerInfo[locale],
      activated: string[] = [];
    const actions = [
      {
        id: 'message',
        label: copy.backToChat,
        icon: 'bubble',
        onPress: () => activated.push('message'),
      },
      { id: 'unmute', label: copy.unmute, icon: 'bell', onPress: () => activated.push('unmute') },
      { id: 'search', label: copy.search, icon: 'search', onPress: () => activated.push('search') },
    ];
    const f = fixture('header');
    const tree = f.render({ name: 'Ana', actions });
    for (const action of actions) {
      const button = target(tree, action.id),
        layout = button.props.style({ pressed: false });
      assert.equal(
        layout.height,
        undefined,
        'The action must not bound a scaled/multiline decision to58pt',
      );
      assert.equal(layout.minHeight, 58);
      const label = nodes(button).find((node) => node.type === 'Text')!;
      assert.equal(
        label.props.numberOfLines,
        undefined,
        'The visible decision must not be ellipsized',
      );
      assert.equal(label.props.children, action.label.toLocaleLowerCase());
      assert.equal(button.props.accessibilityRole, 'button');
      assert.equal(button.props.accessibilityLabel, action.label);
      button.props.onPress();
    }
    assert.deepEqual(activated, ['message', 'unmute', 'search']);
  });

test('the actual conversation profile search action still reaches search scoped to that conversation', () => {
  const details = fixture('space', {
    'expo-router': { useLocalSearchParams: () => ({ id: 'space' }) },
  });
  const screen = details.render();
  const header = nodes(screen).find((node) => node.type === 'ProfileHeader')!;
  const actualHeader = fixture('header').render(header.props);
  const search = target(actualHeader, 'details-search');
  search.props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(details.routes)), [
    ['push', { pathname: '/search', params: { space: 'space' } }],
  ]);
});
