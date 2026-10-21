import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as presentation from './telegram-composer-presentation.ts';
import { resolveSkyTheme } from '../theme/sky.ts';

type Element = { type: any; props: Record<string, any> };
const sources = await Promise.all(
  ['composer-glass.ios.tsx', 'composer-glass-fallback.tsx', 'composer-control-surface.tsx'].map(
    async (file) => {
      const output = await build({
        stdin: {
          contents: readFileSync(new URL(`../components/${file}`, import.meta.url), 'utf8'),
          loader: 'tsx',
        },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'native-ports',
            setup(b) {
              b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return output.outputFiles[0]!.text;
    },
  ),
);
function fixture(version: number | null = 1) {
  const registration: string[] = [];
  const theme = { ...resolveSkyTheme('night', new Date(2026, 9, 21, 23)), reducedMotion: false };
  const ports: Record<string, any> = {
    'react/jsx-runtime': {
      jsx: (type: any, props: any): Element => ({ type, props }),
      jsxs: (type: any, props: any): Element => ({ type, props }),
    },
    expo: {
      requireOptionalNativeModule: (name: string) => {
        assert.equal(name, 'ZoenCore');
        return version === null ? null : { composerGlassVersion: version };
      },
      requireNativeView: (name: string, view: string) => {
        assert.equal(name, 'ZoenCore');
        registration.push(view);
        return view;
      },
    },
    'react-native': { View: 'View' },
    'expo-image': { Image: 'Image' },
    '../theme': { useTheme: () => theme },
    '../core/telegram-composer-presentation': presentation,
    './primitives': { Icon: 'Icon' },
  };
  const load = (index: number) => {
    const module = { exports: {} as Record<string, any> };
    runInNewContext(sources[index]!, {
      exports: module.exports,
      module,
      process: { env: { EXPO_OS: 'ios' } },
      require: (name: string) => {
        assert.ok(name in ports, name);
        return ports[name];
      },
    });
    return module.exports;
  };
  const fallback = load(1);
  ports['./composer-glass-fallback'] = fallback;
  const native = load(0);
  ports['./composer-glass'] = native;
  const controls = load(2);
  const expand = (tree: Element): Element =>
    typeof tree.type === 'function' ? expand(tree.type(tree.props)) : tree;
  return {
    theme,
    registration,
    surface: (props: Record<string, unknown>) => expand(native.ComposerGlassSurface(props)),
    container: (props: Record<string, unknown>) => expand(native.ComposerGlassContainer(props)),
    control: (props: Record<string, unknown>) => expand(controls.ComposerControlSurface(props)),
    fallback: (props: Record<string, unknown>) => fallback.ComposerGlassSurface(props),
  };
}

test('one original native glass host preserves the React editor and props across theme, font and text changes', () => {
  const f = fixture();
  const editor = {
    type: 'ActualComposerTextField',
    props: { ref: {}, onFocus() {}, onChangeText() {} },
  };
  const first = f.surface({
    cornerRadius: 20,
    children: editor,
    testID: 'field',
    style: { width: 260 },
  });
  assert.equal(first.type, 'ComposerGlassSurface');
  assert.equal(first.props.children, editor);
  assert.equal(first.props.preference, 'panel');
  assert.equal(first.props.interactive, true);
  assert.equal(first.props.isDark, true);
  assert.equal(first.props.collapsable, false);
  Object.assign(f.theme, resolveSkyTheme('day', new Date(2026, 9, 21, 12)));
  const second = f.surface({ cornerRadius: 23, children: editor });
  assert.equal(second.type, first.type, 'style changes must not replace the native field host');
  assert.equal(second.props.children, editor);
  assert.equal(second.props.cornerRadius, 23);
  assert.equal(second.props.isDark, false);
  assert.deepEqual(f.registration, ['ComposerGlassSurface', 'ComposerGlassContainer']);
});

test('field and control use the same adaptive panel/clear contract and original container', () => {
  const f = fixture();
  f.theme.sky.top = '#FF0000';
  f.theme.sky.middle = '#00FF00';
  f.theme.sky.bottom = '#0000FF';
  const field = f.surface({ cornerRadius: 20, children: 'editor' });
  const control = f.control({ children: 'glyph' });
  assert.equal(field.props.preference, 'clear');
  assert.equal(control.type, field.type);
  assert.equal(control.props.preference, 'clear');
  assert.equal(control.props.cornerRadius, 20);
  assert.equal(control.props.style.width, 40);
  assert.equal(control.props.style.height, 40);
  assert.equal(control.props.children, 'glyph');
  const row = f.container({ children: [field, control], style: { gap: 6 } });
  assert.equal(row.type, 'ComposerGlassContainer');
  assert.equal(row.props.isDark, true);
  assert.equal(row.props.children[0], field);
  f.theme.scheme = 'light';
  assert.equal(f.surface({ cornerRadius: 20 }).props.preference, 'panel');
  assert.equal(f.surface({ cornerRadius: 20, preference: 'clear' }).props.preference, 'clear');
});

for (const version of [null, 0, 2])
  test(`unavailable native renderer (${version}) keeps children and existing semantic fallback without another glass effect`, () => {
    const f = fixture(version);
    const field = f.surface({ children: 'editor', cornerRadius: 23 });
    assert.equal(field.type, 'View');
    assert.equal(field.props.children, 'editor');
    assert.equal(field.props.style[0].backgroundColor, f.theme.presentation.chat.input.fill);
    assert.equal(field.props.style[0].borderWidth, 0.5);
    assert.equal(field.props.style[0].borderRadius, 23);
    assert.equal(f.control({ children: 'glyph' }).props.style[1].borderWidth, 0);
    assert.equal(f.container({ children: 'same subtree' }).type, 'View');
    assert.deepEqual(f.registration, []);
  });

test('gradient saturation follows the pinned brightness penalty, hue wrap and diversity threshold', () => {
  assert.equal(presentation.composerWallpaperSaturation([]), 0);
  assert.equal(presentation.composerWallpaperSaturation(['invalid']), 0);
  assert.equal(presentation.composerWallpaperSaturation(['#FFFFFF', '#000000']), 0);
  assert.equal(
    presentation.composerWallpaperSaturation(['#FF0000']),
    0,
    'one very bright hue retains the source brightness penalty',
  );
  assert.equal(presentation.composerWallpaperSaturation(['#FF0000', '#00FF00']), 1);
  assert.equal(
    presentation.composerWallpaperSaturation(['#FF0000', '#FF0001']),
    0,
    'hues adjacent across the circular wrap remain similar',
  );
  assert.ok(presentation.composerWallpaperSaturation(['#800000']) > 0.99);
  for (const profile of ['day', 'night'] as const) {
    const { sky } = resolveSkyTheme(profile, new Date(2026, 9, 21, 12));
    assert.ok(presentation.composerWallpaperSaturation([sky.top, sky.middle, sky.bottom]) <= 0.35);
  }
});
