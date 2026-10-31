import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { randomUUID } from 'node:crypto';
import { build } from 'esbuild';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import * as access from '../home/access.ts';
import * as attachment from '../media/attachment-draft.ts';
import * as contract from '../media/media-draft-contract.ts';
import * as mediaReply from '../media/media-reply.ts';
import * as mediaAcceptance from '../media/media-acceptance.ts';
import * as location from '../media/location-file.ts';
import * as categories from '../media/attachment-categories.ts';
import { AttachmentReview } from '../media/attachment-review.ts';
import { MediaDraftFailure } from '../media/media-draft-channel.ts';
import { sensitiveCacheWork } from '../media/cache-work.ts';
import { mediaPlayback } from '../media/playback-owner.ts';
import { t } from '../i18n/index.ts';
import type { MediaDriver } from '../media/types.ts';
import type { MediaDraftPort } from '../media/media-draft-types.ts';
import type { useAttachmentComposer } from '../media/attachment-composer.tsx';

const require = createRequire(import.meta.url),
  { transformSync } = require('@babel/core');
const files = new Map([
  ['attachment-composer', 'tsx'],
  ['durable-attachment-composer', 'tsx'],
  ['use-durable-attachment-controller', 'ts'],
  ['durable-attachment-view', 'tsx'],
  ['durable-attachment-commands', 'ts'],
  ['durable-attachment-pick', 'ts'],
  ['durable-attachment-review', 'ts'],
  ['attachment-gallery-journal', 'ts'],
  ['attachment-photo-picker', 'tsx'],
]);
const output = await build({
  entryPoints: [new URL('../media/attachment-composer.tsx', import.meta.url).pathname],
  bundle: true,
  format: 'cjs',
  platform: 'node',
  write: false,
  plugins: [
    {
      name: 'actual-compiled-attachment-hooks',
      setup(builder) {
        builder.onResolve({ filter: /^\.\// }, ({ path }) => {
          const name = path.slice(2),
            extension = files.get(name);
          return extension
            ? { path: new URL(`../media/${name}.${extension}`, import.meta.url).pathname }
            : undefined;
        });
        builder.onResolve({ filter: /./ }, ({ path, kind }) =>
          kind === 'entry-point' ? undefined : { path, external: true },
        );
        builder.onLoad({ filter: /\.tsx?$/ }, ({ path }) => {
          const result = transformSync(readFileSync(path, 'utf8'), {
            filename: path,
            babelrc: false,
            configFile: false,
            presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
            plugins: [['babel-plugin-react-compiler', { target: '19' }]],
            caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
          });
          if (path.endsWith('/attachment-composer.tsx'))
            assert.match(result.code, /react\/compiler-runtime/);
          return { contents: result.code, loader: 'js' };
        });
      },
    },
  ],
});
class MediaStoreDriver extends Driver {
  media?: MediaDriver;
}
/** Whole production attachment hook/controller/view stack with actual Compiler,
 * React, Store and media channels; native image/file/picker IO remains a port. */
export async function attachmentOwnerReactFixture(open = true) {
  const root = createReactTestRoot(),
    driver = new MediaStoreDriver(),
    snapshot = draftSession();
  const spaceId = snapshot.conversations[0]!.id;
  let caption = 'Owner A private caption',
    name = 'owner-a-private.jpg';
  const calls: { op: string; scope?: unknown }[] = [];
  const value = () => ({
    revision: '1',
    attempted: false,
    content_json: JSON.stringify({
      kind: 'attachments',
      reply_to: null,
      items: [{ asset_id: 'asset', caption, mutation_id: 'post-asset' }],
    }),
    assets: [{ id: 'asset', name, mime: 'image/jpeg', size: 3, sha256: 'a'.repeat(64) }],
  });
  const port: MediaDraftPort = {
    async load(scope) {
      calls.push({ op: 'load', scope: structuredClone(scope) });
      return value();
    },
    async materialize(scope) {
      calls.push({ op: 'materialize', scope: structuredClone(scope) });
      return { ...value().assets[0]!, uri: `file:///synthetic/${name}` };
    },
    async release() {
      calls.push({ op: 'release' });
    },
    async save() {
      calls.push({ op: 'save' });
      throw Error('Test must not save');
    },
    async send() {
      calls.push({ op: 'send' });
      throw Error('Test must not send');
    },
  };
  driver.media = { drafts: port } as MediaDriver;
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return structuredClone(snapshot);
    throw Error(`Unexpected attachment Core operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  if (open) await store.open('account');
  let foreground = 'active',
    focused = true;
  const appListeners = new Set<(next: string) => void>();
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', callback: () => void) {
      focusListeners[name].add(callback);
      return () => focusListeners[name].delete(callback);
    },
  };
  const loaded = { exports: {} },
    focus = { exports: {} as { useFocusEffect: unknown } };
  runInNewContext(
    readFileSync(require.resolve('expo-router/build/link/useLoadedNavigation.js'), 'utf8'),
    {
      module: loaded,
      exports: loaded.exports,
      require(name: string) {
        if (name === 'react') return React;
        if (name === '../react-navigation/native')
          return {
            useNavigation: () => navigation,
            NavigationContainerRefContext: React.createContext({}),
          };
        throw Error(name);
      },
    },
  );
  runInNewContext(readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8'), {
    module: focus,
    exports: focus.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  const native = () => null;
  let filenames: readonly string[] = [];
  function Strip({ state }: { state: attachment.AttachmentDraftState }) {
    React.useLayoutEffect(() => {
      const value = state.items.map((item) => item.name);
      filenames = value;
      return () => {
        if (filenames === value) filenames = [];
      };
    }, [state]);
    return null;
  }
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      View: ({ children }: { children: React.ReactNode }) => children,
      Text: native,
      Platform: { OS: 'ios' },
      Keyboard: { dismiss() {} },
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener(_: string, callback: (next: string) => void) {
          appListeners.add(callback);
          return { remove: () => appListeners.delete(callback) };
        },
      },
    },
    'expo-router': {
      useFocusEffect: focus.exports.useFocusEffect,
      router: {
        push() {
          throw Error('Unexpected route');
        },
      },
    },
    'expo-crypto': { randomUUID },
    'expo-media-library': { MediaType: { IMAGE: 'image', VIDEO: 'video' } },
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../components/primitives': { NativeButton: native },
    '../components/dialogs': {
      useDialogs: () => ({
        inform() {
          throw Error('Unexpected dialog');
        },
      }),
    },
    '../home/access': access,
    '../i18n/index.ts': { t },
    '../i18n': { t },
    '../theme': { useTheme: () => ({ colors: {} }) },
    './attachment-draft': attachment,
    './media-draft-contract': contract,
    './media-reply': mediaReply,
    './media-acceptance': mediaAcceptance,
    './attachment-review': { AttachmentReview },
    './media-draft-channel': { MediaDraftFailure },
    './location-file': location,
    './attachment-categories': categories,
    './cache-work': { sensitiveCacheWork },
    './playback-owner': { mediaPlayback },
    './cache-files': {
      async discardCacheCopy() {
        throw Error('Unexpected cache discard');
      },
    },
    './attachment-picker': {
      async pickAttachmentAssets() {
        throw Error('Unexpected picker');
      },
    },
    './attachment-draft-strip': { AttachmentDraftStrip: Strip },
    './attachment-approval': { AttachmentApproval: native, supportsMediaApproval: false },
  };
  const module = { exports: {} as { useAttachmentComposer: typeof useAttachmentComposer } };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
    setTimeout,
    clearTimeout,
  });
  let current: ReturnType<typeof useAttachmentComposer> | null = null;
  function Surface() {
    const result = module.exports.useAttachmentComposer({
      spaceId,
      replyTo: null,
      thread: false,
      disabled: false,
      suspended: false,
    });
    React.useLayoutEffect(() => {
      current = result;
    }, [result]);
    return result.review;
  }
  return {
    filenames: () => filenames.join(','),
    store,
    snapshot,
    calls,
    errors: root.errors,
    act: root.act,
    current() {
      assert.ok(current);
      return current;
    },
    render: () => root.render(React.createElement(Surface)),
    open: () => root.act(() => store.open('account')),
    async reopen(change?: 'identity' | 'device' | 'relay') {
      await root.act(async () => {
        await store.close();
        if (change === 'identity') {
          snapshot.account = { ...snapshot.account!, identity_id: 'e'.repeat(64) };
          snapshot.conversations[0]!.members[0]!.id = snapshot.account.identity_id;
        }
        if (change === 'device')
          snapshot.account = { ...snapshot.account!, device_id: 'f'.repeat(64) };
        if (change === 'relay')
          snapshot.account = { ...snapshot.account!, relay_url: 'https://other-relay.invalid/' };
        caption = 'New session private caption';
        name = 'new-session-private.jpg';
        await store.open('account');
      });
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
