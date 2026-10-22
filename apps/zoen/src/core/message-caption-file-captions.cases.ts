import assert from 'node:assert/strict';
import { test } from 'node:test';
import { contrastRatio, SKY_PHASES } from '../theme/sky.ts';
import type { Element } from './message-caption.fixtures.ts';
import { elements, entry, render, styleOf, textValues } from './message-caption.fixtures.ts';
import type { TimelineEntry } from './types';

test('actual message renderer shows exact canonical file caption once below the media boundary', () => {
  for (const caption of [
    '  Caption with spaces\nand another line 🇧🇷  ',
    'Arquivo: video.mp4',
    'video.mp4',
  ]) {
    const f = render(entry(caption, { forwarded: true }));
    assert.ok(f.caption);
    assert.equal(textValues(f.tree).filter((text) => text === caption).length, 1);
    const bubble = elements(f.tree).find(
      (node) => node.props.testID === 'message-bubble-caption_message',
    );
    assert.ok(bubble);
    assert.equal(bubble.type, 'View', 'Native media keeps its non-pressable ancestor');
    const contents = elements(bubble.props.children);
    const mediaIndex = contents.findIndex((node) => node.type === 'MessageMedia');
    assert.ok(mediaIndex >= 0);
    assert.ok(contents.indexOf(f.caption) > mediaIndex);
    assert.equal(
      styleOf(f.caption).position,
      undefined,
      'Caption is laid out below arbitrary image pixels',
    );
  }
});

test('file captions retain the selected Telegram palette, text size and independent caption layout', () => {
  for (const phase of SKY_PHASES) {
    for (const mine of [false, true]) {
      const message = entry();
      const f = render({ ...message, author: { ...message.author, is_me: mine } }, { phase });
      assert.ok(f.caption);
      const paper = styleOf(f.caption).backgroundColor;
      const text = elements(f.caption).find((node) => node.type === 'Text');
      assert.ok(text);
      const ink = styleOf(text).color;
      assert.equal(paper, mine ? f.theme.colors.mine : f.theme.colors.theirs);
      assert.equal(ink, mine ? f.theme.colors.onMine : f.theme.colors.text);
      if (phase === 'day' || !mine) assert.ok(contrastRatio(ink as string, paper as string) >= 4.5);
      else
        assert.deepEqual(
          f.theme.presentation.chat.outgoing.fillColors,
          ['#61BCF9', '#0088FF'],
          'retain the pinned default gradient; custom colors have their own contrast test',
        );
      assert.equal(styleOf(text).fontSize, 17);
      assert.equal(styleOf(text).lineHeight, 23);
      assert.ok(Number(styleOf(f.caption).minHeight) >= 44);
    }
  }
});

test('deleted, voice, miniapp and blank messages never render a file caption', () => {
  const retained = 'Private retained caption';
  const messages = [
    entry(retained, { deleted: true }),
    entry(retained, { voice: { item_id: 'voice' } as TimelineEntry['voice'] }),
    entry(retained, {
      kind: {
        Message: { text: retained, card: { item_id: 'miniapp', kind_id: 'app' } },
      } as TimelineEntry['kind'],
    }),
    entry(' \n\t '),
    entry('', {}),
  ];
  for (const message of messages) {
    const f = render(message);
    assert.equal(f.caption, undefined);
    if (message.deleted || message.voice) assert.ok(!textValues(f.tree).includes(retained));
  }
  const plain = 'An ordinary text message';
  const f = render(entry(plain, { kind: { Message: { text: plain, card: null } } }));
  assert.equal(f.caption, undefined);
  assert.equal(textValues(f.tree).filter((text) => text === plain).length, 1);
});

test('caption native press and hold use existing actions while consumed swipes suppress trailing presses', () => {
  const message = entry();
  const f = render(message);
  assert.ok(f.caption);
  assert.ok('Message' in message.kind);
  assert.equal(f.caption.props.accessibilityRole, 'button');
  assert.equal(f.caption.props.accessibilityLabel, `Sender: ${message.kind.Message.text}`);
  const native = f.nativeCaption();
  (native.props.onPress as () => void)();
  (native.props.onLongPress as () => void)();
  assert.equal(f.calls[0]?.mode, 'reactions');
  assert.equal(f.calls[1]?.mode, 'actions');
  assert.equal(f.calls[0]?.entry, message);
  f.pan(true);
  (native.props.onPress as () => void)();
  (native.props.onLongPress as () => void)();
  assert.equal(f.calls.length, 2);
  (native.props.onAccessibilityTap as () => void)();
  assert.equal(f.calls.length, 3, 'Accessibility action bypasses physical swipe consumption');
  assert.equal(f.calls[2]?.mode, 'actions');
  const interaction = elements(f.tree).find((node) => node.type === f.interactionType);
  assert.ok(interaction);
  assert.ok(elements(interaction).includes(f.caption));
});

test('disabled captions stay readable without opening message actions', () => {
  const f = render(entry(), { disabled: true });
  assert.ok(f.caption);
  assert.equal(f.caption.props.disabled, true);
  assert.equal(f.caption.props.accessibilityHint, undefined);
  const native = f.nativeCaption();
  (native.props.onPress as () => void)();
  (native.props.onLongPress as () => void)();
  assert.equal(f.calls.length, 0);
});

test('caption tails follow the existing incoming and outgoing run grouping', () => {
  for (const mine of [false, true]) {
    const message = entry();
    message.author = { ...message.author, is_me: mine };
    const ended = render(message);
    const continued = render(message, { next: { ...message, id: 'next', at_ms: '2000' } });
    assert.ok(ended.caption && continued.caption);
    const corner = mine ? 'borderBottomRightRadius' : 'borderBottomLeftRadius';
    assert.equal(styleOf(ended.caption)[corner], 6);
    assert.equal(styleOf(continued.caption)[corner], 22);
  }
});

test('decoded image dimensions fit the actual bubble, caption and metadata and recompute on narrow and rotated viewports', () => {
  for (const mine of [false, true]) {
    const message = entry();
    message.author = { ...message.author, is_me: mine };
    const f = render(message);
    const media = elements(f.tree).find((node) => node.type === 'MessageMedia')!;
    const decode = media.props.onImageDecoded as (image: Record<string, unknown>) => void;
    decode({ itemId: 'signed_file', version: 1, sha256: 'a'.repeat(64), width: 700, height: 1000 });
    let tree = f.rerender();
    const bubble = (value: Element) =>
      elements(value).find((node) => node.props.testID === 'message-bubble-caption_message')!;
    assert.ok(Math.abs(Number(styleOf(bubble(tree)).width) - 252) < 1e-6);
    const caption = elements(bubble(tree)).find(
      (node) => node.props.testID === 'message-caption-caption_message',
    )!;
    assert.ok(caption, 'Caption stays within the fitted image bubble');
    assert.equal(
      styleOf(caption).width,
      undefined,
      'Caption stretches to its bubble rather than a separate fixed width',
    );
    if (!mine)
      assert.ok(
        elements(tree).some(
          (node) =>
            Math.abs(Number(styleOf(node)?.width) - 252) < 1e-6 &&
            styleOf(node)?.maxWidth === '100%',
        ),
        'Incoming metadata shares the same fitted width',
      );
    tree = f.rerender(message, 240);
    assert.ok(Number(styleOf(bubble(tree)).width) <= 240 - 32 - (mine ? 0 : 44));
    decode({
      itemId: 'signed_file',
      version: 1,
      sha256: 'a'.repeat(64),
      width: 2000,
      height: 1000,
    });
    const narrowWidth = Number(styleOf(bubble(f.rerender())).width);
    const wideWidth = Number(styleOf(bubble(f.rerender(message, 900))).width);
    assert.ok(
      wideWidth > narrowWidth,
      'Fit uses current space rather than retaining the prior fitted width',
    );
    assert.equal(wideWidth, 294);
  }
});

test('late decoded dimensions from a replaced file version cannot overwrite the new image geometry', () => {
  const original = entry();
  const f = render(original);
  const oldDecode = elements(f.tree).find((node) => node.type === 'MessageMedia')!.props
    .onImageDecoded as (image: Record<string, unknown>) => void;
  const replacement = entry();
  assert.ok('Message' in replacement.kind && replacement.kind.Message.card);
  replacement.kind.Message.card.version = 2;
  let tree = f.rerender(replacement);
  const decode = elements(tree).find((node) => node.type === 'MessageMedia')!.props
    .onImageDecoded as (image: Record<string, unknown>) => void;
  decode({ itemId: 'signed_file', version: 2, sha256: 'b'.repeat(64), width: 700, height: 1000 });
  tree = f.rerender(replacement);
  const width = () =>
    styleOf(elements(tree).find((node) => node.props.testID === 'message-bubble-caption_message')!)
      .width;
  assert.ok(Math.abs(Number(width()) - 252) < 1e-6);
  oldDecode({
    itemId: 'signed_file',
    version: 1,
    sha256: 'a'.repeat(64),
    width: 2000,
    height: 1000,
  });
  tree = f.rerender(replacement);
  assert.ok(Math.abs(Number(width()) - 252) < 1e-6);
  decode({ itemId: 'another-file', version: 2, sha256: 'c'.repeat(64), width: 2000, height: 1000 });
  tree = f.rerender(replacement);
  assert.ok(Math.abs(Number(width()) - 252) < 1e-6);
});
