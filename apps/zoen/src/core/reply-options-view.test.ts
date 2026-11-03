import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { plainMessage } from './message-caption.fixtures.ts';

const jsx = (type: unknown, props: any) => ({ type, props });
const { ReplyOptions } = loadEditorAgent('../components/reply-options.tsx', {
  'react/compiler-runtime': {
    c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')),
  },
  'react/jsx-runtime': { jsx, jsxs: jsx },
  'react-native': { Text: 'Text', View: 'View' },
  './message-actions': { MessageActions: 'Actions' },
  './message-markdown': { MessageMarkdown: 'Markdown' },
  '../core/presentation': { messageText: (entry: any) => entry.kind.Message.text },
  '../theme': { useTheme: () => ({ colors: {} }) },
  '../i18n': { t: (key: string) => key },
});
test('reply menu offers native selection only for actual source text and retains explicit draft actions', () => {
  let selected = 0,
    applied = 0,
    removed = 0,
    canceled = 0;
  const options = {
    entry: { ...plainMessage('Actual caption'), text_revision: 'signed' },
    selection: null,
    selecting: false,
    pending: false,
    error: null,
    canMove: true,
    select: () => selected++,
    apply: () => applied++,
    remove: () => removed++,
    cancelReply: () => canceled++,
    move() {},
    close() {},
    review() {},
  };
  let menu = ReplyOptions({ options }).props.options;
  menu.find((row: any) => row.label === 'replyOptions.select').run();
  menu.find((row: any) => row.label === 'replyOptions.apply').run();
  assert.equal(selected, 1);
  assert.equal(applied, 1);
  menu = ReplyOptions({ options: { ...options, selection: { text: 'Actual' } } }).props.options;
  assert.ok(menu.some((row: any) => row.label === 'replyOptions.change'));
  menu.find((row: any) => row.label === 'replyOptions.remove').run();
  const cancel = menu.find((row: any) => row.label === 'replyOptions.cancel');
  cancel.run();
  assert.equal(removed, 1);
  assert.equal(canceled, 1);
  assert.equal(cancel.keepOpen, true, 'controller finishes its action before native dismissal');
  for (const text of ['', '   ']) {
    menu = ReplyOptions({
      options: { ...options, entry: { ...options.entry, kind: { Message: { text } } } },
    }).props.options;
    assert.equal(
      menu.some((row: any) => row.label === 'replyOptions.select'),
      false,
    );
    assert.ok(menu.some((row: any) => row.label === 'replyOptions.cancel'));
  }
});
