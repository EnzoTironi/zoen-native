import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as jsxRuntime from 'react/jsx-runtime';
import * as forwardOptions from './forward-options.ts';
import * as messagePresentation from './message-presentation.ts';
import type { TimelineEntry } from './types';
import type { ForwardOptions } from './generated/ForwardOptions';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
async function compile(path: string) {
  const file = new URL(path, import.meta.url);
  const transformed = transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    configFile: false,
    babelrc: false,
    presets: ['babel-preset-expo'],
    caller: {
      name: 'metro',
      platform: 'ios',
      engine: 'hermes',
      isDev: false,
      supportsReactCompiler: false,
    },
  }).code;
  return (
    await build({
      stdin: { contents: transformed, loader: 'tsx' },
      bundle: false,
      platform: 'node',
      format: 'cjs',
      write: false,
    })
  ).outputFiles[0]!.text;
}
const [menuCode, panelCode, previewCode] = await Promise.all([
  compile('../components/forward-options-menu.ios.tsx'),
  compile('../components/forward-review-panel.tsx'),
  compile('../components/forward-message-preview.tsx'),
]);

test('legacy missing flags use defaults; supplied malformed flags cannot become a different preview', () => {
  assert.deepEqual(forwardOptions.ownForwardOptions(), { hide_author: false, hide_caption: false });
  for (const value of [
    null,
    {},
    { hide_author: true },
    { hide_author: 'false', hide_caption: false },
  ])
    assert.throws(() => forwardOptions.ownForwardOptions(value as unknown as ForwardOptions));
});
type Element = { type: string; props: Record<string, any> };
function all(node: any, type: string): Element[] {
  if (!node || typeof node !== 'object') return [];
  if (Array.isArray(node)) return node.flatMap((child) => all(child, type));
  return [...(node.type === type ? [node] : []), ...all(node.props?.children, type)];
}
function load(code: string, scheme = 'dark') {
  const module = { exports: {} as Record<string, any> };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'react/jsx-runtime') return jsxRuntime;
      if (name.startsWith('@babel/runtime/')) return require(name);
      if (name.endsWith('/forward-options')) return forwardOptions;
      if (name.endsWith('/message-presentation')) return messagePresentation;
      if (name.endsWith('/i18n/index.ts')) return { t: (key: string) => key };
      if (name.endsWith('/theme'))
        return {
          useTheme: () => ({ scheme, colors: { accent: '#08f' }, presentation: { list: {} } }),
        };
      if (name === '@expo/ui') return { Host: 'Host' };
      if (name === '@expo/ui/swift-ui')
        return { Menu: 'Menu', Toggle: 'Toggle', Section: 'Section' };
      if (name === '@expo/ui/swift-ui/modifiers')
        return Object.fromEntries(
          ['buttonStyle', 'disabled', 'frame', 'labelStyle', 'tint'].map((key) => [
            key,
            (value: unknown) => ({ key, value }),
          ]),
        );
      if (name === 'react-native')
        return {
          View: 'View',
          Text: 'Text',
          ScrollView: 'Scroll',
          StyleSheet: { hairlineWidth: 0.5 },
          useWindowDimensions: () => ({ width: 402, fontScale: 1 }),
        };
      if (name === 'react-native-keyboard-controller') return { KeyboardAvoidingView: 'Avoiding' };
      if (name.endsWith('/profile-avatar')) return { ProfileAvatar: 'Avatar' };
      if (name === './primitives') return { NativeButton: 'Button' };
      if (name === './forward-options-menu') return { ForwardOptionsMenu: 'Options' };
      if (name === './forward-message-preview') return { ForwardMessagePreview: 'Preview' };
      if (name.endsWith('/MediaAttachment')) return { MessageMedia: 'Media' };
      if (name.endsWith('/message-audio-provider')) return { useMessageAudioFocus() {} };
      if (name === './message-markdown') return { MessageMarkdown: 'Markdown' };
      throw Error(`Missing native forwarding port: ${name}`);
    },
  });
  return module.exports;
}

for (const scheme of ['light', 'dark'])
  test(`native forwarding menu uses ${scheme} theme, exact checked flags and disabled actions`, () => {
    const component = load(menuCode, scheme);
    const changes: ForwardOptions[] = [];
    const props = {
      options: { hide_author: false, hide_caption: true },
      canHideCaption: true,
      remaining: true,
      onChange: (value: ForwardOptions) => changes.push(value),
    };
    const tree = component.ForwardOptionsMenu(props);
    assert.equal(all(tree, 'Host')[0]!.props.colorScheme, scheme);
    assert.equal(all(tree, 'Host')[0]!.props.seedColor, '#08f');
    assert.equal(all(tree, 'Section')[0]!.props.title, 'chat.remainingForwardMessages');
    const toggles = all(tree, 'Toggle');
    assert.deepEqual(
      toggles.map((t) => t.props.isOn),
      [true, false],
    );
    toggles[0]!.props.onIsOnChange(false);
    toggles[1]!.props.onIsOnChange(true);
    assert.deepEqual(JSON.parse(JSON.stringify(changes)), [
      { hide_author: true, hide_caption: true },
      { hide_author: false, hide_caption: false },
    ]);
    const disabled = component.ForwardOptionsMenu({ ...props, disabled: true });
    const modifier = all(disabled, 'Menu')[0]!.props.modifiers.find(
      (v: any) => v.key === 'disabled',
    );
    assert.equal(modifier.value, true);
    for (const toggle of all(disabled, 'Toggle')) toggle.props.onIsOnChange(false);
    assert.equal(changes.length, 2, 'retained disabled native actions cannot change options');
    const textOnly = component.ForwardOptionsMenu({
      ...props,
      canHideCaption: false,
      remaining: false,
    });
    assert.equal(all(textOnly, 'Toggle').length, 1);
    assert.equal(all(textOnly, 'Section').length, 0);
  });

const entry = (kind: string): TimelineEntry =>
  ({
    id: 'source',
    deleted: false,
    author: { id: 'alice', name: 'Alice', is_me: false },
    kind: {
      Message: {
        text: 'Preserve text 雨 😀',
        card: ['file', 'app'].includes(kind)
          ? {
              item_id: 'original',
              version: 7,
              kind_id: kind,
              title: 'Original',
              kind_label: kind,
              summary: null,
            }
          : null,
      },
    },
    voice:
      kind === 'voice'
        ? { item_id: 'voice', ms: '2400', transcript: 'Exact transcript', levels: [0.2] }
        : null,
  }) as TimelineEntry;

for (const kind of ['text', 'file', 'voice', 'app'])
  test(`hidden caption preview preserves ${kind} source identity and masks only ordinary file captions`, () => {
    const source = entry(kind);
    const before = JSON.stringify(source);
    const options = { hide_author: true, hide_caption: true };
    const tree = load(previewCode).ForwardMessagePreview({
      spaceId: 'source',
      entry: source,
      options,
      isCurrent: () => true,
    });
    const markdown = all(tree, 'Markdown');
    assert.equal(markdown.length, kind === 'file' ? 0 : 1);
    if (kind !== 'file') assert.equal(markdown[0]!.props.text, 'Preserve text 雨 😀');
    const media = all(tree, 'Media');
    if (kind === 'file' || kind === 'voice') {
      assert.equal(media[0]!.props.entry.id, source.id);
      assert.strictEqual(media[0]!.props.entry.voice, source.voice);
      assert.equal(
        media[0]!.props.entry.kind.Message.card?.version,
        kind === 'file' ? 7 : undefined,
      );
    }
    assert.equal(JSON.stringify(source), before, 'canonical source is never edited by preview');
  });

test('single review hides attribution, gates caption by source and retires options after acceptance', () => {
  const panel = load(panelCode).ForwardReviewPanel;
  const props = {
    spaceId: 'source',
    entry: entry('file'),
    recipient: 'Recipient',
    sourceTitle: 'Source chat',
    options: { hide_author: true, hide_caption: true },
    onOptions() {},
    accepted: false,
    busy: false,
    canSend: true,
    insets: { bottom: 34 },
    isCurrent: () => true,
    onSend() {},
    onCancel() {},
  };
  const tree = panel(props);
  assert.deepEqual(
    all(tree, 'Text').map((n) => n.props.children),
    ['Recipient'],
  );
  assert.strictEqual(all(tree, 'Preview')[0]!.props.options, props.options);
  assert.equal(all(tree, 'Options')[0]!.props.canHideCaption, true);
  assert.equal(all(panel({ ...props, canSend: false }), 'Options')[0]!.props.disabled, true);
  assert.equal(all(panel({ ...props, accepted: true }), 'Options').length, 0);
  assert.equal(
    all(panel({ ...props, entry: entry('voice') }), 'Options')[0]!.props.canHideCaption,
    false,
  );
});
