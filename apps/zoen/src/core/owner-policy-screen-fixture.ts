import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { OwnerPolicySession, type PolicyPort, type PolicyScope } from '../owner-policy/session.ts';
import type { AgentRequestDto } from './generated/AgentRequestDto.ts';
import * as presentation from '../agents/owner-policy-presentation.ts';
import * as amounts from '../agents/owner-policy-amounts.ts';
import * as values from '../owner-policy/values.ts';
import { imageToolReviewValid } from './image-tool-review.ts';

type Context = NonNullable<ReturnType<OwnerPolicySession['getSnapshot']>['context']>;
type Review = Awaited<ReturnType<PolicyPort['prepare']>>;
type Receipt = Awaited<ReturnType<PolicyPort['submit']>>;
type VNode = { type: any; props: any; key?: string };
export const policy = 'c'.repeat(64),
  profile = 'a'.repeat(64);
export const scope: PolicyScope = {
  identity: 'owner',
  device: 'device',
  relay: 'https://relay.test',
  epoch: 1,
  spaceId: 'private-dm',
  agentId: 'agent',
  requestId: 'request',
  expectedHash: 'f'.repeat(64),
};
export const request: AgentRequestDto = {
  id: scope.requestId,
  space_id: scope.spaceId,
  tool_approval: {
    content_hash: scope.expectedHash,
    artifact_id: 'image',
    artifact_title: 'Image',
    version: 1,
    operation: 'Create image',
    expires_ms: '200000',
    canonical_tool: 'generate_image',
    image: {
      prompt: 'A quiet blue sky',
      size: '1024x1024',
      quality: 'low',
      background: 'opaque',
      definition_version: 'image-profile/1',
      price_digest: profile,
      max_image_bytes: '12582912',
      source_id: 'original-message',
      source_sha256: policy,
    },
  },
  agent: {
    id: scope.agentId,
    kind: 'Agent',
    name: 'Agent',
    handle: 'agent',
    initials: 'A',
    tint_hex: '#000000',
    glyph: null,
    bio: '',
    owner_id: scope.identity,
    owner_name: 'Owner',
    owner_tint_hex: null,
    photo_sha256: null,
    photo_ready: false,
    photo_mime: null,
    is_me: false,
    is_mine: true,
  },
  space_title: 'Private conversation',
  title: 'Image',
  detail: '',
  audience: '',
  action_label: '',
  cost_cents: null,
  reason: '',
  status: 'Pending',
  opened_ms: '100000',
  resolved_ms: null,
  item_id: null,
  line_id: null,
  action_key: '',
  can_always_approve: false,
  by_standing: false,
};
export function context(patch: Partial<Context> = {}): Context {
  return {
    options_digest: 'd'.repeat(64),
    currency: 'USD',
    scale: 6,
    year: 2026,
    month: 10,
    reservation_units: '100',
    current_limit_units: null,
    current_max_attempt_units: null,
    current_enabled: null,
    current_expired: null,
    current_expires_at_ms: null,
    current_policy_digest: null,
    existing_profile_count: 0,
    image_profile_digest: profile,
    image_size: '1024x1024',
    image_quality: 'low',
    observed_at_ms: '100000',
    ...patch,
  };
}
export function review(
  owner: PolicyScope,
  limit: string,
  attempt: string,
  enabled: boolean,
): Review {
  return {
    scope: {
      expected_identity_id: owner.identity,
      expected_device_id: owner.device,
      space_id: owner.spaceId,
      agent_id: owner.agentId,
    },
    canonical_policy_json: '{"exact":"signed Core policy"}',
    policy_digest: policy,
    confirmation_digest: 'b'.repeat(64),
    currency: 'USD',
    scale: 6,
    year: 2026,
    month: 10,
    version: 1,
    limit_units: limit,
    max_attempt_units: attempt,
    expires_at_ms: '200000',
    enabled,
    allowed_profiles: [profile],
  };
}
export async function settle() {
  for (let i = 0; i < 8; i++) await new Promise<void>((resolve) => setImmediate(resolve));
}
export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
export async function fixture() {
  const f = {
    owner: { ...scope } as PolicyScope | null,
    context: context(),
    receipt: { source: 'original-policy', policy_digest: policy, state: 'pending' } as Receipt,
    prepares: [] as unknown[][],
    submissions: [] as Review[],
    finds: 0,
    queries: 0,
  };
  const options = (source: string) => ({
    source,
    inquiry_digest: 'e'.repeat(64),
    state: 'ready',
    context: f.context,
  });
  const port: PolicyPort = {
    scope: () => f.owner,
    inquire: async () => options(`query-${++f.queries}`),
    options: async (_owner, source) => options(source),
    findOptions: async () => null,
    prepare: async (owner, source, digest, limit, attempt, enabled) => {
      f.prepares.push([owner, source, digest, limit, attempt, enabled]);
      return review(owner, limit, attempt, enabled);
    },
    submit: async (_owner, value) => {
      f.submissions.push(value);
      return f.receipt;
    },
    status: async () => f.receipt,
    find: async () => {
      f.finds++;
      return f.receipt;
    },
    changed() {},
  };
  const session = new OwnerPolicySession(port);
  session.activate(f.owner);
  await session.refresh();
  await settle();
  return { f, port, session };
}
type Fixture = Awaited<ReturnType<typeof fixture>>;

// Compile the actual composed screen, form and review. Only native host elements,
// local React state and the hook subscription are replaced; the real Session owns
// observed-state, generation, confirmation and reconciliation decisions.
const sourcePaths = [
  '../agents/owner-policy-form.tsx',
  '../agents/owner-policy-review.tsx',
  '../agents/owner-policy-screen.tsx',
  '../app/owner-budget/[id].tsx',
];
const nativeRequire = createRequire(import.meta.url);
const { transformSync } = nativeRequire('@babel/core');
const bundles = await Promise.all(
  sourcePaths.map((path) => {
    const filename = fileURLToPath(new URL(path, import.meta.url));
    const { code } = transformSync(readFileSync(filename, 'utf8'), {
      filename,
      configFile: false,
      babelrc: false,
      presets: [nativeRequire.resolve('babel-preset-expo')],
      caller: {
        name: 'metro',
        platform: 'ios',
        engine: 'hermes',
        isDev: false,
        supportsReactCompiler: true,
      },
    });
    assert.match(code, /react\/compiler-runtime/, 'Exercise installed iOS React compiler');
    return build({
      stdin: { contents: code, loader: 'js' },
      bundle: true,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
      external: ['*'],
    });
  }),
);
export function screen(f: Fixture, mode: 'image' | 'account' = 'image') {
  const slots = new Map<string, any[]>();
  const caches = new Map<string, unknown[][]>();
  let activeSlots: any[] = [],
    activeCaches: unknown[][] = [],
    cursor = 0,
    compilerCursor = 0;
  const components: Record<string, any> = {};
  let routeRows: AgentRequestDto[] = [request];
  let routeSpaces: { id: string; kind: string }[] = [];
  const accountTarget = { purpose: 'account', spaceId: scope.spaceId, agentId: scope.agentId };
  const require = (name: string) => {
    if (name === 'react/compiler-runtime')
      return {
        c: (size: number) =>
          (activeCaches[compilerCursor++] ??= Array(size).fill(
            Symbol.for('react.memo_cache_sentinel'),
          )),
      };
    if (name.startsWith('@babel/runtime/helpers/')) return nativeRequire(name);
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: unknown, props: unknown, key?: string) => ({ type, props, key }),
        jsxs: (type: unknown, props: unknown, key?: string) => ({ type, props, key }),
        Fragment: 'Fragment',
      };
    if (name === 'react')
      return {
        useState(initial: any) {
          const state = activeSlots,
            index = cursor++;
          if (!(index in state)) state[index] = typeof initial === 'function' ? initial() : initial;
          return [
            state[index],
            (value: any) => {
              state[index] = value;
            },
          ];
        },
      };
    if (name === 'expo-router')
      return {
        Stack: { Screen: 'Stack.Screen' },
        useLocalSearchParams: () => ({ id: scope.requestId }),
      };
    if (name === 'react-native') return { ScrollView: 'ScrollView', Text: 'Text', View: 'View' };
    if (name === '@expo/ui') return { Host: 'Host', Switch: 'Switch' };
    if (name === '../../core/provider')
      return { useClient: () => ({ snapshot: { conversations: routeSpaces } }) };
    // The alternative account selector is qualified by its own real component/port cases.
    if (name === '../../agents/account-budget-screen')
      return { AccountBudgetScreen: 'AccountBudgetScreen' };
    if (name === '../../core/query') return { useCoreQuery: () => ({ data: routeRows }) };
    if (name === '../../core/image-tool-review') return { imageToolReviewValid };
    if (name === '../../agents/owner-policy-screen') return components.screen;
    if (name === '../components/primitives' || name === '../../components/primitives')
      return {
        Field: 'Field',
        NativeButton: 'NativeButton',
        Surface: 'Surface',
        EmptyState: 'EmptyState',
        Loading: 'Loading',
      };
    if (name === '../theme') return { useTheme: () => ({ colors: {}, scheme: 'light' }) };
    if (name === '../i18n/index.ts' || name === '../../i18n/index.ts')
      return { t: (key: string) => key, locale: 'en', formatLocale: 'en-US' };
    if (name === '../owner-policy/use-owner-policy')
      return {
        useOwnerPolicy: (target: unknown) => {
          assert.deepEqual(
            JSON.parse(JSON.stringify(target)),
            mode === 'account'
              ? accountTarget
              : {
                  spaceId: scope.spaceId,
                  agentId: scope.agentId,
                  requestId: scope.requestId,
                  expectedHash: scope.expectedHash,
                },
          );
          return { session: f.session, state: f.session.getSnapshot(), unavailableReason: null };
        },
      };
    if (name === './owner-policy-presentation') return presentation;
    if (name === './owner-policy-amounts') return amounts;
    if (name === '../owner-policy/values') return values;
    if (name === './owner-policy-form') return components.form;
    if (name === './owner-policy-review') return components.review;
    throw Error(`Unexpected composed screen dependency ${name}`);
  };
  for (let i = 0; i < bundles.length; i++) {
    const module = { exports: {} as any };
    runInNewContext(bundles[i]!.outputFiles[0]!.text, {
      module,
      exports: module.exports,
      require,
    });
    components[['form', 'review', 'screen', 'route'][i]!] = module.exports;
  }
  function expand(value: any, path: string): VNode[] {
    if (Array.isArray(value))
      return value.flatMap((child, index) => expand(child, `${path}/${index}`));
    if (!value?.props) return [];
    if (typeof value.type === 'function') {
      const id = `${path}/${value.type.name}/${value.key ?? ''}`;
      activeSlots = slots.get(id) ?? [];
      slots.set(id, activeSlots);
      activeCaches = caches.get(id) ?? [];
      caches.set(id, activeCaches);
      cursor = 0;
      compilerCursor = 0;
      return expand(value.type(value.props), id);
    }
    return [value, ...expand(value.props.children, path)];
  }
  const render = () =>
    expand(
      mode === 'account'
        ? { type: components.screen.OwnerPolicyControllerScreen, props: { target: accountTarget } }
        : { type: components.screen.OwnerPolicyScreen, props: { request } },
      'root',
    );
  return Object.assign(render, {
    route(rows: AgentRequestDto[], spaces: { id: string; kind: string }[] = []) {
      routeRows = rows;
      routeSpaces = spaces;
      return expand({ type: components.route.default, props: {} }, 'route');
    },
  });
}
export const button = (tree: VNode[], label: string) => {
  const result = tree.find((node) => node.type === 'NativeButton' && node.props.label === label);
  assert.ok(result, `Missing actual button ${label}`);
  return result;
};
export const has = (tree: VNode[], text: string) =>
  tree.some((node) => node.props.children === text);
export function fill(render: ReturnType<typeof screen>, limit = '1', attempt = '0.000100') {
  const fields = render().filter((node) => node.type === 'Field');
  assert.equal(fields.length, 2);
  fields[0]!.props.onChangeText(limit);
  fields[1]!.props.onChangeText(attempt);
}
export async function prepare(f: Fixture, render: ReturnType<typeof screen>) {
  fill(render);
  button(render(), 'ownerPolicy.review').props.onPress();
  await settle();
  assert.equal(f.session.getSnapshot().phase, 'review');
}
