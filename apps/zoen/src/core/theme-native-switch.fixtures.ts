import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';

type Element = { type: unknown; props: Record<string, any> };
const sdk = new URL('../../node_modules/@expo/ui/src/', import.meta.url);
const result = await build({
  entryPoints: [fileURLToPath(new URL('universal/Switch/index.ios.tsx', sdk))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'installed-native-switch-leaves',
      setup(builder) {
        builder.onResolve({ filter: /^@expo\/ui\/swift-ui$/ }, () => ({
          path: fileURLToPath(new URL('swift-ui/Toggle/index.tsx', sdk)),
        }));
        builder.onResolve(
          { filter: /^(expo|react\/jsx-runtime|@expo\/ui\/swift-ui\/modifiers)$/ },
          ({ path }) => ({ path, external: true }),
        );
      },
    },
  ],
});
const element = (type: unknown, props: Record<string, unknown>): Element => ({ type, props });
const ports: Record<string, unknown> = {
  expo: {
    requireNativeView(module: string, name: string) {
      assert.equal(module, 'ExpoUI');
      assert.equal(name, 'ToggleView');
      return 'ExpoUIToggleView';
    },
  },
  'react/jsx-runtime': { jsx: element, jsxs: element },
  '@expo/ui/swift-ui/modifiers': {
    disabled: (value: boolean) => ({ $type: 'disabled', disabled: value }),
  },
};
const module = { exports: {} as { Switch(props: Record<string, any>): Element } };
runInNewContext(result.outputFiles[0]!.text, {
  module,
  exports: module.exports,
  require: (name: string) => {
    assert.ok(name in ports, name);
    return ports[name];
  },
});
/** Actual installed universal Switch and SwiftUI Toggle JS; only Fabric's native endpoint is a port. */
export function nativeSwitch(props: Record<string, any>): Element {
  let node = module.exports.Switch(props);
  while (typeof node.type === 'function') node = node.type(node.props);
  assert.equal(node.type, 'ExpoUIToggleView');
  return node;
}
