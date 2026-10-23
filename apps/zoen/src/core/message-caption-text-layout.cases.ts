import assert from 'node:assert/strict';
import { test } from 'node:test';
import { contrastRatio, SKY_PHASES } from '../theme/sky.ts';
import {
  elements,
  entry,
  measureText,
  nodeById,
  plainMessage,
  render,
  styleOf,
  textValues,
} from './message-caption.fixtures.ts';
import type { TimelineEntry } from './types';

test('plain text uses compact Telegram metrics while canonical App/media presentation keeps its policy', () => {
  const msg = plainMessage();
  const f = render(msg);
  const bubble = nodeById(f.tree, `message-bubble-${msg.id}`);
  const style = styleOf(bubble);
  assert.equal(style.maxWidth, 345);
  assert.equal(style.minWidth, 40);
  assert.equal(style.minHeight, 35);
  assert.equal(style.borderRadius, 16);
  assert.equal(style.paddingHorizontal, 11);
  assert.equal(style.paddingTop, 6 + 1 / 3);
  assert.equal(style.paddingBottom, 6 - 1 / 3);
  assert.equal(styleOf(nodeById(f.tree, `message-row-${msg.id}`)).marginTop, 2 + 1 / 3);
  assert.equal(styleOf(nodeById(f.tree, `message-body-${msg.id}`)).fontSize, 17);
  assert.equal(styleOf(nodeById(f.tree, `message-body-${msg.id}`)).lineHeight, undefined);
  const app = entry('App activity', {
    author: { ...entry().author, kind: 'Agent' },
    kind: {
      Message: {
        text: 'App activity',
        card: { item_id: 'app', kind_id: 'app', app: { app_id: 'deckcraft' } },
      },
    } as TimelineEntry['kind'],
  });
  const appTree = render(app).tree;
  assert.equal(styleOf(nodeById(appTree, `message-bubble-${app.id}`)).width, 164);
  const appMetadata = nodeById(appTree, `message-metadata-${app.id}`);
  assert.equal(styleOf(appMetadata).paddingBottom, 6);
  assert.equal(styleOf(appMetadata).paddingHorizontal, 11);
  assert.equal(
    elements(appTree).find((node) => node.type === 'MiniappMessageCard')?.props.width,
    160,
  );
  assert.equal(nodeById(appTree, `message-body-${app.id}`).props.onTextLayout, undefined);
});

test('core grouping drives zero merged spacing and the correct 8pt same-side corners', () => {
  for (const mine of [false, true]) {
    const msg = plainMessage('One', { author: { ...entry().author, is_me: mine } });
    const previous = { ...msg, id: 'previous' };
    const next = { ...msg, id: 'next' };
    const f = render(msg, { previous, next });
    const style = styleOf(nodeById(f.tree, `message-bubble-${msg.id}`));
    assert.equal(styleOf(nodeById(f.tree, `message-row-${msg.id}`)).marginTop, 0);
    assert.equal(style[mine ? 'borderTopRightRadius' : 'borderTopLeftRadius'], 8);
    assert.equal(style[mine ? 'borderBottomRightRadius' : 'borderBottomLeftRadius'], 8);
    assert.equal(style[mine ? 'borderTopLeftRadius' : 'borderTopRightRadius'], 16);
  }
});

test('actual native trailing-line measurements place time inline without estimating characters', () => {
  const msg = plainMessage('OK');
  const f = render(msg);
  assert.equal(styleOf(nodeById(f.tree, `message-metadata-${msg.id}`)).position, undefined);
  measureText(f.tree, msg.id);
  const tree = f.rerender();
  assert.equal(styleOf(nodeById(tree, `message-bubble-${msg.id}`)).minWidth, 113);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${msg.id}`)).position, 'absolute');
  assert.equal(styleOf(nodeById(tree, `message-metadata-${msg.id}`)).right, 17);
  assert.equal(textValues(tree).filter((text) => text === 'OK').length, 1);
  const receipt = elements(tree).find((node) => node.type === 'MessageReceipt');
  assert.equal(receipt?.props.delivery, msg.delivery);
});

test('wrapped text only shares a trailing line that has measured space; RTL and tall metadata keep a row', () => {
  for (const [text, right, height, inline] of [
    ['A wrapped message', 160, 21, true],
    ['A wrapped message', 280, 21, false],
    ['مرحبا hello', 20, 21, false],
    ['A wrapped message', 160, 10, false],
  ] as const) {
    const msg = plainMessage(text);
    const f = render(msg);
    measureText(f.tree, msg.id, { right, count: 2, width: 300, height });
    assert.equal(
      styleOf(nodeById(f.rerender(), `message-metadata-${msg.id}`)).position,
      inline ? 'absolute' : undefined,
    );
  }
});

test('recycled message, viewport, delivery and font changes reject stale native layout callbacks', () => {
  const original = plainMessage('Original');
  const f = render(original);
  const oldBody = nodeById(f.tree, `message-body-${original.id}`);
  const oldStatus = nodeById(f.tree, `message-metadata-${original.id}`);
  const next = plainMessage('New owner', { id: 'next_owner', delivery: 'Read', edited: true });
  let tree = f.rerender(next);
  (oldBody.props.onTextLayout as Function)({
    nativeEvent: { lines: [{ x: 0, width: 20, height: 21 }] },
  });
  (oldBody.props.onLayout as Function)({ nativeEvent: { layout: { width: 300 } } });
  (oldStatus.props.onLayout as Function)({ nativeEvent: { layout: { width: 60, height: 14 } } });
  tree = f.rerender(next);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${next.id}`)).position, undefined);
  measureText(tree, next.id);
  tree = f.rerender(next);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${next.id}`)).position, 'absolute');
  tree = f.rerender(next, 240, 2);
  assert.equal(styleOf(nodeById(tree, `message-metadata-${next.id}`)).position, undefined);
  assert.equal(styleOf(nodeById(tree, `message-bubble-${next.id}`)).maxWidth, 195);
  assert.equal(
    elements(tree).find((node) => node.type === 'MessageReceipt')?.props.delivery,
    'Read',
  );
});

test('plain bubble ink and paper retain every actual Zoen sky theme and message side', () => {
  for (const phase of SKY_PHASES)
    for (const mine of [false, true]) {
      const msg = plainMessage('Read me', { author: { ...entry().author, is_me: mine } });
      const f = render(msg, { phase });
      const graphic = elements(f.tree).find(
        (node) => typeof node.type === 'function' && node.type.name === 'MessageBubbleGraphic',
      );
      assert.ok(graphic);
      const paper = graphic.props.fill as string;
      const ink = styleOf(nodeById(f.tree, `message-body-${msg.id}`)).color as string;
      if (phase === 'day' || !mine) assert.ok(contrastRatio(ink, paper) >= 4.5);
      else assert.equal(ink, '#FFFFFF', 'the default Night gradient uses the pinned white ink');
      assert.equal(paper, mine ? f.theme.colors.mine : f.theme.colors.theirs);
    }
});

test('plain bubble fill uses viewport coordinates before edge and author insets at the exact 500pt boundary', () => {
  const cases = [
    [240, 195, 151],
    [402, 357, 313],
    [500, 455, 411],
    [501, 416, 372],
    [520, 433, 389],
    [120, 75, 40],
  ] as const;
  for (const [width, ownBound, incomingBound] of cases)
    for (const mine of [true, false]) {
      const msg = plainMessage('A long enough actual text body', {
        author: { ...entry().author, is_me: mine },
      });
      const f = render(msg);
      const tree = f.rerender(msg, width);
      const bound = styleOf(nodeById(tree, `message-bubble-${msg.id}`)).maxWidth;
      assert.equal(bound, mine ? ownBound : incomingBound, `${width}pt mine=${mine}`);
      assert.ok(
        Number(bound) <= width - 32 - (mine ? 0 : 44),
        'Never overflow the actual available row slot',
      );
    }
});

test('text and App shares own the native background; file and voice boundaries retain their own rendering', () => {
  for (const mine of [true, false]) {
    const msg = plainMessage('Editable text', { author: { ...entry().author, is_me: mine } });
    const f = render(msg);
    const bubble = nodeById(f.tree, `message-bubble-${msg.id}`);
    const graphics = elements(f.tree).filter(
      (node) => typeof node.type === 'function' && node.type.name === 'MessageBubbleGraphic',
    );
    assert.equal(graphics.length, 1);
    assert.equal(graphics[0]!.props.ownerKey, msg.id);
    assert.equal(graphics[0]!.props.incoming, !mine);
    assert.equal(
      styleOf(bubble).backgroundColor,
      'transparent',
      'Sky remains visible through the actual curved tail cutout',
    );
    assert.equal(styleOf(bubble).paddingLeft, mine ? 11 : 17);
    assert.equal(styleOf(bubble).paddingRight, mine ? 17 : 11);
  }
  for (const msg of [
    entry('caption'),
    entry('', { voice: { item_id: 'voice' } as TimelineEntry['voice'] }),
    entry('App', {
      kind: {
        Message: {
          text: 'App',
          card: { item_id: 'app', kind_id: 'app', app: { app_id: 'deckcraft' } },
        },
      } as TimelineEntry['kind'],
    }),
  ]) {
    assert.equal(
      elements(render(msg).tree).filter(
        (node) => typeof node.type === 'function' && node.type.name === 'MessageBubbleGraphic',
      ).length,
      'Message' in msg.kind &&
        msg.kind.Message.card?.kind_id === 'app' &&
        msg.author.kind === 'Agent'
        ? 1
        : 0,
    );
  }
});

test('actual renderer binds receipt contrast to bubble gradient stops or the separate media metadata surface', () => {
  for (const phase of SKY_PHASES) {
    for (const file of [false, true]) {
      const message = entry('Exact canonical content', { delivery: 'Read' });
      message.author = { ...message.author, is_me: true };
      if (!file)
        message.kind = {
          Message: { text: 'Exact canonical content', card: null },
        } as TimelineEntry['kind'];
      const f = render(message, { phase });
      const receipt = elements(f.tree).find((node) => node.type === 'MessageReceipt');
      assert.ok(receipt);
      assert.equal(receipt.props.delivery, 'Read');
      if (file) {
        assert.equal(
          receipt.props.backgrounds,
          undefined,
          'media metadata is outside its caption bubble',
        );
        assert.equal(receipt.props.background, f.theme.colors.background);
        assert.equal(receipt.props.ink, f.theme.colors.secondary);
      } else {
        assert.deepEqual(receipt.props.backgrounds, f.theme.presentation.chat.outgoing.fillColors);
        assert.equal(receipt.props.background, f.theme.presentation.chat.outgoing.fill);
        assert.equal(receipt.props.ink, f.theme.presentation.chat.outgoing.text);
      }
      assert.equal(
        textValues(f.tree).filter((text) => text === 'Exact canonical content').length,
        1,
      );
    }
  }
});
