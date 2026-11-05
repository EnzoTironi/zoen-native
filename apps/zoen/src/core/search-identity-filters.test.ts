import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { searchIdentityOptions } from './search-identity-options.ts';
import type { SpaceSummary } from './generated/SpaceSummary';
import type { UniversalHit } from './generated/UniversalHit';
const require = createRequire(import.meta.url);
const compiled = await build({
  entryPoints: [new URL('../components/search-identity-filters.tsx', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  packages: 'external',
  external: ['../theme', '../i18n/index.ts', '../profile/profile-avatar', './primitives'],
});

test('native sheet uses exact identities, filters names/handles, clears one field and fences stale rows', async () => {
  const root = createReactTestRoot(),
    rows = new Map<string, any>();
  let input: any,
    list: any,
    current = true;
  const changes: any[] = [];
  let value: any = { author_id: 'a', space_id: 'chat' };
  const options = {
    authors: [
      { id: 'a', title: 'Same name', person: { handle: 'alpha' } },
      { id: 'b', title: 'Same name', person: { handle: 'beta' } },
    ],
    spaces: [{ id: 'chat', title: 'Chat' }],
  };
  const Container = ({ children }: any) => React.createElement(React.Fragment, null, children);
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/jsx-runtime') return require(name);
      if (name === '@expo/ui')
        return {
          Host: Container,
          RNHostView: Container,
          BottomSheet: ({ isPresented, children }: any) =>
            isPresented ? React.createElement(Container, null, children) : null,
        };
      if (name === 'react-native')
        return {
          View: Container,
          Text: () => null,
          TextInput: (props: any) => {
            input = props;
            return null;
          },
          Pressable: (props: any) => {
            rows.set(props.testID, props);
            return React.createElement(Container, props);
          },
          FlatList: (props: any) => {
            list = props;
            return React.createElement(
              Container,
              null,
              props.ListHeaderComponent,
              ...props.data.map((item: any) =>
                React.createElement(React.Fragment, { key: item.id }, props.renderItem({ item })),
              ),
            );
          },
        };
      if (name === '../theme') return { useTheme: () => ({ colors: {}, scheme: 'dark' }) };
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      if (name === '../profile/profile-avatar') return { ProfileAvatar: () => null };
      if (name === './primitives') return { Icon: () => null, IconButton: () => null };
      throw Error(name);
    },
  });
  const render = () =>
    root.render(
      React.createElement(module.exports.SearchIdentityFilters, {
        value,
        options,
        isCurrent: () => current,
        onChange: (next: any) => {
          changes.push(next);
          value = next;
        },
      }),
    );
  try {
    await render();
    await root.act(() => rows.get('search-filter-author_id').onPress());
    assert.equal(list.data.length, 2);
    await root.act(() => input.onChangeText('beta'));
    assert.equal(list.data.length, 1);
    assert.equal(list.data[0].id, 'b');
    const stale = rows.get('search-identity-b').onPress;
    await root.act(() => rows.get('search-filter-space_id').onPress());
    await root.act(() => stale());
    assert.equal(changes.length, 0);
    await root.act(() => rows.get('search-identity-clear').onPress());
    assert.deepEqual(JSON.parse(JSON.stringify(changes[0])), { author_id: 'a' });
    await render();
    await root.act(() => rows.get('search-filter-author_id').onPress());
    await root.act(() => rows.get('search-identity-b').onPress());
    assert.equal(changes[1].author_id, 'b');
    await render();
    await root.act(() => rows.get('search-filter-author_id').onPress());
    const late = rows.get('search-identity-a').onPress;
    current = false;
    await root.act(() => late());
    assert.equal(changes.length, 2);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});

test('identity choices keep same-name people separate and include historical authors only from the visible result context', () => {
  const spaces = [
    {
      id: 'room',
      title: 'Room',
      members: [
        { id: 'a', name: 'Same' },
        { id: 'b', name: 'Same' },
      ],
    },
    { id: 'other', title: 'Room', members: [{ id: 'other-person', name: 'Other' }] },
  ] as SpaceSummary[];
  const hits = [
    { kind: 'message', space_id: 'room', persona: { id: 'former', name: 'Former' } },
    { kind: 'message', space_id: 'other', persona: { id: 'foreign', name: 'Foreign' } },
  ] as UniversalHit[];
  const options = searchIdentityOptions(spaces, hits, 'room');
  assert.deepEqual(new Set(options.authors.map((x) => x.id)), new Set(['a', 'b', 'former']));
  assert.deepEqual(
    options.spaces.map((x) => x.id),
    ['other', 'room'],
  );
});
