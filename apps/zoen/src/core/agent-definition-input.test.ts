import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { DefinitionSession } from '../agent-definition/session.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { t } from '../i18n/index.ts';
import type { AgentDefinitionDto } from './generated/AgentDefinitionDto.ts';

type Element = { type: any; props: Record<string, any> };
const jsx = {
  jsx: (type: unknown, props: object) => ({ type, props }),
  jsxs: (type: unknown, props: object) => ({ type, props }),
  Fragment: 'Fragment',
};
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
async function compile(path: string) {
  if (path.includes('/app/')) {
    // The shipped app enables React Compiler. Preserve its actual memo caches,
    // including the first render before the focus effect activates the session.
    return transformSync(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      filename: new URL(path, import.meta.url).pathname,
      babelrc: false,
      configFile: false,
      presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
      plugins: [['babel-plugin-react-compiler', { target: '19' }]],
      caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
    }).code as string;
  }
  const result = await build({
    stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    external: ['*'],
  });
  return result.outputFiles[0]!.text;
}
const [routeCode, viewCode, primitivesCode, settingsCode] = await Promise.all([
  compile('../app/agent-definition/[id].tsx'),
  compile('../agent-definition/definition-view.tsx'),
  compile('../components/primitives.tsx'),
  compile('../components/settings-screen.tsx'),
]);
const generate = require('@babel/generator').default;
let changeHandler: any;
const responderProperties: any[] = [];
for (const name of ['TextInput', 'ScrollView']) {
  transformSync(
    readFileSync(require.resolve(`react-native/Libraries/Components/${name}/${name}.js`), 'utf8'),
    {
      filename: `${name}.js`,
      babelrc: false,
      configFile: false,
      presets: ['module:@react-native/babel-preset'],
      plugins: [
        () => ({
          visitor: {
            VariableDeclarator(path: any) {
              if (name === 'TextInput' && path.node.id.name === '_onChange')
                changeHandler = path.node.init;
            },
            ClassProperty(path: any) {
              if (
                name === 'ScrollView' &&
                [
                  '_handleStartShouldSetResponder',
                  '_handleStartShouldSetResponderCapture',
                  '_handleResponderRelease',
                ].includes(path.node.key.name)
              )
                responderProperties.push(path.node);
            },
          },
        }),
      ],
    },
  );
}
assert.ok(changeHandler, 'actual installed RN TextInput change handler');
assert.equal(responderProperties.length, 3);
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  if (typeof node.type === 'function') return nodes(node.type(node.props));
  return [node, ...nodes(node.props.children)];
}
function target(tree: Element, id: string) {
  const found = nodes(tree).find((node) => node.props.testID === id);
  assert.ok(found, id);
  return found;
}
async function settle() {
  for (let i = 0; i < 16; i++) await Promise.resolve();
}
async function fixture() {
  const baseline: AgentDefinitionDto = {
    agent_id: 'agent',
    owner_id: 'owner',
    device_id: 'phone',
    relay_url: 'http://127.0.0.1:8891',
    status: 'Active',
    version: 1,
    definition_sha256: 'base',
    persona: 'Synthetic baseline.',
    rules: ['Ask before acting.'],
  };
  const scope = { agent: 'agent', owner: 'owner', device: 'phone', relay: baseline.relay_url };
  const calls: string[] = [];
  const session = new DefinitionSession({
    read: async () => baseline,
    reconcile: async () => null,
    preview: async (_scope, current, persona, rules) => {
      calls.push('preview');
      return {
        current,
        proposed: { ...current, persona, rules, version: 2, definition_sha256: 'next' },
        preview_digest: 'digest',
      };
    },
    update: async (_scope, review) => {
      calls.push('update');
      return { receipt_id: 'receipt', review, outcome: 'Confirmed', current: review.proposed };
    },
    dismiss: async () => {
      calls.push('dismiss');
      return true;
    },
  });
  let cursor = 0,
    protectedState = false,
    dismissed = 0,
    dialogs = 0,
    backs = 0;
  const refs: { current: unknown }[] = [];
  const theme = {
    useTheme: () => ({ presentation: telegramPresentation('night'), scheme: 'dark' }),
  };
  const native = {
    Text: 'Text',
    View: 'View',
    TextInput: 'TextInput',
    ScrollView: 'ScrollView',
    ActivityIndicator: 'ActivityIndicator',
    Pressable: 'Pressable',
    Keyboard: { dismiss: () => dismissed++ },
  };
  function load(code: string, imports: Record<string, unknown>) {
    const module: { exports: Record<string, any> } = { exports: {} };
    runInNewContext(code, {
      module,
      exports: module.exports,
      require: (name: string) => {
        if (name === 'react/jsx-runtime') return jsx;
        if (name === 'react-native') return native;
        if (name === '../theme') return theme;
        if (name === '../i18n' || name === '../i18n/index.ts') return { t };
        if (name.startsWith('@babel/runtime/helpers/')) return require(name);
        if (name in imports) return imports[name];
        throw Error(`Unexpected joined input import ${name}`);
      },
    });
    return module.exports;
  }
  const primitives = load(primitivesCode, {
    // This fixture ports hooks while joining real form/input/responder wiring.
    // field-identity.test.ts qualifies useId with the actual React lifecycle.
    react: { useId: () => 'joined-input-field' },
    'expo-symbols': { SymbolView: 'SymbolView' },
    '@expo/ui': { Host: 'Host', Button: 'Button' },
    'expo-image': { Image: 'Image' },
    '../theme/telegram': {},
  });
  const settings = load(settingsCode, { 'expo-router': { Stack: { Screen: 'Screen' } } });
  const view = load(viewCode, {
    '../components/primitives': primitives,
    '../components/settings-screen': settings,
    '../profile/profile-section': { ProfileSection: 'ProfileSection' },
  });
  let memoCache: unknown[] | undefined;
  const route = load(routeCode, {
    'react/compiler-runtime': {
      c: (size: number) =>
        (memoCache ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    react: {
      useState: () => [false, () => {}],
      useRef: (value: unknown) => (refs[cursor++] ??= { current: value }),
    },
    'expo-router': {
      useLocalSearchParams: () => ({ id: 'agent' }),
      router: {
        back: () => {
          backs++;
        },
      },
      usePreventRemove: (enabled: boolean) => {
        protectedState = enabled;
      },
    },
    '../../agent-definition/use-agent-definition': {
      useAgentDefinition: () => ({ session, state: session.getSnapshot() }),
    },
    '../../agent-definition/definition-view': view,
    '../../components/dialogs': {
      useDialogs: () => ({
        confirm: async () => {
          dialogs++;
          return false;
        },
      }),
    },
    '../../i18n': { t },
  });
  const acknowledgements: unknown[] = [];
  function nativeChange(field: Element, text: string, eventCount: number) {
    assert.equal(field.type, 'TextInput', 'real shared Field expanded to its RN input');
    const handler = runInNewContext(`(${generate(changeHandler).code})`, {
      props: field.props,
      inputRef: { current: {} },
      setLastNativeText: (value: string) => acknowledgements.push(['text', value]),
      setMostRecentEventCount: (value: number) => acknowledgements.push(['count', value]),
    });
    handler({ nativeEvent: { text, eventCount } });
  }
  const render = () => {
    cursor = 0;
    return route.default() as Element;
  };
  render();
  session.activate(scope);
  await session.load();
  return {
    baseline,
    session,
    scope,
    calls,
    acknowledgements,
    nativeChange,
    render,
    counts: () => ({ protectedState, dismissed, dialogs }),
    backs: () => backs,
  };
}

test('compiler-transformed owner route and real Field accept sequential RN text events after cold activation', async () => {
  const f = await fixture();
  let published = 0;
  f.session.subscribe(() => published++);
  assert.equal(target(f.render(), 'agent-definition-persona').type, 'TextInput');
  assert.equal(f.counts().protectedState, false);
  const values = ['', 'Z', 'Zo', '  e\u0301 🪷\nexact  ', '  Complete owner text.\nPreserve it.  '];
  for (const [index, value] of values.entries()) {
    const field = target(f.render(), 'agent-definition-persona');
    assert.equal(field.props.editable, true);
    assert.equal(field.props.showSoftInputOnFocus, undefined);
    f.nativeChange(field, value, index + 1);
    assert.equal(f.session.getSnapshot().persona, value);
    assert.equal(target(f.render(), 'agent-definition-persona').props.value, value);
    assert.equal(f.counts().protectedState, true);
  }
  const persona = f.session.getSnapshot().persona;
  const ruleId = `agent-definition-rule-${f.session.getSnapshot().ruleKeys[0]}`;
  f.nativeChange(target(f.render(), ruleId), '  Regra e\u0301 🪷  ', 6);
  assert.equal(f.session.getSnapshot().persona, persona);
  assert.deepEqual(f.session.getSnapshot().rules, ['  Regra e\u0301 🪷  ']);
  assert.equal(published, 6);
  assert.deepEqual(f.acknowledgements.slice(-2), [
    ['text', '  Regra e\u0301 🪷  '],
    ['count', 6],
  ]);
  assert.equal(target(f.render(), 'agent-definition-review').props.disabled, false);
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.counts(), { protectedState: true, dismissed: 0, dialogs: 0 });
});

test('actual ScrollView responder admits one explicit Review tap after real controlled input edits', async () => {
  const f = await fixture();
  f.nativeChange(target(f.render(), 'agent-definition-persona'), 'Exact reviewed text 🪷\n  ', 1);
  const tree = f.render(),
    scroll = target(tree, 'agent-definition-screen');
  assert.equal(scroll.type, 'ScrollView', 'real SettingsScreen expanded');
  assert.equal(scroll.props.keyboardShouldPersistTaps, 'always');
  assert.equal(scroll.props.automaticallyAdjustKeyboardInsets, true);
  const input = {},
    state = {
      currentlyFocusedInput: () => input,
      isTextInput: (value: unknown) => value === input,
      blurTextInput: () => {
        throw Error('Unexpected responder blur');
      },
    };
  const ScrollResponder = runInNewContext(
    `(class {
    constructor(props) { this.props=props; }
    _keyboardIsDismissible=()=>true; _softKeyboardIsDetached=()=>false; _isAnimating=()=>false;
    ${responderProperties.map((node) => generate(node).code).join('\n')}
  })`,
    { __DEV__: false, TextInputState: state, _TextInputState: { default: state } },
  );
  const responder = new ScrollResponder(scroll.props),
    event = { target: {}, nativeEvent: { touches: [] } };
  assert.equal(responder._handleStartShouldSetResponderCapture(event), false);
  assert.equal(responder._handleStartShouldSetResponder(event), false);
  target(tree, 'agent-definition-review').props.onPress();
  await settle();
  assert.deepEqual(f.calls, ['preview']);
  assert.equal(f.session.getSnapshot().phase, 'review');
  assert.equal(f.session.getSnapshot().review?.proposed.persona, 'Exact reviewed text 🪷\n  ');
  assert.deepEqual(f.counts(), { protectedState: true, dismissed: 1, dialogs: 0 });
  assert.ok(target(f.render(), 'agent-definition-apply'));
  assert.deepEqual(f.calls, ['preview'], 'rendering Apply never dispatches it');
  const apply = target(f.render(), 'agent-definition-apply');
  apply.props.onPress();
  await settle();
  assert.deepEqual(f.calls, ['preview', 'update']);
  assert.equal(f.session.getSnapshot().receipt?.outcome, 'Confirmed');
  apply.props.onPress();
  await settle();
  assert.deepEqual(f.calls, ['preview', 'update'], 'a stale Apply cannot repeat dispatch');
});

test('late native changes from an abandoned Field cannot modify the newly activated owner flow', async () => {
  const f = await fixture(),
    field = target(f.render(), 'agent-definition-persona');
  f.session.retire();
  f.session.activate(f.scope);
  await f.session.load();
  f.nativeChange(field, 'Stale native event', 1);
  assert.equal(f.session.getSnapshot().persona, f.baseline.persona);
  assert.equal(target(f.render(), 'agent-definition-review').props.disabled, true);
  assert.deepEqual(f.calls, []);
});

test('compiled native buttons work before keyboard focus and render tokens reject old snapshots', async () => {
  const f = await fixture();
  const tree = f.render();
  const done = nodes(tree).find(
    (node) => node.type === 'Button' && node.props.label === t('agentDefinition.done'),
  );
  assert.ok(done);
  done.props.onPress();
  assert.equal(f.backs(), 1);
  const before = f.session.getSnapshot();
  assert.ok(f.session.ticket(before));
  target(tree, 'agent-definition-add-rule').props.onPress();
  assert.equal(f.session.getSnapshot().rules.length, 2);
  assert.equal(f.session.ticket(before), null);
  assert.equal(f.session.confirmation(before), null);
  assert.equal(f.session.dirty(before), false);
  assert.ok(f.session.ticket());
  assert.equal(f.session.dirty(), true);
  const oldField = target(f.render(), 'agent-definition-persona');
  const oldTicket = f.session.ticket()!;
  f.session.retire();
  f.session.activate({ ...f.scope, device: 'different-device' });
  await f.session.load();
  f.nativeChange(oldField, 'Stale owner/device callback', 1);
  assert.equal(f.session.acceptsSession(oldTicket), false);
  assert.equal(f.session.getSnapshot().current, null);
  assert.equal(f.session.getSnapshot().phase, 'error');
  assert.deepEqual(f.calls, []);
});
