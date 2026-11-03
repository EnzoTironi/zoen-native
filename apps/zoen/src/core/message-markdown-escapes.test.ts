import assert from 'node:assert/strict';
import { test } from 'node:test';
import { elements, entry, nodeById, plainMessage, render } from './message-caption.fixtures.ts';
import { messageMayContainMarkdown } from './message-markdown.ts';

test('canonical Markdown escapes in plain messages and captions reach the parser unchanged', () => {
  for (const source of [
    String.raw`QA753\: nota em Mensagens Salvas\.`,
    String.raw`café 🦊\: 2 \+ 2 \= 4\.`,
    String.raw`C\:\\Temp\\note\.txt`,
    String.raw`literal \\ and \q`,
    String.raw`\*literal\* \[a\]\(b\)`,
    String.raw`**forte** e \*literal\*`,
    '`literal\\: code`',
    String.raw`[link](https://example.org/a\(b\))`,
  ]) {
    assert.equal(messageMayContainMarkdown(source, true), true, source);
    const message = plainMessage(source);
    const body = nodeById(render(message).tree, `message-body-${message.id}`);
    assert.equal(body.type, 'MessageMarkdown', source);
    assert.equal(body.props.text, source, 'canonical bytes must reach the maintained parser');
    const file = entry(source);
    for (const options of [{}, { albumPart: { captionOnly: true } }]) {
      const caption = elements(render(file, options).tree).find(
        (node) => node.type === 'MessageMarkdown',
      );
      assert.ok(caption, source);
      assert.equal(caption.props.text, source);
    }
  }
});

test('escape candidates retain existing media admission and ordinary text fast path', () => {
  const source = String.raw`\<video src="https://example.org/clip"\>`;
  assert.equal(messageMayContainMarkdown(source), false);
  assert.equal(messageMayContainMarkdown(source, true), true);
  for (const text of ['QA753: nota em Mensagens Salvas.', 'café 🦊', '2 + 2 = 4.'])
    assert.equal(messageMayContainMarkdown(text, true), false);
});
