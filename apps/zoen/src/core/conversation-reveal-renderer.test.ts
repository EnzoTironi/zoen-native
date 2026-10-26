import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import * as measurements from './conversation-reveal-measurement.ts';
import * as presentation from './message-presentation.ts';
import { elements, entry, plainMessage, render } from './message-caption.fixtures.ts';
import { resolveSkyTheme } from '../theme/sky.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const diagnostics: unknown[] = [];
function load(path: string, ports: Record<string, unknown>) {
  const file = new URL(path, import.meta.url);
  const { code } = transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [
      [
        'babel-plugin-react-compiler',
        {
          target: '19',
          logger: {
            logEvent(_file: unknown, event: { kind: string }) {
              if (event.kind === 'CompileError' || event.kind === 'CompileDiagnostic')
                diagnostics.push(event);
            },
          },
        },
      ],
    ],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  });
  const module = { exports: {} as Record<string, any> };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, `Undeclared native port: ${name}`);
      return ports[name];
    },
  });
  return module.exports;
}

test('Compiler Timeline/context bind real native text measurements and retire late recycled callbacks', async () => {
  const root = createReactTestRoot();
  let closed = false;
  let host: any, anchor: any;
  const reports: { id: string; layout: any }[] = [];
  const bodyNode = { measureLayout() {} };
  const callbacks: ((x: number, y: number, w: number, h: number) => void)[] = [];
  const textNode = {
    measureLayout(relative: unknown, callback: (typeof callbacks)[number]) {
      assert.equal(relative, bodyNode);
      callbacks.push(callback);
    },
  };
  const ports: Record<string, unknown> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      View: (props: any) => {
        if (props.collapsable === false) host = props;
        return React.createElement(React.Fragment, null, props.children);
      },
      Text: () => null,
    },
    '../core/conversation-reveal-measurement': measurements,
    '../core/message-presentation': presentation,
    '../theme': {
      useTheme: () => ({ ...resolveSkyTheme('night', new Date(1000)), localDate: new Date(1000) }),
    },
    '../i18n': { t: (key: string) => key, formatLocale: 'en-US' },
    '../miniapps/timeline-activity': { MiniappTimelineActivityView: () => null },
    './conversation-album-message': { ConversationAlbumMessage: () => null },
  };
  const content = load('../components/conversation-reveal-content-view.tsx', ports);
  ports['./conversation-reveal-content-view'] = content;
  ports['./conversation-message'] = {
    ConversationMessage: ({ entry }: any) => {
      anchor = content.useConversationRevealAnchor(entry.id);
      return null;
    },
  };
  const timeline = load('../components/conversation-timeline-message.tsx', ports);
  assert.deepEqual(
    diagnostics,
    [],
    'the actual components must compile without diagnostic bailouts',
  );
  const message = entry('QA345: contact');
  const report = (id: string, layout: unknown) => reports.push({ id, layout });
  const show = (id = message.id, query = 'QA345') =>
    root.render(
      React.createElement(
        React.StrictMode,
        null,
        React.createElement(timeline.ConversationTimelineMessage, {
          entry: { ...message, id },
          revealQuery: query,
          onRevealLayout: report,
        }),
      ),
    );
  try {
    await show();
    assert.equal(anchor.id, message.id);
    host.onLayout({
      currentTarget: bodyNode,
      nativeEvent: { layout: { x: 0, y: 39 + 2 / 3, width: 842, height: 139 } },
    });
    anchor.onLayout({
      currentTarget: textNode,
      nativeEvent: { layout: { x: 14, y: 10, width: 266, height: 23 } },
    });
    anchor.onTextLayout({
      currentTarget: textNode,
      nativeEvent: {
        lines: [{ text: 'QA345: contact', x: 0, y: 0, width: 200, height: 23 }],
      },
    });
    callbacks.at(-1)!(14, 83 + 2 / 3, 266, 23);
    assert.equal(reports.at(-1)!.layout.anchor.y, 83 + 2 / 3);
    assert.equal(reports.at(-1)!.layout.anchor.height, 23);
    const old = callbacks.at(-1)!;
    await show('next-message');
    const count = reports.length;
    old(14, 90, 266, 23);
    assert.equal(reports.length, count, 'a recycled id cannot publish old geometry');
    assert.equal(anchor.id, 'next-message');
    await root.close();
    closed = true;
    const after = reports.length;
    anchor.onLayout({
      currentTarget: textNode,
      nativeEvent: { layout: { x: 14, y: 10, width: 266, height: 23 } },
    });
    assert.equal(reports.length, after, 'closed search cannot resume from a late native event');
    assert.deepEqual(root.errors, []);
  } finally {
    if (!closed) await root.close();
  }
});

test('actual Compiler message renderer attaches the reveal to text, never the media/contact wrapper', () => {
  for (const message of [
    entry('QA345: contact'),
    plainMessage('QA345: text'),
    entry('**QA345**'),
  ]) {
    const events: unknown[] = [];
    const revealAnchor = {
      id: message.id,
      onLayout: (event: unknown) => events.push(event),
      onTextLayout: (event: unknown) => events.push(event),
    };
    const f = render(message, { revealAnchor });
    const text = elements(f.tree).find(
      (node) =>
        node.type === 'Text' &&
        node.props.children === ('Message' in message.kind ? message.kind.Message.text : ''),
    );
    const markdown = elements(f.tree).find((node) => node.type === 'MessageMarkdown');
    const target = text ?? markdown;
    assert.ok(target);
    assert.equal(
      f.caption?.props.onLayout,
      undefined,
      '44pt caption wrapper is not the text target',
    );
    if (text) {
      (target.props.onLayout as Function)({ nativeEvent: { layout: { width: 266 } } });
      (target.props.onTextLayout as Function)({ nativeEvent: { lines: [] } });
      assert.equal(events.length, 2);
    } else assert.equal(target.props.onLayout, revealAnchor.onLayout);
  }
});
