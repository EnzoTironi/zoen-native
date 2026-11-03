import assert from 'node:assert/strict';
import { test } from 'node:test';
import { actions, elements, fixture, message, quote } from './message-accessibility.fixtures.ts';
import type { RenderedMessageText } from '../components/use-rendered-message-text';

function rendered(tree: unknown): RenderedMessageText {
  const body = elements(tree).find((node) => node.type === 'MessageMarkdown');
  assert.ok(body, 'the actual formatted face must retain the native Markdown body');
  return body.props.accessibilityText as RenderedMessageText;
}

test('native display receipt replaces raw source in the actual grouped label without losing metadata/actions', async () => {
  const raw = String.raw`QA753\: nota em Mensagens Salvas\.`;
  const entry = message({
    kind: { Message: { text: raw, card: null } },
    reply_to: quote('original', 'Quoted body'),
    edited: true,
    forwarded: true,
  });
  const f = fixture(entry);
  let output = await f.enable();
  assert.ok(!(output.group.props.accessibilityLabel as string).includes(raw));
  const receipt = rendered(output.tree);
  receipt.accept({
    revision: receipt.revision,
    markdown: raw,
    text: 'QA753: nota em Mensagens Salvas.',
  });
  output = f.render();
  const label = output.group.props.accessibilityLabel as string;
  for (const expected of [
    'Nina Author: QA753: nota em Mensagens Salvas.',
    'Replying to Quoted original: Quoted body',
    'Edited at',
    'Forwarded',
  ])
    assert.ok(label.includes(expected), expected);
  assert.deepEqual(
    Array.from(actions(output.group) ?? [], (action) => action.name),
    ['reply', 'thread', 'quote', 'actions'],
  );
  const stale = receipt;
  output = f.recycle({ ...entry, kind: { Message: { text: '**changed**', card: null } } });
  assert.ok(!(output.group.props.accessibilityLabel as string).includes('QA753'));
  const writes = f.writes();
  stale.accept({ revision: stale.revision, markdown: raw, text: 'stale previous body' });
  assert.equal(f.writes(), writes);
  assert.ok(!(f.render().group.props.accessibilityLabel as string).includes('stale'));
  f.unmount();
});
