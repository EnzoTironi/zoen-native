import assert from 'node:assert/strict';
import { test } from 'node:test';
import { sharedItems, sharedLinks, sharedMediaTab } from '../profile/shared-media-projection.ts';
import type { Handler } from './telegram-profile-ui.fixtures.ts';
import {
  deferred,
  fixture,
  item,
  message,
  nodes,
  settle,
  target,
} from './telegram-profile-ui.fixtures.ts';
import type { ItemDetail } from './types.ts';

test('conversation details preferences use fresh canonical unrelated flags; shared-media targets same chat', async () => {
  const f = fixture('space', {
    'expo-router': {
      router: {
        push: (route: unknown) => {
          f.routes.push(route);
        },
      },
      Stack: { Screen: 'Screen' },
      useLocalSearchParams: () => ({ id: 'space' }),
    },
  });
  const tree = f.render();
  f.state.snapshot.conversations[0]!.archived = true;
  const mute = nodes(tree)
    .find((node) => node.type === 'ProfileHeader')!
    .props.actions.find((action: any) => action.id === 'details-mute');
  mute.onPress();
  await settle();
  assert.deepEqual(JSON.parse(JSON.stringify(f.requests[0])), {
    op: 'set_chat_prefs',
    space_id: 'space',
    pinned: false,
    muted: true,
    archived: true,
  });
  target(tree, 'details-shared-media').props.onPress();
  assert.deepEqual(JSON.parse(JSON.stringify(f.routes[0])), {
    pathname: '/shared-media/[id]',
    params: { id: 'space' },
  });
});
test('native details switches keep canonical flags, coalesce events and retire on blur', async () => {
  const f = fixture('space', {
    'expo-router': { useLocalSearchParams: () => ({ id: 'space' }) },
  });
  const tree = f.render();
  const chat = f.state.snapshot.conversations[0]!;
  chat.muted = true;
  chat.archived = true;
  target(tree, 'details-preference-pinned').props.onValueChange(true);
  target(tree, 'details-preference-archived').props.onValueChange(false);
  await settle();
  assert.deepEqual(JSON.parse(JSON.stringify(f.requests)), [
    {
      op: 'set_chat_prefs',
      space_id: 'space',
      pinned: true,
      preserve_mute: true,
      muted: true,
      archived: true,
    },
  ]);
  f.blur();
  target(tree, 'details-preference-muted').props.onValueChange(false);
  await settle();
  assert.equal(f.requests.length, 1);
});
test('shared media projection uses exact authorized space/current descriptors independently of hidden Home shortcuts', () => {
  const photo = item('photo');
  const video = item('video', 'video/mp4');
  const voice = item('voice', 'audio/mp4');
  const pdf = item('pdf', 'application/pdf');
  const app = item('deck', 'application/octet-stream', {
    app: { app_id: 'deckcraft' } as ItemDetail['app'],
    kind_id: 'app',
  });
  const rows = [
    photo,
    video,
    voice,
    pdf,
    app,
    item('other-space', 'image/png', { space_id: 'secret' }),
  ];
  assert.deepEqual(sharedItems(rows, 'space', 'media'), [photo, video]);
  assert.deepEqual(sharedItems(rows, 'space', 'audio'), [voice]);
  assert.deepEqual(sharedItems(rows, 'space', 'apps'), [app]);
  assert.equal(sharedItems(rows, 'space', 'media')[0]!.version, 4);
  assert.equal(sharedItems(rows, 'space', 'media')[0]!.file!.sha256, photo.file!.sha256);
  assert.equal(sharedMediaTab(['apps']), 'media');
  assert.equal(sharedMediaTab('apps'), 'apps');
});
test('shared links omit deleted messages, malformed/credential URLs and deduplicate within each accepted message', () => {
  const rows = sharedLinks([
    message('old', 'https://example.test/a'),
    message('gone', 'https://secret.test', { deleted: true }),
    message(
      'new',
      'https://user:password@example.test/no https://example.test/a https://example.test/a https://[broken',
    ),
  ]);
  assert.deepEqual(
    rows.map((row) => [row.messageId, row.url]),
    [
      ['new', 'https://example.test/a'],
      ['old', 'https://example.test/a'],
    ],
  );
});
test('materialized media must match exact Item/version/hash/MIME; unexpected copies are released', async () => {
  const f = fixture('tile');
  const materialize = f.exports.materializeSharedFile;
  for (const mismatch of [
    { itemId: 'other' },
    { version: 3 },
    { info: { bytes: '5', mime: 'image/jpeg', sha256: 'wrong' } },
    { info: { bytes: '5', mime: 'video/mp4', sha256: 'aa'.repeat(32) } },
    { info: { bytes: '9', mime: 'image/jpeg', sha256: 'aa'.repeat(32) } },
  ]) {
    const accepted = {
      uri: 'local:unexpected',
      itemId: 'photo',
      version: 4,
      info: { bytes: '5', mime: 'image/jpeg', sha256: 'aa'.repeat(32) },
    };
    const releases: string[] = [];
    await assert.rejects(
      materialize(
        {
          materializeFile: async () => ({ ...accepted, ...mismatch }),
          releaseLocalFile: async (uri: string) => {
            releases.push(uri);
          },
        },
        item('photo'),
      ),
    );
    assert.deepEqual(releases, ['local:unexpected']);
  }
  const exact = await materialize(f.store.driver.media, item('photo'));
  assert.equal(exact.version, 4);
  assert.equal(exact.uri, 'local:photo');
});
test('late thumbnail after retirement releases its local copy; late Open never enters a foreign native preview', async () => {
  const f = fixture('tile');
  const thumbnail = deferred<any>();
  const opened = deferred<any>();
  let reads = 0;
  f.store.driver.media.materializeFile = () => (reads++ === 0 ? thumbnail.promise : opened.promise);
  let current = true;
  const tree = f.render({ item: item('photo'), owner: 'owner', isCurrent: () => current });
  tree.props.onPress();
  current = false;
  f.unmount();
  const local = {
    itemId: 'photo',
    version: 4,
    info: { bytes: '5', mime: 'image/jpeg', sha256: 'aa'.repeat(32) },
  };
  thumbnail.resolve({ ...local, uri: 'local:thumbnail' });
  opened.resolve({ ...local, uri: 'local:opened' });
  await settle();
  assert.equal(f.previews.length, 0);
  assert.deepEqual(f.releases.sort(), ['local:opened', 'local:thumbnail']);
});

test('Shared Media renders canonical space items and retires captured navigation when owner changes', () => {
  const routes: unknown[] = [];
  const f = fixture('shared', {
    'expo-router': {
      router: { push: (route: unknown) => routes.push(route) },
      Stack: { Screen: 'Screen' },
      useLocalSearchParams: () => ({ id: 'space' }),
    },
  });
  const deck = item('deck', 'application/octet-stream', {
    app: { app_id: 'deckcraft' } as ItemDetail['app'],
  });
  f.setQuery({
    data: [item('photo'), item('secret', 'image/png', { space_id: 'elsewhere' }), deck],
    error: null,
    retry() {},
  });
  const wrapper = f.render();
  const screen = wrapper.type as Handler;
  let tree = f.render(wrapper.props, screen);
  let list = nodes(tree).find((node) => node.type === 'FlashList')!;
  assert.deepEqual(
    Array.from(list.props.data, (row: ItemDetail) => row.id),
    ['photo'],
  );
  target(list.props.ListHeaderComponent, 'shared-media-tab-apps').props.onPress();
  tree = f.render(wrapper.props, screen);
  list = nodes(tree).find((node) => node.type === 'FlashList')!;
  assert.deepEqual(
    Array.from(list.props.data, (row: ItemDetail) => row.id),
    ['deck'],
  );
  const open = list.props.renderItem({ item: deck });
  open.props.onPress();
  assert.equal(routes.length, 1);
  f.store.sessionEpoch++;
  open.props.onPress();
  assert.equal(routes.length, 1);
});
test('Shared links use explicit canonical history/older requests and reject external open after owner retirement', async () => {
  const routes: unknown[] = [];
  const reads: unknown[] = [];
  const f = fixture('shared', {
    'expo-router': {
      router: { dismissTo: (route: unknown) => routes.push(route) },
      Stack: { Screen: 'Screen' },
      useLocalSearchParams: () => ({ id: 'space', tab: 'links' }),
    },
    'react-native': {
      View: 'View',
      Text: 'Text',
      ScrollView: 'ScrollView',
      Pressable: 'Pressable',
      Linking: {
        openURL: async (url: string) => {
          routes.push(url);
        },
      },
    },
  });
  f.state.timelines.space = {
    entries: [message('message', 'https://example.test/story')],
    has_more: true,
  };
  f.store.loadTimeline = async (...args: unknown[]) => {
    reads.push(args);
  };
  const wrapper = f.render();
  const tree = f.render(wrapper.props, wrapper.type as Handler);
  const list = nodes(tree).find((node) => node.type === 'FlashList')!;
  await settle();
  assert.deepEqual(reads, [['space']]);
  nodes(list.props.ListFooterComponent)
    .find((node) => node.type === 'NativeButton')!
    .props.onPress();
  await settle();
  assert.deepEqual(reads, [['space'], ['space', true]]);
  const row = list.props.renderItem({ item: list.props.data[0] });
  const external = nodes(row).find((node) => node.props.accessibilityRole === 'link')!;
  const jump = nodes(row).find((node) => node.props.accessibilityRole === 'button')!;
  external.props.onPress();
  jump.props.onPress();
  assert.equal(routes.length, 2);
  f.store.sessionEpoch++;
  external.props.onPress();
  jump.props.onPress();
  assert.equal(routes.length, 2);
  const earlier = sharedLinks([
    message('old', 'https://old.test'),
    message('message', 'https://example.test/story'),
  ]);
  const later = sharedLinks([
    message('old', 'https://old.test'),
    message('message', 'https://example.test/story'),
    message('new', 'https://new.test'),
  ]);
  assert.equal(
    earlier.find((link) => link.messageId === 'message')!.id,
    later.find((link) => link.messageId === 'message')!.id,
  );
});
