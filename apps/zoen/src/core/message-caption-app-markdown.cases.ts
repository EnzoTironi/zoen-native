import assert from 'node:assert/strict';
import { test } from 'node:test';
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
import { messageMayContainMarkdown } from './message-markdown.ts';
import type { TimelineEntry } from './types';

test('album cells keep the actual canonical renderer and per-item reply, thread, menu and decode targets', () => {
  const message = entry('Caption for the second canonical member');
  let decoded: unknown;
  const tile = { width: 120, height: 200 };
  const f = render(message, {
    albumPart: {
      tile,
      onDecoded: (image) => {
        decoded = image;
      },
    },
  });
  const interaction = elements(f.tree).find((node) => node.type === f.interactionType)!;
  assert.ok(interaction);
  assert.equal(interaction.props.messageId, message.id);
  (interaction.props.onReply as () => void)();
  (interaction.props.onThread as () => void)();
  (interaction.props.onActions as () => void)();
  assert.deepEqual(
    f.calls.map((call) => call.mode),
    ['reply', 'thread', 'actions'],
  );
  assert.ok(f.calls.every((call) => call.entry === message));
  const media = elements(f.tree).find((node) => node.type === 'MessageMedia')!;
  assert.strictEqual(media.props.entry, message);
  assert.deepEqual(media.props.tile, tile);
  const dimensions = {
    itemId: 'signed_file',
    version: 1,
    sha256: 'a'.repeat(64),
    width: 40,
    height: 80,
  };
  (media.props.onImageDecoded as (image: unknown) => void)(dimensions);
  assert.strictEqual(decoded, dimensions);
  assert.equal(
    textValues(f.tree).includes(
      message.kind && 'Message' in message.kind ? message.kind.Message.text : '',
    ),
    false,
  );
  const caption = render(message, { albumPart: { captionOnly: true } });
  assert.equal(
    textValues(caption.tree).filter((value) => value === 'Caption for the second canonical member')
      .length,
    1,
  );
  assert.equal(
    elements(caption.tree).some((node) => node.type === 'MessageMedia'),
    false,
  );
});

test('typed human App shares show one160pt cover without caption/bubble; Agent text stays exact', () => {
  const caption = 'Apresentação adicionada a esta conversa';
  const card = {
    item_id: 'deck',
    kind_id: 'app',
    title: 'Presentation',
    version: 1,
    app: { app_id: 'deckcraft' },
  };
  for (const kind of ['Person', 'Agent'] as const) {
    const message = entry(caption, {
      author: { ...entry().author, kind },
      kind: { Message: { text: caption, card } } as TimelineEntry['kind'],
    });
    const f = render(message);
    const all = elements(f.tree);
    const body = all.filter((n) => n.props.testID === `message-body-${message.id}`);
    const cover = all.find((n) => n.type === 'MiniappMessageCard')!;
    assert.ok(cover);
    assert.equal(cover.props.card, card);
    assert.equal(cover.props.width, 160);
    assert.equal(body.length, kind === 'Agent' ? 1 : 0);
    assert.equal(
      all.some((n) => n.props.testID === `message-bubble-${message.id}`),
      kind === 'Agent',
    );
    if (kind === 'Agent') assert.equal(body[0]!.props.children, caption);
    else {
      assert.ok(all.some((n) => n.props.testID === `message-app-card-${message.id}`));
      assert.ok(all.every((n) => n.props.children !== caption));
      assert.ok(all.some((n) => n.props.testID === `message-metadata-${message.id}`));
    }
  }
});

test('approval and resolved receipts retain canonical text in the shared compact face and exact review route', () => {
  for (const approved of [null, true, false]) {
    const f = render(
      entry('', {
        kind: {
          Request: { request_id: 'canonical_request', title: 'Read presentation', approved },
        },
      }),
    );
    assert.equal(f.tree.type, 'ServiceMessage');
    assert.equal(f.tree.props.label, 'Read presentation');
    (f.tree.props.onPress as () => void)();
    assert.deepEqual(JSON.parse(JSON.stringify(f.routes)), [
      { pathname: '/request/[id]', params: { id: 'canonical_request' } },
    ]);
  }
  const receipt = 'Approved once. Waiting for the reviewed action.';
  const f = render(entry('', { kind: { System: { text: receipt } } }));
  assert.equal(f.tree.type, 'ServiceMessage');
  assert.equal(f.tree.props.label, receipt);
  assert.equal(f.tree.props.onPress, undefined);
  const disabled = render(
    entry('', { kind: { Request: { request_id: 'r', title: 'Denied', approved: false } } }),
    { disabled: true },
  );
  assert.equal(disabled.tree.props.onPress, undefined);
});

test('formatted bodies, file captions and album captions share native Markdown without changing signed source', () => {
  const text = '**Title**\n\n- one\n- two\n\n```ts\nconst exact = 1;\n```';
  const message = plainMessage(text);
  const body = render(message);
  const formatted = nodeById(body.tree, `message-body-${message.id}`);
  assert.equal(formatted.type, 'MessageMarkdown');
  assert.equal(formatted.props.text, text);
  assert.equal(formatted.props.maxWidth, 317);
  assert.equal(formatted.props.onTextLayout, undefined);
  assert.equal(styleOf(nodeById(body.tree, `message-metadata-${message.id}`)).position, undefined);
  const file = entry(text);
  for (const options of [{}, { albumPart: { captionOnly: true } }]) {
    const f = render(file, options);
    const native = elements(f.tree).find((node) => node.type === 'MessageMarkdown')!;
    assert.ok(native);
    assert.equal(native.props.text, text);
    assert.equal(native.props.maxWidth, 245.18);
  }
  const app = entry(text, {
    author: { ...file.author, kind: 'Agent' },
    kind: {
      Message: { text, card: { item_id: 'deck', kind_id: 'app', app: { app_id: 'deckcraft' } } },
    } as TimelineEntry['kind'],
  });
  const appBody = nodeById(render(app).tree, `message-body-${app.id}`);
  assert.equal(appBody.type, 'MessageMarkdown');
  assert.equal(appBody.props.text, text);
  assert.equal(appBody.props.maxWidth, 142);
});

test('formatting retires prior inline status measurements and iOS sends possible media to the guarded native renderer', () => {
  const original = plainMessage('Short');
  const f = render(original);
  measureText(f.tree, original.id);
  assert.equal(
    styleOf(nodeById(f.rerender(), `message-metadata-${original.id}`)).position,
    'absolute',
  );
  const text = '**Formatted**';
  const next = { ...original, kind: { Message: { text, card: null } } };
  const changed = f.rerender(next);
  assert.equal(nodeById(changed, `message-body-${original.id}`).type, 'MessageMarkdown');
  assert.equal(styleOf(nodeById(changed, `message-metadata-${original.id}`)).position, undefined);
  for (const text of [
    '**Bold** ![image](https://example.com/image)',
    '**Code**\n```c\nx < y\n```',
    '<video src="https://example.com/video"></video>',
  ]) {
    const next = plainMessage(text);
    const rendered = render(next);
    const body = nodeById(rendered.tree, `message-body-${next.id}`);
    if (messageMayContainMarkdown(text, true)) {
      assert.equal(body.type, 'MessageMarkdown');
      assert.equal(body.props.text, text);
    } else {
      assert.equal(body.type, 'Text');
      assert.equal(body.props.children, text);
    }
  }
});

test('plain links and email in messages, captions and AI App text reach the same parser with exact source', () => {
  for (const text of [
    'See https://example.invalid/help.',
    'www.example.invalid',
    'person@example.invalid',
  ]) {
    const message = plainMessage(text);
    const body = nodeById(render(message).tree, `message-body-${message.id}`);
    assert.equal(body.type, 'MessageMarkdown');
    assert.equal(body.props.text, text);
    const file = entry(text);
    for (const options of [{}, { albumPart: { captionOnly: true } }]) {
      const caption = elements(render(file, options).tree).find(
        (node) => node.type === 'MessageMarkdown',
      )!;
      assert.ok(caption);
      assert.equal(caption.props.text, text);
    }
    const app = entry(text, {
      author: { ...file.author, kind: 'Agent' },
      kind: {
        Message: { text, card: { item_id: 'deck', kind_id: 'app', app: { app_id: 'deckcraft' } } },
      } as TimelineEntry['kind'],
    });
    const appBody = nodeById(render(app).tree, `message-body-${app.id}`);
    assert.equal(appBody.type, 'MessageMarkdown');
    assert.equal(appBody.props.text, text);
  }
});
