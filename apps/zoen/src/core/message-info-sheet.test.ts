import * as messagePresentation from './message-presentation.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as jsx from 'react/jsx-runtime';
import { build } from 'esbuild';
import * as dates from './message-info-presentation.ts';
import { draftSession } from './store.fixtures.ts';
import type { MessageInfo } from './generated/MessageInfo';
const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../components/message-info-sheet.tsx', import.meta.url),
      'utf8',
    ),
    loader: 'tsx',
  },
  bundle: true,
  external: ['*'],
  jsx: 'automatic',
  platform: 'node',
  format: 'cjs',
  write: false,
});
const module = { exports: {} as { MessageInfoSheet(props: any): any } };
const ports: Record<string, unknown> = {
  'react/jsx-runtime': jsx,
  'react-native': { ScrollView: 'ScrollView', View: 'View', Text: 'Text' },
  '../core/message-info-presentation': dates,
  '../core/message-presentation': messagePresentation,
  '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en' },
  '../theme': { useTheme: () => ({ colors: {}, presentation: { list: {} } }) },
  '../profile/profile-avatar': { ProfileAvatar: 'Avatar' },
  './primitives': { NativeButton: 'Button', Loading: 'Loading' },
  './message-receipt': { messageDeliveryLabel: (value: string) => value },
};
runInNewContext(compiled.outputFiles[0]!.text, {
  module,
  exports: module.exports,
  require(name: string) {
    assert.ok(name in ports, name);
    return ports[name];
  },
});
function text(node: any): string {
  return Array.isArray(node)
    ? node.map(text).join(' ')
    : node && typeof node === 'object'
      ? text(node.props?.children)
      : node == null
        ? ''
        : String(node);
}
function info(): MessageInfo {
  return {
    scope: {
      expected_identity_id: 'owner',
      expected_device_id: 'device',
      space_id: 'space',
      message_id: 'message',
    },
    author: draftSession().conversations[0]!.members[0]!,
    sent_at_ms: '1000',
    forwarded: false,
    edited: true,
    edited_at_ms: '2000',
    delivery: 'Read',
    read_details: 'available',
    participants: [
      {
        person: { ...draftSession().conversations[0]!.members[0]!, id: 'peer', name: 'Peer' },
        delivered: true,
        read: true,
        read_reported_at_ms: '3000',
        read_observed_at_ms: '90000000000',
      },
    ],
  };
}
test('actual compiled Info sheet shows precise author/edit/peer dates without substituting local observation', () => {
  const value = info();
  const output = text(
    module.exports.MessageInfoSheet({ value, loading: false, error: false, retry() {} }),
  );
  assert.ok(!output.includes('messageInfo.authorClock'));
  assert.ok(!output.includes('messageInfo.peerClock'));
  for (const at of ['1000', '2000', '3000'])
    assert.ok(output.includes(dates.messageInfoDate(at, 'en')!));
  assert.ok(!output.includes(dates.messageInfoDate('90000000000', 'en')!));
  value.participants[0]!.read_reported_at_ms = null;
  assert.ok(
    text(module.exports.MessageInfoSheet({ value })).includes('messageInfo.timeUnavailable'),
  );
});
test('actual compiled Info sheet distinguishes load error from absent metadata and suppresses participant detail when unauthorized', () => {
  const value = info();
  value.read_details = 'not_sender';
  value.participants = [];
  assert.ok(!text(module.exports.MessageInfoSheet({ value })).includes('Peer'));
  assert.ok(
    text(module.exports.MessageInfoSheet({ value: null, error: true })).includes(
      'messageInfo.loadFailed',
    ),
  );
  assert.ok(
    text(module.exports.MessageInfoSheet({ value: null, error: false })).includes(
      'messageInfo.unavailable',
    ),
  );
});

test('compiled message info shows the preserved author without a profile control or read grant', () => {
  const value = { ...info(), forwarded: true, forwarded_author: 'Original 雨' };
  const output = text(module.exports.MessageInfoSheet({ value }));
  assert.ok(output.includes('Original 雨'));
  assert.ok(output.includes('Forwarded from'));
  const ordinary = text(module.exports.MessageInfoSheet({ value: { ...value, forwarded: false } }));
  assert.ok(!ordinary.includes('Original 雨'));
});
