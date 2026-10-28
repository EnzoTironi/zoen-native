import assert from 'node:assert/strict';
import { test } from 'node:test';
import { t } from '../i18n/index.ts';
import { elements, entry, plainMessage, render, textValues } from './message-caption.fixtures.ts';
import { semanticPreviewFixture } from './semantic-message-preview-react.fixtures.ts';

const target = { spaceId: 'read-chat', messageId: 'original' };
const input = { owner: 'row:reply:r1', text: '**truncated…', literal: false, target };

test('whole quotes project the complete scoped canonical message, while selected excerpts remain exact', async () => {
  const f = await semanticPreviewFixture(input);
  try {
    const canonical = '**' + 'café🦊 '.repeat(40).trim() + '**';
    await f.page([entry(canonical, { id: target.messageId, text_revision: 'revision1' })]);
    await f.render();
    assert.equal(f.reads.length, 1, 'cached text does not replace fresh canonical admission');
    await f.act(() => f.reads[0]!.result.resolve(entry(canonical, { id: target.messageId })));
    assert.equal(f.current().text, undefined, 'pending never falls back to raw Markdown');
    assert.equal(f.projections[0]!.markdown, canonical);
    await f.act(() => f.projections[0]!.result.resolve('café🦊 '.repeat(40).trim()));
    assert.equal(f.current().text, 'café🦊 '.repeat(40).trim());
    const literal = String.raw`**literal** \*quoted\* [a](b) café🦊`;
    await f.render({
      ...input,
      owner: 'selected:r1',
      text: literal,
      literal: true,
      target: undefined,
    });
    assert.equal(f.current().text, literal);
    assert.equal(f.projections.length, 1, 'selected plaintext is never reparsed');
    assert.equal(f.reads.length, 1);
    await f.render({
      ...input,
      owner: 'composer:edited:r2',
      target: undefined,
      text: '**edited source**',
    });
    assert.equal(f.projections[1]!.markdown, '**edited source**');
    await f.act(() => f.projections[1]!.result.resolve('edited source'));
    assert.equal(f.current().text, 'edited source');
  } finally {
    await f.close();
  }
});

test('uncached quotes use the existing exact conversation/message read; generated labels stay literal', async () => {
  const f = await semanticPreviewFixture(input);
  try {
    await f.render();
    assert.deepEqual(f.reads[0]!.command, {
      op: 'message',
      space_id: target.spaceId,
      message_id: target.messageId,
    });
    await f.act(() =>
      f.reads[0]!.result.resolve(entry(String.raw`\"Conversa pronta\"`, { id: target.messageId })),
    );
    assert.equal(f.projections[0]!.markdown, String.raw`\"Conversa pronta\"`);
    await f.act(() => f.projections[0]!.result.resolve('"Conversa pronta"'));
    assert.equal(f.current().text, '"Conversa pronta"');
    await f.render({ ...input, owner: 'attachment', text: 'photo_[1].jpg' });
    await f.act(() => f.reads[1]!.result.resolve(entry('', { id: target.messageId })));
    assert.equal(f.current().text, 'photo_[1].jpg');
    assert.equal(f.projections.length, 1);
  } finally {
    await f.close();
  }
});

test('retired source reads and native results cannot overwrite recycled rows, edited revisions or account reopen', async () => {
  const f = await semanticPreviewFixture(input);
  try {
    await f.render();
    const oldRead = f.reads[0]!;
    await f.render({ ...input, owner: 'recycled:r2', target: undefined, text: '**current**' });
    await f.act(() => oldRead.result.resolve(entry('old read', { id: target.messageId })));
    assert.equal(f.projections.length, 1, 'retired read never reaches the parser');
    const oldProjection = f.projections[0]!;
    await f.render({ ...input, owner: 'recycled:r3', target: undefined, text: '**edited**' });
    await f.act(() => oldProjection.result.resolve('old current'));
    assert.equal(f.current().text, undefined);
    const beforeReopen = f.projections[1]!;
    await f.reopen();
    await f.act(() => beforeReopen.result.resolve('retired account'));
    assert.equal(f.current().text, undefined);
    const latest = f.projections.at(-1)!;
    assert.notEqual(latest, beforeReopen);
    await f.act(() => latest.result.resolve('edited'));
    assert.equal(f.current().text, 'edited');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('page replacement, blur/refocus and unmount retire asynchronous work; refusal never exposes raw source', async () => {
  const f = await semanticPreviewFixture(input);
  try {
    await f.render();
    await f.page([entry('**new page**', { id: target.messageId })]);
    await f.act(() => f.reads[0]!.result.resolve(entry('old page', { id: target.messageId })));
    assert.equal(f.projections.length, 0);
    await f.act(() => f.reads[1]!.result.resolve(entry('**new page**', { id: target.messageId })));
    const blurred = f.projections[0]!;
    await f.focus(false);
    await f.focus(true);
    await f.act(() => f.reads[2]!.result.resolve(entry('**new page**', { id: target.messageId })));
    await f.act(() => blurred.result.resolve('blurred'));
    assert.equal(f.current().text, undefined);
    await f.act(() => f.projections[1]!.result.reject(Error('native refused')));
    assert.equal(f.current().text, t('chat.messageUnavailable'));
    await f.render({ ...input, owner: 'foreign-target' });
    await f.page([]);
    await f.act(() =>
      f.reads.at(-1)!.result.resolve(entry('foreign private message', { id: 'different' })),
    );
    assert.equal(f.current().text, t('chat.messageUnavailable'));
    await f.render({ ...input, owner: 'unmount', target: undefined });
    const retired = f.projections.at(-1)!;
    await f.close();
    retired.result.resolve('after unmount');
    assert.deepEqual(f.errors, []);
  } catch (error) {
    await f.close();
    throw error;
  }
});

test('compiled quoted face keeps two lines, semantic accessibility and the exact canonical navigation target', () => {
  const message = plainMessage('ordinary body', {
    reply_to: { id: 'target-id', author: entry().author, text: String.raw`\"Conversa pronta\"` },
  });
  const f = render(message, { previewText: '"Conversa pronta"' });
  const preview = elements(f.tree).find(
    (node) => node.type === 'Text' && node.props.numberOfLines === 2,
  )!;
  assert.equal(preview.props.children, '"Conversa pronta"');
  assert.ok(!textValues(f.tree).includes(message.reply_to!.text));
  const quoteControl = elements(f.tree).find(
    (node) => node.props.accessibilityLabel === 'chat.goToQuotedMessage',
  )!;
  (quoteControl.props.onPress as () => void)();
  assert.deepEqual(f.quotes, ['target-id']);
  const wrapper = elements(f.tree).find((node) => node.type === f.interactionType)!;
  assert.ok(String(wrapper.props.accessibilityLabel).includes('"Conversa pronta"'));
  assert.ok(!String(wrapper.props.accessibilityLabel).includes(message.reply_to!.text));
  const pending = render(message, { previewText: null });
  assert.ok(!textValues(pending.tree).includes(message.reply_to!.text));
  assert.ok(textValues(pending.tree).includes('…'));
});

test('matching canonical change retires a queued native result before the held snapshot reveals lost membership', async () => {
  const f = await semanticPreviewFixture(input);
  try {
    await f.render();
    await f.act(() =>
      f.reads[0]!.result.resolve(entry('**private old text**', { id: target.messageId })),
    );
    const held = f.projections[0]!;
    const refresh = await f.refresh([target.spaceId]);
    await f.act(() => held.result.resolve('private old text'));
    assert.notEqual(
      f.current().text,
      'private old text',
      'canonical notification fences before snapshot resolves',
    );
    const removed = structuredClone(f.snapshot);
    removed.conversations[0]!.members = [];
    await f.act(async () => {
      f.snapshotReads[0]!.resolve(removed);
      await refresh.done;
    });
    assert.equal(f.current().text, t('chat.messageUnavailable'));
    assert.equal(f.reads.length, 1, 'cached timeline never grants access after membership loss');
    const restored = await f.refresh([target.spaceId]);
    await f.act(async () => {
      f.snapshotReads[1]!.resolve(structuredClone(f.snapshot));
      await restored.done;
    });
    await f.act(() =>
      f.reads[1]!.result.resolve(entry('**fresh text**', { id: target.messageId })),
    );
    await f.act(() => f.projections[1]!.result.resolve('fresh text'));
    assert.equal(f.current().text, 'fresh text');
  } finally {
    await f.close();
  }
});

test('unrelated canonical refresh keeps a valid projection; global invalidation hides it even if refresh refuses', async () => {
  const f = await semanticPreviewFixture(input);
  try {
    await f.render();
    await f.act(() => f.reads[0]!.result.resolve(entry('**current**', { id: target.messageId })));
    const unrelated = await f.refresh(['other-conversation']);
    await f.act(() => f.projections[0]!.result.resolve('current'));
    assert.equal(f.current().text, 'current');
    await f.act(async () => {
      f.snapshotReads[0]!.reject(Error('unrelated refresh refused'));
      await unrelated.done;
    });
    assert.equal(f.current().text, 'current');
    const global = await f.refresh();
    assert.equal(
      f.current().text,
      t('chat.messageUnavailable'),
      'already displayed receipt is retired immediately',
    );
    await f.act(async () => {
      f.snapshotReads[1]!.reject(Error('membership read refused'));
      await global.done;
    });
    assert.equal(f.current().text, t('chat.messageUnavailable'));
    assert.equal(f.reads.length, 1, 'no retry loop or cache-only reauthorization');
  } finally {
    await f.close();
  }
});
