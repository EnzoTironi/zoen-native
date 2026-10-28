import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import * as presentation from './message-presentation.ts';
import * as textPresentation from './presentation.ts';
import * as markdown from './message-markdown.ts';
import * as renderedText from '../components/use-rendered-message-text.ts';
import { compileMessageSource } from './message-source-compiler.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { message } from './message-accessibility.fixtures.ts';
const require = createRequire(import.meta.url);

test('the actual selection checkbox shares the current row native text receipt and keeps its canonical toggle', async () => {
  const root = createReactTestRoot();
  let checkbox: any, body: any;
  const calls: unknown[] = [];
  const View = ({ children }: any) => React.createElement(React.Fragment, null, children);
  function Checkbox(props: any) {
    React.useLayoutEffect(() => {
      checkbox = props;
    }, [props]);
    return View(props);
  }
  function Body(props: any) {
    React.useLayoutEffect(() => {
      body = props;
    }, [props]);
    return null;
  }
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': jsxRuntime,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': { View, Text: () => null, Pressable: Checkbox },
    './use-rendered-message-text': renderedText,
    '../core/presentation': textPresentation,
    '../core/message-presentation': presentation,
    '../core/message-markdown': markdown,
    '../theme': {
      useTheme: () => ({
        presentation: { chat: { service: { text: '#fff', background: '#000' } } },
        localDate: new Date(1000),
      }),
    },
    '../i18n': { t: (key: string) => key, formatLocale: 'en-US' },
    './conversation-message': { ConversationMessage: Body },
    './conversation-album-message': { ConversationAlbumMessage: Body },
    './conversation-reveal-content-view': { ConversationRevealContentView: View },
    '../miniapps/timeline-activity': { MiniappTimelineActivityView: () => null },
  };
  const file = new URL('../components/conversation-timeline-message.tsx', import.meta.url);
  const module = { exports: {} as any };
  runInNewContext(compileMessageSource(file.pathname, 'ios', readFileSync(file, 'utf8')), {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name.startsWith('@babel/runtime/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let entry = message({ kind: { Message: { text: String.raw`QA753\: nota\.`, card: null } } });
  const render = () =>
    root.render(
      React.createElement(module.exports.ConversationTimelineMessage, {
        entry,
        spaceId: 'space',
        selection: { selected: false, onToggle: (value: unknown) => calls.push(value) },
      }),
    );
  try {
    await render();
    assert.equal(checkbox.accessibilityRole, 'checkbox');
    assert.ok(!checkbox.accessibilityLabel.includes('\\'));
    const receipt = body.accessibilityText as renderedText.RenderedMessageText;
    await root.act(() =>
      receipt.accept({
        revision: receipt.revision,
        markdown: textPresentation.messageText(entry),
        text: 'QA753: nota.',
      }),
    );
    assert.ok(checkbox.accessibilityLabel.includes('QA753: nota.'));
    checkbox.onPress();
    assert.equal(calls[0], entry);
    entry = { ...entry, id: 'another-row', kind: { Message: { text: '**other**', card: null } } };
    await render();
    await root.act(() =>
      receipt.accept({
        revision: receipt.revision,
        markdown: String.raw`QA753\: nota\.`,
        text: 'retired',
      }),
    );
    assert.ok(!checkbox.accessibilityLabel.includes('QA753'));
    assert.ok(!checkbox.accessibilityLabel.includes('retired'));
  } finally {
    await root.close();
  }
});
