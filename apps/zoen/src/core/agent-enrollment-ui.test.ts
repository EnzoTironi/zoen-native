import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { createRequire } from 'node:module';
import { build } from 'esbuild';
import { telegramPresentation } from '../theme/telegram.ts';
import { navigationGlassAppearance } from '../components/navigation-appearance.ts';
import { t } from '../i18n/index.ts';
import {
  EnrollmentSession,
  emptyEnrollmentState,
  type EnrollmentState,
} from '../agent-enrollment/session.ts';

type Element = { type: unknown; props: Record<string, any> };
const output = await build({
  stdin: {
    contents: readFileSync(
      new URL('../agent-enrollment/connect-agent-view.tsx', import.meta.url),
      'utf8',
    ),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'enrollment-view-platform-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
const review: NonNullable<EnrollmentState['review']> = {
  agent: 'agent',
  digest: 'digest',
  name: 'A long agent name that must wrap naturally',
  handle: 'booking',
  persona: 'A long description. '.repeat(30),
  rules: ['Ask before booking.', 'Never spend without permission.'],
  pairing: '123456',
  expires: 2000n,
};
function fixture(state: EnrollmentState, availability = 'ready') {
  let paste = false;
  let keyboardDismissals = 0;
  const actions: string[] = [];
  const props: Record<string, any> = {
    state,
    availability,
    scannerAvailable: true,
    scannerError: null,
    onScan: () => actions.push('scan'),
    onCode: () => actions.push('code'),
    onCheck: () => actions.push('check'),
    onConfirm: () => actions.push('confirm'),
    onNewCode: () => actions.push('new-code'),
    onReconcile: () => actions.push('status'),
    onRetryRestore: () => actions.push('retry'),
    onDismiss: () => actions.push('dismiss'),
    onProfile: () => actions.push('profile'),
    onStartAnother: () => actions.push('another'),
  };
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    require: (name: string) => {
      if (name === 'react')
        return {
          useState: () => [
            paste,
            (value: boolean) => {
              paste = value;
            },
          ],
        };
      if (name === 'react/jsx-runtime')
        return {
          jsx: (type: unknown, props: object) => ({ type, props }),
          jsxs: (type: unknown, props: object) => ({ type, props }),
          Fragment: 'Fragment',
        };
      if (name === 'react-native')
        return {
          Text: 'Text',
          View: 'View',
          Keyboard: { dismiss: () => keyboardDismissals++ },
        };
      if (name === '../theme')
        return { useTheme: () => ({ presentation: telegramPresentation('night') }) };
      if (name === '../i18n') return { t };
      if (name === '../components/primitives')
        return { Field: 'Field', Icon: 'Icon', Loading: 'Loading', NativeButton: 'NativeButton' };
      if (name === '../components/settings-rows') return { SettingsRows: 'SettingsRows' };
      if (name === '../components/settings-screen') return { SettingsScreen: 'SettingsScreen' };
      if (name === '../profile/profile-header') return { ProfileHeader: 'ProfileHeader' };
      if (name === '../profile/profile-section') return { ProfileSection: 'ProfileSection' };
      throw Error(`Unexpected view import: ${name}`);
    },
  });
  const render = () => module.exports.ConnectAgentView(props) as Element;
  return { actions, props, render, keyboardDismissals: () => keyboardDismissals };
}
const target = (tree: Element, id: string) => {
  const found = nodes(tree).find((node) => node.props.testID === id);
  assert.ok(found, id);
  return found;
};

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const generate = require('@babel/generator').default;
const responderProperties: unknown[] = [];
transformSync(
  readFileSync(
    require.resolve('react-native/Libraries/Components/ScrollView/ScrollView.js'),
    'utf8',
  ),
  {
    filename: 'ScrollView.js',
    babelrc: false,
    configFile: false,
    presets: ['module:@react-native/babel-preset'],
    plugins: [
      () => ({
        visitor: {
          ClassProperty(path: any) {
            if (
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
assert.equal(responderProperties.length, 3);
const settingsOutput = await build({
  stdin: {
    contents: readFileSync(new URL('../components/settings-screen.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  external: ['*'],
});
function settingsScroll(tree: Element) {
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(settingsOutput.outputFiles[0]!.text, {
    module,
    require: (name: string) => {
      if (name === 'react-native') return { ScrollView: 'ScrollView' };
      if (name === 'expo-router') return { Stack: { Screen: 'Screen' } };
      if (name === './navigation-appearance') return { navigationGlassAppearance };
      if (name === '../theme')
        return { useTheme: () => ({ presentation: telegramPresentation('night') }) };
      if (name === 'react/jsx-runtime')
        return {
          jsx: (type: unknown, props: object) => ({ type, props }),
          jsxs: (type: unknown, props: object) => ({ type, props }),
        };
      throw Error(`Unexpected SettingsScreen import: ${name}`);
    },
  });
  return module.exports.SettingsScreen(tree.props) as Element;
}
function nativeHostedTap(scroll: Element, press: () => void) {
  let blurred = false;
  const input = {};
  const inputState = {
    currentlyFocusedInput: () => input,
    isTextInput: (target: object) => target === input,
    blurTextInput: () => {
      blurred = true;
    },
  };
  const ScrollResponder = runInNewContext(
    `(class { constructor(props) { this.props=props; }
      _keyboardIsDismissible=()=>true;
      _softKeyboardIsDetached=()=>false;
      _isAnimating=()=>false;
      ${responderProperties.map((node) => generate(node).code).join('\n')}
    })`,
    {
      __DEV__: false,
      TextInputState: inputState,
      _TextInputState: { default: inputState },
    },
  );
  const responder = new ScrollResponder(scroll.props);
  const event = { target: {}, nativeEvent: { touches: [] } };
  const intercepted =
    responder._handleStartShouldSetResponderCapture(event) ||
    responder._handleStartShouldSetResponder(event);
  if (intercepted) responder._handleResponderRelease(event);
  else press();
  return { intercepted, blurred };
}

test('actual RN responder delivers the first native Check tap to preview with keyboard visible', async () => {
  const calls: string[] = [];
  const session = new EnrollmentSession(
    {
      preview: async (code: string) => {
        calls.push(`preview:${code}`);
        return review;
      },
      approve: async () => {
        throw Error('Preview must not approve');
      },
    } as any,
    () => 1000n,
  );
  session.activate({ owner: 'owner', device: 'phone', relay: 'https://relay.example' });
  session.setCode('synthetic-code');
  const f = fixture(session.getSnapshot());
  f.props.onCheck = () => session.check();
  const tree = f.render();
  const scroll = settingsScroll(tree);
  // The real installed RN responder explains the old native-host interception.
  assert.equal(
    nativeHostedTap(
      { ...scroll, props: { ...scroll.props, keyboardShouldPersistTaps: 'handled' } },
      () => {},
    ).intercepted,
    true,
  );
  const tap = nativeHostedTap(scroll, () => target(tree, 'agent-enrollment-check').props.onPress());
  assert.equal(tap.intercepted, false);
  assert.equal(tap.blurred, false);
  assert.equal(f.keyboardDismissals(), 1);
  assert.deepEqual(calls, ['preview:synthetic-code']);
  await Promise.resolve();
  assert.equal(session.getSnapshot().phase, 'review');
  assert.equal(scroll.props.automaticallyAdjustKeyboardInsets, true);
});

test('explicit keyboard Go shares Check, and the session fences a simultaneous second preview', async () => {
  const calls: string[] = [];
  const session = new EnrollmentSession(
    {
      preview: async (code: string) => {
        calls.push(code);
        return review;
      },
    } as any,
    () => 1000n,
  );
  session.activate({ owner: 'owner', device: 'phone', relay: 'https://relay.example' });
  session.setCode('exact-code');
  const f = fixture(session.getSnapshot());
  f.props.onCheck = () => session.check();
  const tree = f.render();
  const field = target(tree, 'agent-enrollment-code-input');
  assert.equal(field.props.returnKeyType, 'go');
  field.props.onSubmitEditing();
  target(tree, 'agent-enrollment-check').props.onPress();
  assert.deepEqual(calls, ['exact-code']);
  await Promise.resolve();
  assert.equal(session.getSnapshot().phase, 'review');
  assert.equal(f.keyboardDismissals(), 2);
  const row = target(tree, 'agent-enrollment-code-row');
  assert.equal(row.props.style.flexDirection, 'row');
  assert.equal(row.props.style.flexWrap, 'wrap');
  assert.ok(nodes(row).includes(field));
  assert.ok(nodes(row).includes(target(tree, 'agent-enrollment-check')));
});

for (const availability of ['loading', 'restore-error', 'account-required', 'unsupported']) {
  test(`actual view exposes no scanner/input/confirmation while ${availability}`, () => {
    const f = fixture(emptyEnrollmentState, availability);
    const tree = f.render();
    assert.equal(
      nodes(tree).some((node) =>
        [
          'agent-enrollment-input-options',
          'agent-enrollment-code-input',
          'agent-enrollment-confirm',
        ].includes(node.props.testID),
      ),
      false,
    );
    assert.deepEqual(f.actions, []);
  });
}

test('actual paste option only reveals the field; checking and confirmation remain explicit', () => {
  const f = fixture(emptyEnrollmentState);
  const tree = f.render();
  const paste = target(tree, 'agent-enrollment-input-options').props.rows.find(
    (row: any) => row.id === 'agent-enrollment-paste',
  );
  paste.onPress();
  assert.deepEqual(f.actions, []);
  const field = target(f.render(), 'agent-enrollment-code-input');
  assert.equal(field.props.autoComplete, 'off');
  assert.equal(field.props.autoCorrect, false);
  assert.equal(target(f.render(), 'agent-enrollment-check').props.disabled, true);
});

test('actual connection input masks the full code without transformation or automatic preview', () => {
  const code = 'zoen-agent-enroll:1:Exact_Case-and+bytes/==';
  const f = fixture({ ...emptyEnrollmentState, code });
  const received: string[] = [];
  f.props.onCode = (value: string) => received.push(value);
  const field = target(f.render(), 'agent-enrollment-code-input');
  assert.equal(field.props.secureTextEntry, true);
  assert.equal(field.props.multiline, false);
  assert.equal(field.props.autoCapitalize, 'none');
  assert.equal(field.props.autoCorrect, false);
  assert.equal(field.props.spellCheck, false);
  assert.equal(field.props.autoComplete, 'off');
  assert.equal(field.props.textContentType, 'none');
  assert.equal(field.props.importantForAutofill, 'no');
  assert.equal(field.props.value, code);
  assert.equal(String(field.props.label).includes(code), false);
  field.props.onChangeText(code);
  assert.deepEqual(received, [code]);
  assert.deepEqual(f.actions, []);
  target(f.render(), 'agent-enrollment-check').props.onPress();
  assert.deepEqual(f.actions, ['check']);
  f.props.state = { ...f.props.state, phase: 'review', review };
  assert.equal(
    nodes(f.render()).some((node) => node.props.testID === 'agent-enrollment-code-input'),
    false,
  );
});

test('actual review shows all human rules and the exact code, with no fixed-height truncation', () => {
  const f = fixture({ ...emptyEnrollmentState, phase: 'review', review });
  const tree = f.render();
  assert.equal(target(tree, 'agent-enrollment-pairing-code').props.children, review.pairing);
  assert.deepEqual(
    nodes(tree)
      .filter((node) => node.type === 'Text' && review.rules.includes(node.props.children))
      .map((node) => node.props.children),
    [...review.rules],
  );
  for (const node of nodes(tree).filter((node) => node.type === 'Text')) {
    assert.equal(node.props.numberOfLines, undefined);
    assert.equal(node.props.style?.height, undefined);
  }
  assert.deepEqual(f.actions, []);
  target(tree, 'agent-enrollment-confirm').props.onPress();
  assert.deepEqual(f.actions, ['confirm']);
});

for (const outcome of ['Sent', 'Unknown'] as const) {
  test(`actual ${outcome} delivery view has status and Done, never a resend/approval control`, () => {
    const f = fixture({
      ...emptyEnrollmentState,
      phase: 'waiting',
      review,
      outcome,
      receipt: 'receipt',
      receiptKnown: true,
    });
    const tree = f.render();
    assert.equal(
      nodes(tree).some((node) => node.props.testID === 'agent-enrollment-confirm'),
      false,
    );
    assert.equal(
      nodes(tree).some((node) => node.props.label === t('agentEnrollment.cancel')),
      false,
    );
    target(tree, 'agent-enrollment-status').props.onPress();
    assert.deepEqual(f.actions, ['status']);
    assert.equal(target(tree, 'agent-enrollment-another').props.disabled, false);
  });
}

test('submitting disables approval and labels dismissal Done with the delivery notice', () => {
  const f = fixture({ ...emptyEnrollmentState, phase: 'submitting', review });
  const tree = f.render();
  assert.equal(target(tree, 'agent-enrollment-confirm').props.disabled, true);
  assert.equal(
    nodes(tree).some((node) => node.props.label === t('agentEnrollment.cancel')),
    false,
  );
  assert.ok(
    nodes(tree).some((node) => node.props.children === t('agentEnrollment.deliveryNotice')),
  );
});

test('expired review disables approval and offers a new code without leaving the flow', () => {
  const f = fixture({ ...emptyEnrollmentState, phase: 'review', review, error: 'expired' });
  assert.equal(target(f.render(), 'agent-enrollment-confirm').props.disabled, true);
  target(f.render(), 'agent-enrollment-new-code').props.onPress();
  assert.deepEqual(f.actions, ['new-code']);
});

test('only the registered projection exposes Open profile; unknown receipt cannot start another flow', () => {
  const f = fixture({ ...emptyEnrollmentState, phase: 'waiting', review, outcome: 'Unknown' });
  assert.equal(target(f.render(), 'agent-enrollment-another').props.disabled, true);
  assert.equal(
    nodes(f.render()).some((node) => node.props.testID === 'agent-enrollment-profile'),
    false,
  );
  f.props.state = { ...f.props.state, phase: 'registered' };
  target(f.render(), 'agent-enrollment-profile').props.onPress();
  assert.deepEqual(f.actions, ['profile']);
});

test('revoked registration shows the terminal state without waiting, profile or approval actions', () => {
  const f = fixture({
    ...emptyEnrollmentState,
    phase: 'revoked',
    review,
    receipt: 'receipt',
    receiptKnown: true,
  });
  const tree = f.render();
  assert.ok(nodes(tree).some((node) => node.props.children === t('agentEnrollment.revoked')));
  assert.equal(
    nodes(tree).some((node) =>
      ['agent-enrollment-status', 'agent-enrollment-profile', 'agent-enrollment-confirm'].includes(
        node.props.testID,
      ),
    ),
    false,
  );
  assert.equal(target(tree, 'agent-enrollment-another').props.disabled, false);
  assert.deepEqual(f.actions, []);
});
