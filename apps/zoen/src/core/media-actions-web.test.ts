import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { build } from 'esbuild';
import { initializeLocalization } from '../i18n/index.ts';
import { resolveSkyTheme } from '../theme/sky.ts';
import type { MediaActionsProps } from '../media/media-actions.types';

const require = createRequire(import.meta.url);
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../media/media-actions.web.tsx', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'attachment-render-platform-ports',
      setup(builder) {
        builder.onResolve(
          {
            filter:
              /^(react(?:\/jsx-runtime)?|\.\.\/theme|\.\.\/components\/primitives|\.\.\/i18n\/index\.ts)$/,
          },
          ({ path }) => ({ path, external: true }),
        );
      },
    },
  ],
});
const theme = resolveSkyTheme('deepNight', new Date(2026, 9, 19));
const result = {
  exports: {} as { MediaActions: (props: MediaActionsProps) => ReturnType<typeof createElement> },
};
runInNewContext(compiled.outputFiles[0]!.text, {
  module: result,
  exports: result.exports,
  require: (name: string) => {
    if (name === '../theme') return { useTheme: () => theme };
    if (name === '../components/primitives') return { Icon: () => createElement('span') };
    if (name === '../i18n/index.ts') return require('../i18n/index.ts');
    return require(name);
  },
});
initializeLocalization('en');
function render(overrides: Partial<MediaActionsProps> = {}) {
  return renderToStaticMarkup(
    createElement(result.exports.MediaActions, {
      disabled: false,
      hasVoiceEditor: true,
      onChoose: () => assert.fail('Rendering must never launch a file picker'),
      ...overrides,
    }),
  );
}

test('Web attachments use the browser nonmodal popover associated with the composer trigger', () => {
  const html = render();
  const target = html.match(/popovertarget="([^"]+)"/i)?.[1];
  assert.ok(target);
  assert.ok(html.includes(`id="${target}" popover="auto" role="menu"`));
  assert.match(html, /aria-haspopup="menu" aria-expanded="false"/);
  assert.ok(html.includes(`aria-controls="${target}"`));
  assert.doesNotMatch(html, /role="dialog"|aria-modal|BottomSheet/);
});

test('Each attachment action is a native keyboard-activatable menu button with an accessible label', () => {
  const html = render();
  assert.equal((html.match(/role="menuitem"/g) ?? []).length, 4);
  for (const label of ['Photos and videos', 'Camera', 'Import audio', 'File']) {
    assert.ok(html.includes(`</span>${label}</button>`), label);
    assert.ok(html.includes(`role="menuitem" aria-label="${label}"`), label);
  }
  assert.equal((html.match(/tabindex="-1" class="zoen-attachment-item"/g) ?? []).length, 4);
  assert.equal((html.match(/aria-hidden="true"/g) ?? []).length, 4);
});

test('Voice import follows the existing editor capability and disabled composers cannot invoke actions', () => {
  const html = render({ hasVoiceEditor: false, disabled: true });
  assert.equal((html.match(/role="menuitem"/g) ?? []).length, 3);
  assert.doesNotMatch(html, />Import audio</);
  assert.equal((html.match(/ disabled=""/g) ?? []).length, 4);
});

test('Concurrent composers keep distinct browser popover targets', () => {
  const html = renderToStaticMarkup(
    createElement(
      'div',
      null,
      createElement(result.exports.MediaActions, {
        disabled: false,
        hasVoiceEditor: false,
        onChoose() {},
      }),
      createElement(result.exports.MediaActions, {
        disabled: false,
        hasVoiceEditor: false,
        onChoose() {},
      }),
    ),
  );
  const targets = [...html.matchAll(/popovertarget="([^"]+)"/gi)].map((match) => match[1]);
  assert.equal(targets.length, 2);
  assert.equal(new Set(targets).size, 2);
});

test('The Web menu inherits the current sky colors without requiring DOM access during prerender', () => {
  const html = render();
  assert.ok(html.includes(`--attachment-raised:${theme.colors.raised}`));
  assert.ok(html.includes(`--attachment-text:${theme.colors.text}`));
  assert.match(html, /color-scheme:dark/);
});
