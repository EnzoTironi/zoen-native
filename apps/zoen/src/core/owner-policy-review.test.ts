import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as presentation from '../agents/owner-policy-presentation.ts';

const digest = 'ab'.repeat(32);
const review = {
  currency: 'USD',
  scale: 6,
  year: 2026,
  month: 10,
  limit_units: '9223372036854775807',
  max_attempt_units: '11',
  expires_at_ms: '1793491200000',
  enabled: true,
  allowed_profiles: [digest],
};
const profiles = [{ digest, label: 'Image generation' }];

test('budget amounts preserve signed micro-units beyond floating-point integer precision', () => {
  assert.equal(
    presentation.policyAmount(review.limit_units, 'USD', 6, 'en-US'),
    '$9,223,372,036,854.775807',
  );
  assert.equal(presentation.policyAmount('11', 'USD', 6, 'en-US'), '$0.000011');
  assert.equal(presentation.policyAmount('1000001', 'USD', 6, 'pt-BR'), 'US$ 1,000001');
  assert.equal(presentation.policyAmount('0', 'USD', 6, 'en-US'), '$0.00');
  for (const units of ['-1', '1e6', '1.2', '01', '9223372036854775808'])
    assert.equal(presentation.policyAmount(units, 'USD', 6, 'en-US'), null);
});

test('native presentation does not require formatToParts or Intl BigInt support', (context) => {
  context.mock.method(Intl.NumberFormat.prototype, 'formatToParts', () => {
    throw new Error('formatToParts unavailable on native');
  });
  const NativeFormat = Intl.NumberFormat;
  context.mock.method(
    Intl,
    'NumberFormat',
    function (...args: ConstructorParameters<typeof Intl.NumberFormat>) {
      const formatter = new NativeFormat(...args);
      return {
        format(value: number | bigint) {
          assert.equal(typeof value, 'number');
          return formatter.format(value);
        },
      } as Intl.NumberFormat;
    },
  );
  assert.equal(
    presentation.policyAmount(review.limit_units, 'USD', 6, 'en-US'),
    '$9,223,372,036,854.775807',
  );
  assert.equal(presentation.policyAmount('11', 'USD', 6, 'pt-BR'), 'US$ 0,000011');
});

test('every allowed profile must have one unambiguous display name before confirmation', () => {
  assert.ok(presentation.policyPresentation(review, profiles, 'en-US'));
  for (const labels of [
    [],
    [...profiles, ...profiles],
    [{ digest: 'cd'.repeat(32), label: 'Other' }],
    [{ digest, label: ' ' }],
  ])
    assert.equal(presentation.policyPresentation(review, labels, 'en-US'), null);
  assert.equal(
    presentation.policyPresentation(
      { ...review, allowed_profiles: [digest, digest] },
      profiles,
      'en-US',
    ),
    null,
  );
  for (const patch of [
    { max_attempt_units: '9223372036854775808' },
    { limit_units: '10' },
    { expires_at_ms: '9007199254740992' },
    { month: 0 },
    { year: 1999 },
  ])
    assert.equal(presentation.policyPresentation({ ...review, ...patch }, profiles, 'en-US'), null);
});

const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../agents/owner-policy-review.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  external: ['*'],
});
const module = { exports: {} as any };
runInNewContext(compiled.outputFiles[0]!.text, {
  module,
  require(name: string) {
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: unknown, props: unknown) => ({ type, props }),
        jsxs: (type: unknown, props: unknown) => ({ type, props }),
      };
    if (name === 'react-native') return { Text: 'Text', View: 'View' };
    if (name === '../components/primitives')
      return { NativeButton: 'NativeButton', Surface: 'Surface' };
    if (name === '../i18n/index.ts') return { t: (key: string) => key, formatLocale: 'en-US' };
    if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
    if (name === './owner-policy-presentation') return presentation;
    throw new Error(`Unexpected module ${name}`);
  },
});
function nodes(value: any): any[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  return value?.props ? [value, ...nodes(value.props.children)] : [];
}
function render(patch: Record<string, unknown> = {}) {
  return nodes(
    module.exports.OwnerPolicyReview({
      review,
      profiles,
      phase: 'ready',
      active: true,
      busy: false,
      onConfirm() {},
      ...patch,
    }),
  );
}

test('compiled review identifies account-wide scope and only explicit ready confirmation can submit', () => {
  let submissions = 0;
  const tree = render({ onConfirm: () => submissions++ });
  assert.ok(tree.some((node) => node.props.children === 'ownerPolicy.ownerScope'));
  const button = tree.find((node) => node.type === 'NativeButton');
  assert.equal(button.props.label, 'ownerPolicy.confirm');
  assert.equal(button.props.disabled, false);
  button.props.onPress();
  assert.equal(submissions, 1);
  for (const patch of [{ active: false }, { busy: true }, { profiles: [] }]) {
    const blocked = render({ ...patch, onConfirm: () => submissions++ }).find(
      (node) => node.type === 'NativeButton',
    );
    assert.equal(blocked.props.disabled, true);
    blocked.props.onPress();
    assert.equal(submissions, 1);
  }
});

test('queued and refused submissions cannot appear installed or emit another policy', () => {
  for (const [phase, status] of [
    ['pending', 'ownerPolicy.pending'],
    ['refused', 'ownerPolicy.refused'],
    ['delivery_failed', 'ownerPolicy.deliveryFailed'],
    ['installed', 'ownerPolicy.installed'],
  ]) {
    const tree = render({ phase, onConfirm: () => assert.fail('non-ready submission') });
    assert.equal(
      tree.some((node) => node.type === 'NativeButton'),
      false,
    );
    assert.ok(tree.some((node) => node.props.children === status));
    assert.equal(
      tree.some((node) => node.props.children === 'ownerPolicy.installed'),
      phase === 'installed',
    );
  }
  const disabled = render({ review: { ...review, enabled: false } });
  assert.ok(disabled.some((node) => node.props.children === 'ownerPolicy.disabled'));
  assert.equal(
    disabled.find((node) => node.type === 'NativeButton').props.label,
    'ownerPolicy.disable',
  );
});
