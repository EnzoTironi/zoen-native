import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { createLocalization } from '../i18n/runtime.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { common } from '../i18n/catalogs/common.ts';
import { deferred, tick } from './store.fixtures.ts';
import { elements, entry, plainMessage, render as renderFace } from './message-caption.fixtures.ts';
import type { TimelineEntry } from './types.ts';
import {
  messageMarkdownLink,
  messageMayContainMarkdown,
  messageMarkdownMayLoadMedia,
} from './message-markdown.ts';

test('ordinary messages keep measured text while Markdown candidates reach the real parser', () => {
  for (const text of ['Hello there', 'Olá 👋', 'First line\nSecond line', '2 + 2 = 4'])
    assert.equal(messageMayContainMarkdown(text), false, text);
  for (const text of [
    '**bold** and *italic*',
    '__underline__',
    '~~removed~~',
    '`inline()`',
    '```ts\nconst value = 1;\n```',
    '# Heading',
    '> A quotation',
    '- first\n- second',
    '1. one\n2. two',
    '[link](https://example.com)',
    '| Column |\n| --- |\n| Value |',
    '||spoiler||',
    'See https://example.invalid/help',
    'HTTP://example.invalid',
    'Visit www.example.invalid.',
    'Email person@example.invalid',
  ])
    assert.equal(messageMayContainMarkdown(text), true, text);
});

test('Markdown links admit explicit user navigation but reject executable and local schemes', () => {
  for (const url of [
    'https://example.com/page?value=1#section',
    'http://example.com/',
    'mailto:person@example.com',
    'tel:+5511999999999',
    'zoen://page/example',
  ])
    assert.equal(messageMarkdownLink(url), new URL(url).href);
  for (const url of [
    'javascript:alert(1)',
    'data:text/html,hello',
    'file:///private/local-file',
    'intent://anything',
    '/relative',
    '//example.com',
    ' https://example.com',
    'https://example.com\n/path',
    'https://user:password@example.com',
    'https://example.com/\u0000',
    '',
  ])
    assert.equal(messageMarkdownLink(url), null, url);
});

const source = fileURLToPath(new URL('../components/message-markdown.tsx', import.meta.url));
const nativePorts = new Set([
  'react',
  'react/jsx-runtime',
  'react-native',
  'react-native-enriched-markdown',
  '../theme',
  './message-context-ports',
  './use-quote-collapse',
  './quote-layout-context',
  'expo-clipboard',
  'expo-haptics',
  '../i18n/index.ts',
]);
const compiled = await build({
  entryPoints: [source],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'message-markdown-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          nativePorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

type AlertRecord = {
  title: string;
  message: string;
  buttons: { text: string; style?: string; onPress: () => void }[];
  options: { cancelable: boolean; onDismiss: () => void };
};
function fixture(
  platform = 'ios',
  options: {
    launch?: () => Promise<unknown>;
    copy?: () => Promise<boolean>;
    language?: 'en' | 'pt-BR';
  } = {},
) {
  const opened: string[] = [];
  const copied: string[] = [];
  const alerts: AlertRecord[] = [];
  const localization = createLocalization({ chat, common }, [options.language ?? 'en']);
  const colors = {
    accent: '#1478d4',
    secondary: '#798088',
    surface: '#232933',
    muted: '#30353f',
    text: '#ffffff',
    line: '#3b424b',
  };
  type Slot = { deps?: unknown[]; value?: unknown; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0;
  const effects: (() => void)[] = [];
  const slot = () => slots[cursor++] ?? (slots[cursor - 1] = {});
  const same = (a: unknown[] | undefined, b: unknown[]) =>
    a && a.length === b.length && a.every((value, i) => Object.is(value, b[i]));
  const module = { exports: {} as Record<string, unknown> };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: platform } },
    URL,
    require(name: string) {
      if (name === 'react')
        return {
          use: () => null,
          memo: (value: unknown) => value,
          useMemo: (read: () => unknown, deps: unknown[]) => {
            const owner = slot();
            if (!same(owner.deps, deps)) {
              owner.deps = deps;
              owner.value = read();
            }
            return owner.value;
          },
          useRef: (initial: unknown) => {
            const owner = slot();
            if (!('value' in owner)) owner.value = { current: initial };
            return owner.value;
          },
          useLayoutEffect: (read: () => (() => void) | undefined, deps: unknown[]) => {
            const owner = slot();
            if (!same(owner.deps, deps)) {
              owner.deps = deps;
              effects.push(() => {
                owner.cleanup?.();
                owner.cleanup = read();
              });
            }
          },
        };
      if (name === 'react/jsx-runtime')
        return { jsx: (type: unknown, props: unknown) => ({ type, props }) };
      if (name === 'react-native')
        return {
          Text: 'PlainText',
          Alert: {
            alert: (...args: [string, string, AlertRecord['buttons'], AlertRecord['options']]) => {
              const [title, message, buttons, options] = args;
              alerts.push({ title, message, buttons, options });
            },
          },
          Linking: {
            openURL: (url: string) => {
              opened.push(url);
              return options.launch?.() ?? Promise.resolve();
            },
          },
        };
      if (name === 'expo-haptics') return { selectionAsync: async () => {} };
      if (name === 'expo-clipboard')
        return {
          setStringAsync: (text: string) => {
            copied.push(text);
            return options.copy?.() ?? Promise.resolve(true);
          },
        };
      if (name === '../i18n/index.ts') return localization;
      if (name === 'react-native-enriched-markdown')
        return { EnrichedMarkdownText: 'NativeMarkdownText' };
      if (name === './use-quote-collapse') return { useQuoteCollapse: () => ({}) };
      if (name === './quote-layout-context') return { useQuoteLayoutTransition: () => null };
      if (name === './message-context-ports') return { MessageTextSelection: {} };
      if (name === '../theme') return { useTheme: () => ({ colors }) };
      throw new Error(`Unexpected native port ${name}`);
    },
  });
  const render = module.exports.MessageMarkdown as (props: Record<string, unknown>) => {
    type: unknown;
    props: Record<string, unknown>;
  };
  return {
    opened,
    copied,
    alerts,
    colors,
    render: (props: Record<string, unknown>) => {
      cursor = 0;
      const tree = render(props);
      for (const effect of effects.splice(0)) effect();
      return tree;
    },
    close: () => {
      for (const owner of slots) owner.cleanup?.();
    },
  };
}

test('the native renderer gets the exact signed source with read-only task lists and chat typography', () => {
  const f = fixture();
  const text =
    '# Plano\n\n**Bold** and *italic*\n\n> Quote\n\n- [ ] Task\n\n```rust\nlet s = "**literal**";\n```\n\n| Item | Value |\n| --- | --- |\n| A | 1 |';
  const rendered = f.render({
    text,
    color: '#ffffff',
    fontSize: 17,
    maxWidth: 300,
    testID: 'message-body',
  });
  assert.equal(rendered.type, 'NativeMarkdownText');
  assert.equal(rendered.props.markdown, text);
  assert.equal(rendered.props.flavor, 'github');
  assert.equal(rendered.props.enableTaskListItemToggle, false);
  assert.equal(rendered.props.enableLinkPreview, false);
  assert.equal(rendered.props.selectable, false);
  assert.equal(rendered.props.enableBlockContextMenu, false);
  assert.equal(rendered.props.allowFontScaling, true);
  assert.equal(rendered.props.streamingAnimation, false);
  const style = rendered.props.markdownStyle as Record<string, Record<string, unknown>>;
  assert.equal(style.paragraph!.color, '#ffffff');
  assert.equal(style.paragraph!.fontSize, 17);
  assert.equal(style.codeBlock!.fontFamily, 'Menlo');
  assert.equal(style.link!.color, f.colors.accent);
  assert.equal(style.table!.headerTextColor, '#ffffff');
  const flags = rendered.props.md4cFlags as Record<string, unknown>;
  assert.equal(flags.hardSoftBreaks, true);
  assert.equal(flags.preserveBlankLines, true);
  assert.equal(flags.latexMath, false);
  assert.equal(readFileSync(source, 'utf8').includes('dangerouslySetInnerHTML'), false);
});

test('native Markdown and media-free fallback expose their actual text-root measurement', () => {
  const onLayout = () => {};
  for (const platform of ['ios', 'web']) {
    const f = fixture(platform);
    const rendered = f.render({
      text: '![media](https://example.invalid/x.png)',
      color: '#fff',
      fontSize: 17,
      maxWidth: 300,
      onLayout,
    });
    assert.equal(rendered.type, platform === 'ios' ? 'NativeMarkdownText' : 'PlainText');
    assert.equal(rendered.props.onLayout, onLayout);
    f.close();
  }
});

test('rendered link events use the navigation boundary, and text selection is an explicit mode', () => {
  const f = fixture();
  const { props } = f.render({
    text: '[text](https://example.com)',
    color: '#fff',
    fontSize: 20,
    maxWidth: 350,
    selectable: true,
  });
  const press = props.onLinkPress as (event: { url: string }) => void;
  press({ url: 'javascript:alert(1)' });
  press({ url: 'file:///private/local-file' });
  assert.deepEqual(f.opened, []);
  press({ url: 'https://example.com/path' });
  assert.deepEqual(f.opened, ['https://example.com/path']);
  assert.equal(props.selectable, true);
  assert.equal(props.enableBlockContextMenu, true);
});

test('iOS opts into the native media-free parser while other platforms retain exact plain-text media fallback', () => {
  for (const platform of ['ios', 'android', 'web']) {
    const f = fixture(platform);
    for (const text of [
      '![remote](https://example.invalid/private.png)',
      '![local](file:///private/synthetic-file)',
      '![data](data:image/png;base64,AA)',
      '![reference][id]\n\n[id]: https://example.invalid/image',
      '**Bold** and ![x](https://example.invalid/x)',
      '<video src="https://example.invalid/video.mp4"></video>',
      '<IMG src="file:///private/synthetic-file">',
      '`![literal](image)`',
    ]) {
      assert.equal(messageMarkdownMayLoadMedia(text), true);
      assert.equal(messageMayContainMarkdown(text), false);
      const rendered = f.render({ text, color: '#123456', fontSize: 17, maxWidth: 280 });
      if (platform === 'ios') {
        assert.equal(rendered.type, 'NativeMarkdownText');
        assert.equal(rendered.props.markdown, text);
        assert.equal((rendered.props.md4cFlags as { noMedia: boolean }).noMedia, true);
      } else {
        assert.equal(rendered.type, 'PlainText');
        assert.equal(rendered.props.children, text);
      }
    }
  }
  assert.equal(messageMayContainMarkdown('**Hello**!'), true);
  assert.equal(messageMayContainMarkdown('```c\nx < y; // ![literal](url)\n```', true), true);
});

test('bare links stay passive until the native link gesture crosses the existing navigation guard', () => {
  const f = fixture();
  for (const text of [
    'https://example.invalid/help',
    'www.example.invalid',
    'person@example.invalid',
  ]) {
    const rendered = f.render({ text, color: '#fff', fontSize: 17, maxWidth: 300 });
    assert.equal(rendered.props.markdown, text);
    assert.equal(rendered.props.enableLinkPreview, false);
    assert.equal((rendered.props.md4cFlags as { noMedia: boolean }).noMedia, true);
    assert.deepEqual(f.opened, []);
    const press = rendered.props.onLinkPress as (event: { url: string }) => void;
    press({ url: 'javascript:alert(1)' });
    press({ url: 'data:text/html,hello' });
    press({ url: 'file:///private/example' });
    assert.deepEqual(f.opened, []);
  }
  const { props } = f.render({
    text: 'person@example.invalid',
    color: '#fff',
    fontSize: 17,
    maxWidth: 300,
  });
  const press = props.onLinkPress as (event: { url: string }) => void;
  press({ url: 'mailto:person@example.invalid' });
  press({ url: 'http://www.example.invalid' });
  assert.deepEqual(f.opened, ['mailto:person@example.invalid', 'http://www.example.invalid/']);
});

const linkProps = { text: 'person@example.invalid', color: '#fff', fontSize: 17, maxWidth: 300 };
function linkPress(f: ReturnType<typeof fixture>, text = linkProps.text) {
  return f.render({ ...linkProps, text }).props.onLinkPress as (event: { url: string }) => void;
}
const launchRefused = () => Promise.reject(Error('Platform launch refused'));

test('invalid links have zero launch, clipboard and native feedback effects', async () => {
  const f = fixture('ios', { launch: launchRefused });
  const press = linkPress(f);
  for (const url of [
    'javascript:alert(1)',
    'file:///private/x',
    '/relative',
    ' https://example.invalid',
  ])
    press({ url });
  await tick();
  assert.deepEqual(f.opened, []);
  assert.deepEqual(f.copied, []);
  assert.deepEqual(f.alerts, []);
  f.close();
});

test('successful link launch stays one normal launch with no fallback or clipboard write', async () => {
  const f = fixture();
  linkPress(f)({ url: 'mailto:person@example.invalid' });
  await tick();
  assert.deepEqual(f.opened, ['mailto:person@example.invalid']);
  assert.deepEqual(f.copied, []);
  assert.deepEqual(f.alerts, []);
  f.close();
});

test('a rejected launch offers native localized copy and cancel, with no implicit effects', async () => {
  for (const language of ['en', 'pt-BR'] as const) {
    const f = fixture('ios', { launch: launchRefused, language });
    linkPress(f)({ url: 'mailto:person@example.invalid' });
    await tick();
    assert.equal(f.alerts.length, 1);
    const alert = f.alerts[0]!;
    assert.equal(
      alert.title,
      language === 'en' ? 'Could not open link' : 'Não foi possível abrir o link',
    );
    assert.equal(alert.message, 'mailto:person@example.invalid');
    assert.deepEqual(
      [...alert.buttons].map((button) => button.text),
      language === 'en' ? ['Cancel', 'Copy address'] : ['Cancelar', 'Copiar endereço'],
    );
    assert.equal(alert.buttons[0]!.style, 'cancel');
    assert.equal(alert.options.cancelable, true);
    assert.deepEqual(f.copied, []);
    alert.buttons[0]!.onPress();
    await tick();
    assert.equal(f.opened.length, 1);
    assert.deepEqual(f.copied, []);
    f.close();
  }
});

test('only the explicit copy action writes the exact validated target once', async () => {
  const f = fixture('ios', { launch: launchRefused });
  linkPress(f)({ url: 'https://example.invalid/path?x=1#part' });
  await tick();
  assert.deepEqual(f.copied, []);
  f.alerts[0]!.buttons[1]!.onPress();
  await tick();
  assert.deepEqual(f.copied, ['https://example.invalid/path?x=1#part']);
  assert.equal(f.opened.length, 1);
  assert.equal(f.alerts.length, 1);
  f.close();
});

test('clipboard rejection and false results show failure without launch or copy retries', async () => {
  for (const copy of [
    () => Promise.resolve(false),
    () => Promise.reject(Error('clipboard refused')),
  ]) {
    const f = fixture('ios', { launch: launchRefused, copy, language: 'pt-BR' });
    linkPress(f)({ url: 'mailto:person@example.invalid' });
    await tick();
    f.alerts[0]!.buttons[1]!.onPress();
    await tick();
    assert.deepEqual(f.copied, ['mailto:person@example.invalid']);
    assert.equal(f.opened.length, 1);
    assert.equal(f.alerts.length, 2);
    assert.equal(f.alerts[1]!.title, 'Não foi possível copiar o endereço');
    assert.equal(f.alerts[1]!.message, 'mailto:person@example.invalid');
    f.alerts[1]!.buttons[0]!.onPress();
    await tick();
    assert.equal(f.copied.length, 1);
    f.close();
  }
});

test('native dismiss cancels and concurrent launch failures do not stack prompts', async () => {
  const f = fixture('ios', { launch: launchRefused });
  const press = linkPress(f);
  press({ url: 'mailto:person@example.invalid' });
  press({ url: 'mailto:person@example.invalid' });
  await tick();
  assert.equal(f.opened.length, 2);
  assert.equal(f.alerts.length, 1);
  f.alerts[0]!.options.onDismiss();
  await tick();
  assert.equal(f.copied.length, 0);
  press({ url: 'mailto:person@example.invalid' });
  await tick();
  assert.equal(f.opened.length, 3);
  assert.equal(f.alerts.length, 2);
  f.alerts[1]!.options.onDismiss();
  await tick();
  f.close();
});

test('a late launch refusal cannot show feedback after message retirement', async () => {
  const pending = deferred<void>();
  const f = fixture('ios', { launch: () => pending.promise });
  linkPress(f)({ url: 'mailto:person@example.invalid' });
  f.close();
  pending.reject(Error('late launch failure'));
  await tick();
  assert.equal(f.alerts.length, 0);
  assert.equal(f.copied.length, 0);
});

test('old copy actions cannot act after source replacement or retirement', async () => {
  for (const retire of [true, false]) {
    const f = fixture('ios', { launch: launchRefused });
    linkPress(f)({ url: 'mailto:person@example.invalid' });
    await tick();
    if (retire) f.close();
    else linkPress(f, 'A different message');
    f.alerts[0]!.buttons[1]!.onPress();
    await tick();
    assert.equal(f.copied.length, 0);
    assert.equal(f.opened.length, 1);
    f.close();
  }
});

test('late clipboard failure cannot present a new alert after message retirement', async () => {
  const pending = deferred<boolean>();
  const f = fixture('ios', { launch: launchRefused, copy: () => pending.promise });
  linkPress(f)({ url: 'mailto:person@example.invalid' });
  await tick();
  f.alerts[0]!.buttons[1]!.onPress();
  await tick();
  assert.equal(f.copied.length, 1);
  f.close();
  pending.reject(Error('late clipboard failure'));
  await tick();
  assert.equal(f.alerts.length, 1);
});

test('compiled message, file/album captions and Agent App faces reach the same real launch feedback', async () => {
  const text = 'person@example.invalid';
  const file = entry(text);
  const agent = entry(text, {
    author: { ...file.author, kind: 'Agent' },
    kind: {
      Message: { text, card: { item_id: 'deck', kind_id: 'app', app: { app_id: 'deckcraft' } } },
    } as TimelineEntry['kind'],
  });
  for (const face of [
    renderFace(plainMessage(text)),
    renderFace(file),
    renderFace(file, { albumPart: { captionOnly: true } }),
    renderFace(agent),
  ]) {
    // Feed the actual React-compiled face's props into the compiled shared renderer;
    // only platform launch, native Alert and clipboard are controlled ports.
    const markdown = elements(face.tree).find((node) => node.type === 'MessageMarkdown')!;
    assert.equal(markdown.props.text, text);
    const f = fixture('ios', { launch: launchRefused });
    const rendered = f.render(markdown.props);
    assert.equal(rendered.props.markdown, text);
    assert.equal(f.opened.length, 0);
    const press = rendered.props.onLinkPress as (event: { url: string }) => void;
    press({ url: 'javascript:alert(1)' });
    assert.equal(f.opened.length, 0);
    press({ url: 'mailto:person@example.invalid' });
    await tick();
    assert.equal(f.alerts.length, 1);
    assert.equal(f.copied.length, 0);
    f.alerts[0]!.buttons[1]!.onPress();
    await tick();
    assert.deepEqual(f.copied, ['mailto:person@example.invalid']);
    assert.equal(f.opened.length, 1);
    f.close();
  }
});
