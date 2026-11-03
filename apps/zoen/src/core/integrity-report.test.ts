import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { transform } from 'esbuild';
import type { LogReport } from './generated/LogReport.ts';
import type { useIntegrityReport as IntegrityHook } from '../integrity/use-integrity-report.ts';
import { deferred } from './store.fixtures.ts';

test('refocusing integrity clears the previous report until the new verification settles', async () => {
  const { code } = await transform(
    readFileSync(new URL('../integrity/use-integrity-report.ts', import.meta.url), 'utf8'),
    { loader: 'ts', format: 'cjs' },
  );
  const states: unknown[] = [];
  let stateIndex = 0;
  let focus: () => void | (() => void) = () => {};
  let request = deferred<LogReport>();
  const client = { snapshot: {}, store: { driver: { request: () => request.promise } } };
  const module = { exports: {} };
  runInNewContext(code, {
    module,
    require(name: string) {
      switch (name) {
        case 'react':
          return {
            useCallback: (callback: unknown) => callback,
            useState(initial: unknown) {
              const index = stateIndex++;
              if (!(index in states)) states[index] = initial;
              return [
                states[index],
                (value: unknown) => {
                  states[index] = value;
                },
              ];
            },
          };
        case 'expo-router':
          return {
            useFocusEffect: (callback: typeof focus) => {
              focus = callback;
            },
          };
        case '../core/provider':
          return { useClient: () => client };
        case '../i18n':
          return { t: (key: string) => key };
        default:
          throw Error(name);
      }
    },
  });
  const { useIntegrityReport: invokeHook } = module.exports as {
    useIntegrityReport: typeof IntegrityHook;
  };
  const render = () => {
    stateIndex = 0;
    return invokeHook('chat');
  };
  render();
  const blur = focus();
  const report = {} as LogReport;
  request.resolve(report);
  await request.promise;
  assert.equal(render().data, report);

  blur?.();
  request = deferred<LogReport>();
  focus();
  assert.equal(render().data, null);
  request.resolve(report);
  await request.promise;
  assert.equal(render().data, report);
});

test('an audit response from before blur cannot replace the current page', async () => {
  const { code } = await transform(
    readFileSync(new URL('../app/integrity/[id].tsx', import.meta.url), 'utf8'),
    { loader: 'tsx', format: 'cjs', jsx: 'automatic' },
  );
  const states: unknown[] = [];
  const refs: { current: unknown }[] = [];
  let slot = 0;
  let focus: () => void | (() => void) = () => {};
  const first = deferred<{ events: { hash: string }[]; before: null }>();
  const second = deferred<{ events: { hash: string }[]; before: null }>();
  let calls = 0;
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => ({ snapshot: {} }),
    driver: { request: () => (++calls === 1 ? first.promise : second.promise) },
  };
  const module = { exports: {} };
  const jsx = (_type: unknown, props: unknown) => props;
  runInNewContext(code, {
    module,
    require(name: string) {
      switch (name) {
        case 'react':
          return {
            useCallback: (callback: unknown) => callback,
            useRef(initial: unknown) {
              return (refs[slot++] ??= { current: initial });
            },
            useState(initial: unknown) {
              const index = slot++;
              if (!(index in states)) states[index] = initial;
              return [
                states[index],
                (value: unknown) => {
                  states[index] = value;
                },
              ];
            },
          };
        case 'react/jsx-runtime':
          return { jsx, jsxs: jsx };
        case 'expo-router':
          return {
            Stack: {},
            useLocalSearchParams: () => ({ id: 'chat' }),
            useFocusEffect: (callback: typeof focus) => {
              focus = callback;
            },
          };
        case '../../core/provider':
          return { useClient: () => ({ store, snapshot: {} }) };
        case '../../core/query':
          return { useCoreQuery: () => ({ data: { events: [], before: 'cursor' } }) };
        case '../../integrity/use-integrity-report':
          return { useIntegrityReport: () => ({}) };
        case '../../theme':
          return { useTheme: () => ({ colors: {} }) };
        case '../../i18n':
          return { t: (key: string) => key };
        default:
          return {};
      }
    },
  });
  const { default: screen } = module.exports as {
    default: () => {
      children: [
        unknown,
        {
          data: { hash: string }[];
          ListFooterComponent: { children: { onPress: () => void; disabled: boolean } };
        },
      ];
    };
  };
  const render = () => {
    slot = 0;
    return screen().children[1];
  };
  render();
  const blur = focus();
  render().ListFooterComponent.children.onPress();
  blur?.();
  focus();
  render().ListFooterComponent.children.onPress();
  first.resolve({ events: [{ hash: 'old' }], before: null });
  await new Promise<void>((done) => setImmediate(done));
  assert.equal(render().data.length, 0);
  assert.equal(render().ListFooterComponent.children.disabled, true);
  second.resolve({ events: [{ hash: 'current' }], before: null });
  await new Promise<void>((done) => setImmediate(done));
  assert.equal(render().data[0]?.hash, 'current');
});
