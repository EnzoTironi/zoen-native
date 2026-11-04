import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { hookHost } from './message-selection-react.fixtures.ts';
import { deferred, tick } from './store.fixtures.ts';

const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../app/open-link.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  jsx: 'automatic',
  external: ['*'],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
});

type Element = { type: string; props: Record<string, any> };

function button(node: Element | Element[]): Element | undefined {
  if (Array.isArray(node)) return node.map(button).find(Boolean);
  if (!node?.props) return;
  return node.type === 'Button' ? node : button(node.props.children);
}

test('invite admission rejects an old route callback and late navigation after session replacement', async () => {
  const host = hookHost();
  const routes: unknown[] = [];
  const joins: string[] = [];
  const pending = deferred<string>();
  const snapshot = { account: { identity_id: 'person', device_id: 'device' } };
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => ({ snapshot }),
    request: ({ code }: { code: string }) => {
      joins.push(code);
      return pending.promise;
    },
  };
  let target = 'first';
  const module = { exports: {} as { default(): Element } };
  const jsx = (type: string, props: unknown) => ({ type, props });
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      switch (name) {
        case 'react':
          return host.react;
        case 'react/jsx-runtime':
          return { jsx, jsxs: jsx };
        case 'expo-router':
          return {
            router: { replace: (route: unknown) => routes.push(route) },
            Stack: { Screen: 'Screen' },
            useFocusEffect: host.useFocusEffect,
            useLocalSearchParams: () => ({ kind: 'invite', target }),
          };
        case 'react-native':
          return { View: 'View', Text: 'Text' };
        case '../core/provider':
          return { useClient: () => ({ store, snapshot }) };
        case '../core/query':
          return { useCoreQuery: () => ({ data: { title: 'Group', members: 2 } }) };
        case '../notifications/links':
          return {
            parseZoenLink: (url: string) => ({ kind: 'invite', code: url.split('/').at(-1) }),
          };
        case '../theme':
          return { useTheme: () => ({ colors: {} }) };
        case '../components/primitives':
          return { EmptyState: 'EmptyState', NativeButton: 'Button' };
        case '../i18n/index.ts':
          return { t: (key: string) => key };
        default:
          throw new Error(name);
      }
    },
  });
  const render = () => button(host.render(() => module.exports.default()))!.props.onPress;
  const old = render();
  target = 'second';
  const current = render();
  old();
  assert.deepEqual(joins, []);
  current();
  current();
  assert.deepEqual(joins, ['second']);
  store.sessionEpoch++;
  pending.resolve('joined-space');
  await tick();
  assert.deepEqual(routes, []);
  host.blur();
});
