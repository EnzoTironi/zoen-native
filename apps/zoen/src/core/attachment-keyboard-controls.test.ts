import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
const code = (
  await build({
    stdin: {
      contents: readFileSync(new URL('../media/media-actions.ios.tsx', import.meta.url), 'utf8'),
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'native-attachment-button-ports',
        setup(b) {
          b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;
function fixture() {
  let next = 0,
    calls = 0;
  const session = new AttachmentKeyboardSession('space', true, () => `id-${++next}`);
  session.bind({ tag: 7, context: 'space:normal', label: 'Message', placeholder: '' });
  let resolve!: () => void;
  const pending = new Promise<void>((yes) => {
    resolve = yes;
  });
  const choose = async () => {
    calls++;
    await pending;
  };
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  const modifiers = new Proxy({}, { get: (_o, name) => (value: unknown) => ({ name, value }) });
  const ports: Record<string, unknown> = {
    react: {
      useSyncExternalStore: (_s: unknown, get: () => unknown) => get(),
      useLayoutEffect: (run: () => void) => run(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    '@expo/ui': { Host: 'Host' },
    '@expo/ui/swift-ui': { Button: 'Button', Menu: 'Menu' },
    '@expo/ui/swift-ui/modifiers': modifiers,
    'expo-glass-effect': { isLiquidGlassAvailable: () => true },
    './attachment-keyboard-context': { useAttachmentKeyboardSession: () => session },
    '../i18n/index.ts': { t: (value: string) => value },
    '../components/composer-action': { ComposerAction: 'ComposerAction' },
    '../theme': {
      useTheme: () => ({
        scheme: 'dark',
        colors: {},
        presentation: { chat: { input: { control: '#fff' } } },
      }),
    },
  };
  const module = { exports: {} as any };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  const render = (disabled: boolean) => {
    const element = module.exports.MediaActions({
      disabled,
      hasVoiceEditor: true,
      onChoose: choose,
    });
    assert.equal(element.type, 'ComposerAction');
    return element.props;
  };
  const blocked = (props: any) => props.disabled;
  return { session, render, blocked, calls: () => calls, resolve };
}
test('actual current attachment X remains an enabled close while picker marks composer busy', () => {
  const f = fixture();
  f.render(false);
  f.session.openAttachments();
  f.session.admitPicker(f.session.getSnapshot().presentation!.id);
  const busy = f.render(true);
  assert.equal(busy.name, 'xmark');
  assert.equal(busy.label, 'common.close');
  assert.equal(f.blocked(busy), false);
  busy.onPress();
  assert.equal(f.session.getSnapshot().presentation?.closing, true);
  assert.equal(f.blocked(f.render(true)), true);
  f.session.retire();
  f.resolve();
});
test('a retained fresh plus cannot open after real disabled registration or route retirement', () => {
  const f = fixture(),
    old = f.render(false);
  assert.equal(old.name, 'plus');
  assert.equal(old.label, 'media.addAttachment');
  f.render(true);
  old.onPress();
  assert.equal(f.calls(), 0);
  f.render(false);
  f.session.configure('space', false);
  old.onPress();
  assert.equal(f.calls(), 0);
  assert.equal(f.blocked(f.render(false)), true);
  f.resolve();
});
