import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { transform } from 'esbuild';

const route = await transform(
  readFileSync(new URL('../app/owner-budget/[id].tsx', import.meta.url), 'utf8'),
  { loader: 'tsx', format: 'cjs', jsx: 'automatic' },
);

test('request budget only routes to a policy for a visible conversation', () => {
  const jsx = (type: string, props: Record<string, unknown>) => ({ type, props });
  const request = {
    id: 'request',
    space_id: 'space',
    agent: { id: 'agent', is_mine: true },
    tool_approval: { canonical_tool: 'generate_image', image: {}, content_hash: 'hash' },
  };
  let conversations: { id: string; kind: string }[] = [];
  const ports: Record<string, unknown> = {
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'expo-router': { Stack: { Screen: 'Screen' }, useLocalSearchParams: () => ({ id: 'request' }) },
    '../../core/query': { useCoreQuery: () => ({ data: [request] }) },
    '../../components/primitives': { EmptyState: 'EmptyState' },
    '../../core/provider': { useClient: () => ({ snapshot: { conversations } }) },
    '../../agents/account-budget-screen': { AccountBudgetScreen: 'AccountBudgetScreen' },
    '../../agents/owner-policy-screen': { OwnerPolicyScreen: 'OwnerPolicyScreen' },
    '../../core/image-tool-review': { imageToolReviewValid: () => true },
    '../../i18n/index.ts': { t: (key: string) => key },
  };
  const module = { exports: {} as { default: () => ReturnType<typeof jsx> } };
  runInNewContext(route.code, { module, require: (name: string) => ports[name] });
  const render = module.exports.default;
  assert.equal(render().type, 'EmptyState');
  assert.equal(render().props.title, 'ownerPolicy.unavailable');
  conversations = [{ id: 'space', kind: 'Group' }];
  assert.equal(render().type, 'AccountBudgetScreen');
  conversations = [{ id: 'space', kind: 'Direct' }];
  assert.equal((render().props.children as ReturnType<typeof jsx>[])[1]?.type, 'OwnerPolicyScreen');
});
