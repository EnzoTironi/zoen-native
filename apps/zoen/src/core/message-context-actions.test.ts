import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { MessageContextPresentation } from '../components/message-context-presentation.ts';

const compiled = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../components/message-context-actions.ios.tsx', import.meta.url),
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
        name: 'native-context-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;
const deferred = <T>() => {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const tick = async () => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};
function fixture(
  options: {
    mine?: boolean;
    deleted?: boolean;
    selected?: boolean;
    pending?: boolean;
    row?: boolean;
    recover?: boolean;
  } = {},
) {
  const result = deferred<string | null>(),
    dismissal = deferred<void>();
  const calls: string[] = [],
    configurations: any[] = [];
  const layouts: (() => void)[] = [];
  const effects: (() => void | (() => void))[] = [],
    cleanups: (() => void)[] = [];
  let failed = false,
    live = options.row !== false;
  let deps: unknown[] | undefined;
  const refs: { current: unknown }[] = [];
  let cursor = 0;
  const request = {
    binding: 'binding',
    gestureId: 'gesture',
    kind: 'hold',
    current: () => live,
    native: {
      presentContext: (_id: string, _binding: string, _gesture: string, data: string) => {
        configurations.push(JSON.parse(data));
        return result.promise;
      },
      dismissContext: () => {
        calls.push('native-dismiss');
        return dismissal.promise;
      },
    },
  };
  let props: any = {
    entry: {
      id: 'message',
      author: { is_me: options.mine ?? true },
      deleted: options.deleted ?? false,
      delivery: options.recover ? 'Failed' : 'Sent',
      reactions: options.selected ? [{ emoji: '❤️', mine: true }] : [],
    },
    nativeSource: request,
    mode: 'actions',
    pending: options.pending ?? false,
    onClose: () => calls.push('close'),
    onReact: (value: string | null) => calls.push(`react:${value}`),
    onMore: () => calls.push('more'),
    onReply: () => calls.push('reply'),
    onThread: () => calls.push('thread'),
    onForward: () => calls.push('forward'),
    onCopy: () => calls.push('copy'),
    onShare: () => calls.push('share'),
    onEdit: options.recover ? undefined : () => calls.push('edit'),
    onRecover: options.recover ? () => calls.push('recover') : undefined,
    onDelete: () => calls.push('delete'),
  };
  const theme = {
    background: '#111111',
    highlight: '#EEEEEE',
    text: '#FFFFFF',
    secondary: '#AAAAAA',
    separator: '#999999',
    destructive: '#FF0000',
    dim: '#00000080',
    isDark: true,
  };
  const module = { exports: {} as { ExtractedMessageActions(props: any): unknown } };
  const ports: Record<string, unknown> = {
    react: {
      useRef: (value: unknown) => (refs[cursor++] ??= { current: value }),
      useLayoutEffect: (effect: () => void) => layouts.push(effect),
      useState: () => [
        failed,
        (value: boolean) => {
          failed = value;
        },
      ],
      useEffect: (effect: () => void | (() => void), next: unknown[]) => {
        if (!deps || next.some((value, index) => value !== deps![index])) {
          effects.push(effect);
          deps = next;
        }
      },
    },
    'expo-crypto': { randomUUID: () => 'request' },
    '../i18n/index.ts': { t: (key: string) => key },
    '../theme': { useTheme: () => ({ presentation: { context: theme }, reducedMotion: true }) },
    './message-context-presentation': { MessageContextPresentation },
  };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  const render = (patch: object = {}) => {
    props = { ...props, ...patch };
    cursor = 0;
    const tree = module.exports.ExtractedMessageActions({
      props,
      fallback: (value: any) => ({ fallback: value }),
    });
    layouts.splice(0).forEach((effect) => effect());
    while (effects.length) {
      const cleanup = effects.shift()!();
      if (cleanup) cleanups.push(cleanup);
    }
    return tree;
  };
  return {
    render,
    calls,
    result,
    dismissal,
    configurations,
    theme,
    request,
    revoke: () => {
      live = false;
    },
    unmount: () => {
      cleanups.forEach((cleanup) => cleanup());
    },
  };
}

test('actual compiled menu offers only supplied real actions and receives the shared theme unchanged', async () => {
  const f = fixture({ mine: false });
  f.render();
  await tick();
  assert.deepEqual(
    f.configurations[0].actions.map((value: any) => value.id),
    ['reply', 'thread', 'forward', 'copy', 'share'],
  );
  assert.deepEqual(f.configurations[0].theme, f.theme);
  assert.equal(f.configurations[0].reducedMotion, true);
  f.result.resolve(null);
  await tick();
});
test('deleted messages cannot gain forward/edit/delete capabilities from native UI', async () => {
  const f = fixture({ deleted: true });
  f.render();
  await tick();
  assert.deepEqual(
    f.configurations[0].actions.map((value: any) => value.id),
    ['reply', 'thread', 'copy', 'share'],
  );
  f.result.resolve('delete');
  await tick();
  assert.deepEqual(f.calls, ['close']);
});
test('actual compiled reply waits native completion, closes selection, then calls existing callback', async () => {
  const f = fixture();
  f.render();
  await tick();
  assert.deepEqual(f.calls, []);
  f.result.resolve('reply');
  await tick();
  assert.deepEqual(f.calls, ['close', 'reply']);
});

test('actual compiled recovery action waits native removal and delegates only the supplied explicit callback', async () => {
  const f = fixture({ recover: true });
  f.render();
  await tick();
  assert.equal(
    f.configurations[0].actions.some((value: any) => value.id === 'recover'),
    true,
  );
  assert.equal(
    f.configurations[0].actions.some((value: any) => value.id === 'edit'),
    false,
  );
  assert.deepEqual(f.calls, []);
  f.result.resolve('recover');
  await tick();
  assert.deepEqual(f.calls, ['close', 'recover']);
});
test('selected reaction toggles null and leaves selection for the existing mutation success/error contract', async () => {
  const f = fixture({ selected: true });
  f.render();
  await tick();
  f.result.resolve('reaction:❤️');
  await tick();
  assert.deepEqual(f.calls, ['react:null']);
  assert.equal(
    f.configurations[0].reactions.find((value: any) => value.emoji === '❤️').selected,
    true,
  );
});
test('More is post-removal and routes to the existing complete emoji surface', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.result.resolve('more');
  await tick();
  assert.deepEqual(f.calls, ['close', 'more']);
});
for (const boundary of ['unmount', 'row', 'pending', 'entry', 'authority'] as const)
  test(`actual compiled delayed callback is fenced after ${boundary}`, async () => {
    const f = fixture();
    f.render();
    await tick();
    if (boundary === 'unmount') f.unmount();
    else if (boundary === 'row') f.revoke();
    else if (boundary === 'pending') f.render({ pending: true });
    else if (boundary === 'authority') f.render({ isCurrent: () => false });
    else f.render({ entry: null });
    f.result.resolve('reply');
    f.dismissal.resolve();
    await tick();
    assert.ok(!f.calls.includes('reply'));
  });
test('a native refusal uses safe fallback only after actual exact dismissal has drained', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.result.reject(new Error('unavailable'));
  await tick();
  assert.equal(f.render(), null);
  f.dismissal.resolve();
  await tick();
  assert.ok(f.render());
  assert.deepEqual(f.calls, ['native-dismiss']);
});
test('an existing canonical reaction error renders the accessible retry menu after native removal', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.result.resolve('reaction:❤️');
  await tick();
  const tree = f.render({ error: 'Canonical mutation failed' }) as any;
  assert.equal(tree.fallback.error, 'Canonical mutation failed');
  assert.equal(tree.fallback.nativeSource, undefined);
});

test('handler/reaction updates keep one native presentation but completion uses the latest committed callbacks', async () => {
  const f = fixture();
  f.render();
  await tick();
  f.render({ onReply: () => f.calls.push('fresh-reply') });
  assert.equal(f.configurations.length, 1);
  f.result.resolve('reply');
  await tick();
  assert.deepEqual(f.calls, ['close', 'fresh-reply']);
});
