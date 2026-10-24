import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const output = await build({
  stdin: {
    contents: readFileSync(
      new URL('../components/navigation-container.ios.tsx', import.meta.url),
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
      name: 'actual-metrics-wrapper',
      setup(b) {
        b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
type Element = { type: string; props: Record<string, any> };
function wrapper(native: boolean, metricsVersion?: number) {
  const module = { exports: {} as { NavigationContainer(props: unknown): Element } };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react/jsx-runtime')
        return { jsx: (type: string, props: Element['props']) => ({ type, props }) };
      if (name === 'react-native') return { View: 'View' };
      if (name === 'expo')
        return {
          requireOptionalNativeModule: () => ({
            navigationContainerVersion: native ? 1 : undefined,
            navigationMetricsVersion: metricsVersion,
          }),
          requireNativeView: () => 'NativeNavigationContainer',
        };
      assert.fail(`Unexpected native-wrapper import: ${name}`);
    },
  });
  return module.exports.NavigationContainer;
}

test('actual iOS wrapper forwards current observer/ref/tag and drops a queued foreign binding', () => {
  const calls: unknown[] = [];
  const ref = { current: null };
  const metrics = { binding: 'current', ref, changed: (packet: unknown) => calls.push(packet) };
  const node = wrapper(true, 1)({ scrollViewTag: 123, metrics });
  assert.equal(node.type, 'NativeNavigationContainer');
  assert.equal(node.props.ref, ref);
  assert.equal(node.props.scrollViewTag, 123);
  assert.equal(node.props.metricsBinding, 'current');
  node.props.onMetricsChanged({ nativeEvent: { binding: 'old', sequence: '99' } });
  assert.deepEqual(calls, []);
  const current = { binding: 'current', sequence: '1' };
  node.props.onMetricsChanged({ nativeEvent: current });
  assert.equal(calls[0], current);
  assert.equal(
    node.props.metrics,
    undefined,
    'the controller object is not passed as a native prop',
  );
});

test('old native versions receive none of the new unsupported metrics props or ref', () => {
  const node = wrapper(true)({
    scrollViewTag: 123,
    metrics: { binding: 'current', ref: {}, changed: assert.fail },
  });
  assert.equal(node.type, 'NativeNavigationContainer');
  for (const prop of ['ref', 'metricsBinding', 'onMetricsChanged', 'metrics'])
    assert.equal(Object.hasOwn(node.props, prop), false, prop);
});

test('missing native support keeps the ordinary View without leaking observer controls', () => {
  const node = wrapper(false)({
    scrollViewTag: 123,
    metrics: { binding: 'current', ref: {}, changed: assert.fail },
    testID: 'header',
  });
  assert.equal(node.type, 'View');
  assert.equal(node.props.testID, 'header');
  assert.equal(node.props.metrics, undefined);
  assert.equal(node.props.scrollViewTag, undefined);
});
