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
