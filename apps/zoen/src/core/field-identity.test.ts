import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import * as React from 'react';
import type { TextInputProps } from 'react-native';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import * as telegram from '../theme/telegram.ts';

const require = createRequire(import.meta.url);
type FieldProps = TextInputProps & { label: string; key: string };

/** Actual Expo/React Compiler Field and real React lifecycle. The native input
 * is a prop-observation port; this does not claim iOS AX or device proof. */
function fixture() {
  const root = createReactTestRoot();
  const inputs = new Map<object, TextInputProps>();
  function Input(props: TextInputProps) {
    const instance = React.useRef({});
    React.useLayoutEffect(() => {
      const key = instance.current;
      inputs.set(key, props);
      return () => {
        inputs.delete(key);
      };
    }, [props]);
    return null;
  }
  const children = ({ children }: { children?: React.ReactNode }) => children;
  const { Field } = loadEditorAgent('../components/primitives.tsx', {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': { View: children, Text: () => null, TextInput: Input },
    'expo-symbols': {},
    'expo-image': {},
    '@expo/ui': {},
    '../i18n/index.ts': { t: (key: string) => key },
    '../theme': { useTheme: () => ({ presentation: telegram.telegramPresentation('day') }) },
    '../theme/telegram': telegram,
  });
  return {
    ...root,
    inputs: () => [...inputs.values()],
    render: (...fields: FieldProps[]) =>
      root.render(
        React.createElement(
          React.Fragment,
          null,
          ...fields.map((props) => React.createElement(Field, props)),
        ),
      ),
  };
}

test('actual Field gives equal labels distinct nonempty native identifiers', async () => {
  const f = fixture();
  try {
    await f.render(
      { key: 'first', label: 'Title', value: '' },
      { key: 'second', label: 'Title', value: '' },
    );
    const inputs = f.inputs();
    assert.equal(inputs.length, 2);
    assert.equal(inputs[0]!.accessibilityLabel, 'Title');
    assert.equal(inputs[1]!.accessibilityLabel, 'Title');
    assert.equal(typeof inputs[0]!.testID, 'string');
    assert.ok(inputs[0]!.testID!.length > 0);
    assert.equal(typeof inputs[1]!.testID, 'string');
    assert.ok(inputs[1]!.testID!.length > 0);
    assert.notEqual(inputs[0]!.testID, inputs[1]!.testID);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('actual Field keeps its identifier through value, label and unrelated rerenders', async () => {
  const f = fixture();
  const onChangeText = () => {};
  const base = {
    key: 'same-instance',
    label: 'Title',
    value: '',
    onChangeText,
    placeholder: 'Page title',
    accessibilityHint: 'Document name',
    style: { marginTop: 17 },
  };
  try {
    await f.render(base);
    const original = f.inputs()[0]!;
    assert.equal(typeof original.testID, 'string');
    assert.ok(original.testID!.length > 0);
    for (const patch of [
      { value: 'Typed title' },
      { value: 'Typed title', label: 'Nome' },
      { value: 'Typed title', label: 'Nome', editable: false },
    ]) {
      await f.render({ ...base, ...patch });
      const current = f.inputs()[0]!;
      assert.equal(current.testID, original.testID);
      assert.equal(current.value, patch.value);
      assert.equal(current.accessibilityLabel, patch.label ?? base.label);
      assert.equal(current.accessibilityHint, base.accessibilityHint);
      assert.equal(current.placeholder, base.placeholder);
      assert.equal(current.onChangeText, onChangeText);
      assert.equal(JSON.stringify(current.style), JSON.stringify(original.style));
    }
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('actual Field preserves an explicit testID and its generated fallback identity', async () => {
  const f = fixture();
  const base = { key: 'same-instance', label: 'Title', value: '' };
  try {
    await f.render(base);
    const generated = f.inputs()[0]!.testID;
    assert.equal(typeof generated, 'string');
    await f.render({ ...base, testID: 'specific-page-title' });
    assert.equal(f.inputs()[0]!.testID, 'specific-page-title');
    await f.render({ ...base, label: 'Renamed', value: 'Typed', testID: 'specific-page-title' });
    assert.equal(f.inputs()[0]!.testID, 'specific-page-title');
    await f.render(base);
    assert.equal(f.inputs()[0]!.testID, generated);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
