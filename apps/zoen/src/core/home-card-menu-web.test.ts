import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { build } from 'esbuild';
import type { HomeActionsProps, HomeCardMenuProps } from '../home/menus.types';
import { resolveSkyTheme } from '../theme/sky.ts';
const require = createRequire(import.meta.url);
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../home/menus.web.tsx', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'home-menu-platform-ports',
      setup(builder) {
        builder.onResolve(
          {
            filter:
              /^(react(?:\/jsx-runtime)?|react-native|expo-router|\.\.\/theme|\.\.\/components\/primitives|\.\.\/i18n\/index\.ts)$/,
          },
          ({ path }) => ({ path, external: true }),
        );
      },
    },
  ],
});
const theme = resolveSkyTheme('night', new Date(2026, 9, 19));
const navigation: string[] = [];
const module = {
  exports: {} as {
    HomeCardMenu: (props: HomeCardMenuProps) => ReturnType<typeof createElement>;
    HomeActions: (props: HomeActionsProps) => ReturnType<typeof createElement>;
  },
};
runInNewContext(compiled.outputFiles[0]!.text, {
  module,
  exports: module.exports,
  require: (name: string) => {
    if (name === '../theme') return { useTheme: () => theme };
    if (name === 'react-native') return { View: 'div' };
    if (name === 'expo-router')
      return { router: { push: (path: string) => navigation.push(path) } };
    if (name === '../components/primitives')
      return {
        Icon: () => createElement('span'),
        NativeButton: ({ label, onPress }: { label: string; onPress: () => void }) =>
          createElement('button', { onClick: onPress }, label),
      };
    if (name === '../i18n/index.ts') return require('../i18n/index.ts');
    return require(name);
  },
});
function render(overrides: Partial<HomeCardMenuProps> = {}) {
  return renderToStaticMarkup(
    createElement(module.exports.HomeCardMenu, {
      name: 'A deliberately long app title that cannot determine the button width',
      pinned: false,
      testID: 'home-card-menu-real-id',
      onToggle: () => assert.fail('rendering cannot pin an app'),
      ...overrides,
    }),
  );
}

test('Home options keep the full accessible name on a fixed44 icon, with a nonmodal top-layer popover', () => {
  const html = render();
  assert.match(html, /data-testid="home-card-menu-real-id"/);
  assert.match(html, /aria-label="Options for A deliberately long app title/);
  assert.match(html, /aria-haspopup="menu" aria-expanded="false"/);
  const target = html.match(/popovertarget="([^"]+)"/i)?.[1];
  assert.ok(target);
  assert.ok(html.includes(`id="${target}" popover="auto" role="menu"`));
  assert.ok(html.includes(`aria-controls="${target}"`));
  assert.match(html, /width: 44px; height: 44px/);
  assert.match(html, /<span aria-hidden="true">/);
  assert.doesNotMatch(html, /role="dialog"|aria-modal|BottomSheet/);
});

test('pin plus history and reorder-only props keep all actual menuitems available', () => {
  const both = render({ onHistory() {}, onMoveEarlier() {}, onMoveLater() {} });
  for (const label of ['Pin to Home', 'Move earlier', 'Move later', 'History'])
    assert.ok(
      both.includes(`role="menuitem" data-action=`) && both.includes(`aria-label="${label}"`),
    );
  assert.equal((both.match(/role="menuitem"/g) ?? []).length, 4);
  const reorderOnly = render({ onToggle: undefined, onMoveLater() {} });
  assert.equal((reorderOnly.match(/role="menuitem"/g) ?? []).length, 1);
  assert.match(reorderOnly, /data-action="move-later"/);
  assert.equal(render({ onToggle: undefined }), '');
});

test('several cards have independent native popover targets and semantic keyboard buttons', () => {
  const html = renderToStaticMarkup(
    createElement(
      'div',
      null,
      createElement(module.exports.HomeCardMenu, { name: 'One', pinned: false, onHistory() {} }),
      createElement(module.exports.HomeCardMenu, { name: 'Two', pinned: true, onToggle() {} }),
    ),
  );
  const targets = [...html.matchAll(/popovertarget="([^"]+)"/gi)].map((match) => match[1]);
  assert.equal(targets.length, 2);
  assert.equal(new Set(targets).size, 2);
  assert.equal((html.match(/tabindex="-1" class="zoen-card-menu-item"/g) ?? []).length, 2);
});

test('the card menu prerenders without DOM access and inherits the actual theme and reduced-motion CSS', () => {
  const html = render();
  assert.ok(html.includes(`--card-menu-raised:${theme.colors.raised}`));
  assert.ok(html.includes(`--card-menu-text:${theme.colors.text}`));
  assert.match(html, /color-scheme:dark/);
  assert.match(html, /prefers-reduced-motion: reduce/);
});

test('HomeActions keeps its existing single Add an app control without executing navigation at render', () => {
  navigation.length = 0;
  const html = renderToStaticMarkup(
    createElement(module.exports.HomeActions, { canOrganize: true }),
  );
  assert.equal((html.match(/<button/g) ?? []).length, 1);
  assert.ok(html.includes('<button>Add an app</button>'));
  const withRestore = renderToStaticMarkup(
    createElement(module.exports.HomeActions, { hiddenCount: 2, onRestoreHidden() {} }),
  );
  assert.equal((withRestore.match(/<button/g) ?? []).length, 2);
  assert.ok(withRestore.includes('Restore hidden apps (2)'));
  assert.deepEqual(navigation, []);
});
