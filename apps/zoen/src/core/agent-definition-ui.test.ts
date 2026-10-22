import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { telegramPresentation } from '../theme/telegram.ts';
import { t } from '../i18n/index.ts';
import { emptyDefinitionState, type DefinitionState } from '../agent-definition/session.ts';
import type { AgentDefinitionDto } from './generated/AgentDefinitionDto.ts';
import type { AgentDefinitionPreviewDto } from './generated/AgentDefinitionPreviewDto.ts';

type Element = { type: unknown; props: Record<string, any> };
const source = new URL('../agent-definition/definition-view.tsx', import.meta.url);
const output = await build({
  stdin: { contents: readFileSync(source, 'utf8'), loader: 'tsx' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  external: ['*'],
});

function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  // Expand the production Behavior component, so text/diff assertions exercise
  // what it renders rather than merely inspecting its DTO props.
  if (typeof node.type === 'function') return nodes(node.type(node.props));
  return [node, ...nodes(node.props.children)];
}
function text(value: unknown): string {
  if (Array.isArray(value)) return value.map(text).join('');
  return typeof value === 'string' || typeof value === 'number' ? String(value) : '';
}
const before: AgentDefinitionDto = {
  agent_id: 'SYNTHETIC_AGENT_ID',
  owner_id: 'SYNTHETIC_OWNER_ID',
  device_id: 'SYNTHETIC_DEVICE_ID',
  relay_url: 'https://synthetic-relay.invalid',
  status: 'Active',
  version: 1001,
  definition_sha256: 'SYNTHETIC_BASE_HASH',
  persona: `Before: ${'A synthetic sentence with enough room to review. '.repeat(12)}END_BEFORE`,
  rules: ['Keep the first rule.', 'Remove this rule.', 'Move this rule.'],
};
const proposed: AgentDefinitionDto = {
  ...before,
  version: 1002,
  definition_sha256: 'SYNTHETIC_NEXT_HASH',
  persona: `After: ${'Another full synthetic sentence. '.repeat(18)}END_AFTER`,
  rules: ['Move this rule.', 'Keep the first rule.', 'Add this rule.'],
};
const review: AgentDefinitionPreviewDto = {
  current: before,
  proposed,
  preview_digest: 'SYNTHETIC_PREVIEW_DIGEST',
};
function state(phase: DefinitionState['phase'] = 'editing'): DefinitionState {
  return {
    ...emptyDefinitionState,
    phase,
    current: before,
    persona: before.persona,
    rules: before.rules,
    ruleKeys: [41, 73, 99],
    review: phase === 'editing' ? null : review,
  };
}
function fixture(value: DefinitionState, dirty = true, mode: 'day' | 'night' = 'night') {
  const actions: unknown[][] = [];
  let keyboardDismissals = 0;
  const props = {
    state: value,
    dirty,
    onPersona: (value: string) => actions.push(['persona', value]),
    onRule: (key: number, value: string) => actions.push(['rule', key, value]),
    onAddRule: () => actions.push(['add']),
    onRemoveRule: (key: number) => actions.push(['remove', key]),
    onPreview: () => actions.push(['preview']),
    onApply: () => actions.push(['apply']),
    onEdit: () => actions.push(['edit']),
    onReconcile: () => actions.push(['reconcile']),
    onDismissAttempt: () => actions.push(['dismiss']),
    onReload: () => actions.push(['reload']),
    onRetry: () => actions.push(['retry']),
    onDone: () => actions.push(['done']),
  };
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    require: (name: string) => {
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
        return { useTheme: () => ({ presentation: telegramPresentation(mode) }) };
      if (name === '../i18n') return { t };
      if (name === '../components/primitives')
        return { Field: 'Field', Loading: 'Loading', NativeButton: 'NativeButton' };
      if (name === '../components/settings-screen') return { SettingsScreen: 'SettingsScreen' };
      if (name === '../profile/profile-section') return { ProfileSection: 'ProfileSection' };
      throw Error(`Unexpected definition view import: ${name}`);
    },
  });
  return {
    actions,
    props,
    render: () => module.exports.DefinitionView(props) as Element,
    keyboardDismissals: () => keyboardDismissals,
  };
}
function target(tree: Element, id: string) {
  const found = nodes(tree).find((node) => node.props.testID === id);
  assert.ok(found, id);
  return found;
}
function absent(tree: Element, id: string) {
  assert.equal(
    nodes(tree).some((node) => node.props.testID === id),
    false,
    id,
  );
}
function sectionTexts(tree: Element, title: string) {
  const section = nodes(tree).find(
    (node) => node.type === 'ProfileSection' && node.props.title === title,
  );
  assert.ok(section, title);
  return nodes(section)
    .filter((node) => node.type === 'Text')
    .map((node) => text(node.props.children));
}

test('actual editor leaf handlers only request review after the explicit Review tap', () => {
  const f = fixture(state());
  const tree = f.render();
  assert.deepEqual(f.actions, []);
  absent(tree, 'agent-definition-apply');
  const persona = target(tree, 'agent-definition-persona');
  assert.equal(persona.props.onSubmitEditing, undefined);
  persona.props.onChangeText('  Synthetic pasted persona\nwith preserved whitespace.  ');
  target(tree, 'agent-definition-rule-73').props.onChangeText('Changed second rule.');
  target(tree, 'agent-definition-add-rule').props.onPress();
  const remove = nodes(tree).find(
    (node) =>
      node.type === 'NativeButton' &&
      node.props.label === t('agentDefinition.removeRule', { number: 2 }),
  );
  assert.ok(remove);
  remove.props.onPress();
  assert.deepEqual(f.actions, [
    ['persona', '  Synthetic pasted persona\nwith preserved whitespace.  '],
    ['rule', 73, 'Changed second rule.'],
    ['add'],
    ['remove', 73],
  ]);
  assert.equal(f.keyboardDismissals(), 0);
  target(tree, 'agent-definition-review').props.onPress();
  assert.deepEqual(f.actions.at(-1), ['preview']);
  assert.equal(f.keyboardDismissals(), 1);
  assert.equal(tree.props.keyboardShouldPersistTaps, 'always');
  assert.equal(tree.props.automaticallyAdjustKeyboardInsets, true);
});

test('unchanged input and pending preview disable Review while rule capacity disables Add', () => {
  assert.equal(
    target(fixture(state(), false).render(), 'agent-definition-review').props.disabled,
    true,
  );
  const pending = fixture(state('previewing'));
  assert.equal(target(pending.render(), 'agent-definition-review').props.disabled, true);
  const full = fixture({
    ...state(),
    rules: Array.from({ length: 8 }, (_, i) => `Rule ${i + 1}`),
    ruleKeys: [1, 2, 3, 4, 5, 6, 7, 8],
  });
  assert.equal(target(full.render(), 'agent-definition-add-rule').props.disabled, true);
  assert.deepEqual(pending.actions, []);
});

test('review renders full Before/After text, removals and rule order in both shipped themes', () => {
  for (const mode of ['day', 'night'] as const) {
    const f = fixture(state('review'), true, mode);
    const tree = f.render();
    assert.deepEqual(sectionTexts(tree, t('agentDefinition.before')), [
      before.persona,
      ...before.rules.map((rule, i) => `${i + 1}. ${rule}`),
    ]);
    assert.deepEqual(sectionTexts(tree, t('agentDefinition.after')), [
      proposed.persona,
      ...proposed.rules.map((rule, i) => `${i + 1}. ${rule}`),
    ]);
    for (const node of nodes(tree).filter((node) => node.type === 'Text')) {
      assert.equal(node.props.numberOfLines, undefined);
      assert.equal(node.props.ellipsizeMode, undefined);
    }
    assert.deepEqual(f.actions, []);
    target(tree, 'agent-definition-apply').props.onPress();
    assert.deepEqual(f.actions, [['apply']]);
  }
});

test('review of removing every rule explicitly renders an empty After section', () => {
  const f = fixture({
    ...state('review'),
    review: { ...review, proposed: { ...proposed, rules: [] } },
  });
  assert.deepEqual(sectionTexts(f.render(), t('agentDefinition.after')), [
    proposed.persona,
    t('agentDefinition.noRules'),
  ]);
});

test('submitting and every held outcome never re-expose Apply or editable fields', () => {
  const submitting = fixture(state('submitting'));
  absent(submitting.render(), 'agent-definition-apply');
  absent(submitting.render(), 'agent-definition-reconcile');
  for (const outcome of ['Unknown', 'Conflict', 'Confirmed'] as const) {
    const receipt = { receipt_id: 'SYNTHETIC_RECEIPT', review, outcome, current: proposed };
    const f = fixture({ ...state('held'), current: proposed, receipt, receiptKnown: true });
    const tree = f.render();
    absent(tree, 'agent-definition-apply');
    absent(tree, 'agent-definition-edit');
    assert.equal(
      nodes(tree).some((node) => node.type === 'Field'),
      false,
    );
    assert.deepEqual(sectionTexts(tree, t('agentDefinition.current')), [
      proposed.persona,
      ...proposed.rules.map((rule, i) => `${i + 1}. ${rule}`),
    ]);
    const label =
      outcome === 'Confirmed' ? 'saved' : outcome === 'Conflict' ? 'conflict' : 'unknown';
    assert.equal(
      text(target(tree, 'agent-definition-outcome').props.children),
      t(`agentDefinition.${label}`),
    );
    assert.deepEqual(f.actions, []);
    target(tree, 'agent-definition-reconcile').props.onPress();
    assert.deepEqual(f.actions, [['reconcile']]);
  }
});

test('Unknown with an unconfirmed receipt stays waiting and cannot dismiss before status is known', () => {
  const f = fixture({ ...state('held'), current: proposed, receipt: null, receiptKnown: false });
  const tree = f.render();
  assert.equal(
    text(target(tree, 'agent-definition-outcome').props.children),
    t('agentDefinition.unknown'),
  );
  assert.equal(target(tree, 'agent-definition-dismiss').props.disabled, true);
  for (const activity of [{ reconciling: true }, { dismissing: true }]) {
    const busy = fixture({ ...state('held'), receiptKnown: true, ...activity }).render();
    assert.equal(target(busy, 'agent-definition-reconcile').props.disabled, true);
    assert.equal(target(busy, 'agent-definition-dismiss').props.disabled, true);
    absent(busy, 'agent-definition-apply');
  }
});

test('revoked, unavailable, loading and failed reads keep update controls closed', () => {
  const revoked = fixture({ ...state(), current: { ...before, status: 'Revoked' } });
  const tree = revoked.render();
  assert.equal(target(tree, 'agent-definition-persona').props.editable, false);
  assert.equal(target(tree, 'agent-definition-rule-41').props.editable, false);
  absent(tree, 'agent-definition-add-rule');
  absent(tree, 'agent-definition-review');
  absent(tree, 'agent-definition-apply');
  assert.equal(
    nodes(tree).some(
      (node) =>
        node.type === 'NativeButton' &&
        node.props.label === t('agentDefinition.removeRule', { number: 1 }),
    ),
    false,
  );
  for (const phase of ['unavailable', 'loading', 'error'] as const) {
    const closed = fixture({ ...emptyDefinitionState, phase });
    const rendered = closed.render();
    assert.equal(
      nodes(rendered).some((node) => node.type === 'Field'),
      false,
    );
    absent(rendered, 'agent-definition-review');
    absent(rendered, 'agent-definition-apply');
    assert.deepEqual(closed.actions, []);
  }
});

test('visible behavior review never exposes technical locators or configuration inputs', () => {
  for (const phase of ['editing', 'review', 'held'] as const) {
    const all = nodes(fixture(state(phase)).render());
    const visible = all
      .flatMap((node) => [
        node.type === 'Text' ? text(node.props.children) : '',
        node.type === 'Field' ? node.props.value : '',
        node.props.label ?? '',
        node.props.title ?? '',
        node.props.placeholder ?? '',
      ])
      .join('\n');
    for (const hidden of [
      before.agent_id,
      before.owner_id,
      before.device_id,
      before.relay_url,
      before.definition_sha256,
      proposed.definition_sha256,
      review.preview_digest,
      String(before.version),
      String(proposed.version),
    ]) {
      assert.equal(visible.includes(hidden), false, `${phase}: ${hidden}`);
    }
    assert.deepEqual(
      all.filter((node) => node.type === 'Field').map((node) => node.props.testID),
      phase === 'editing'
        ? [
            'agent-definition-persona',
            'agent-definition-rule-41',
            'agent-definition-rule-73',
            'agent-definition-rule-99',
          ]
        : [],
    );
  }
});
