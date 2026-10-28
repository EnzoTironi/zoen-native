import assert from 'node:assert/strict';
import { test } from 'node:test';
import { elements, plainMessage, render } from './message-caption.fixtures.ts';

test('a shared quote renders its signed excerpt without fetching private source history', () => {
  const message = plainMessage('Answer');
  message.reply_to = {
    id: 'private-message',
    author: message.author,
    text: 'Shared excerpt',
    origin: { space_id: 'private-chat', available: false },
  };
  const view = render(message);
  const preview = view.previews.find((input: any) => input.text === 'Shared excerpt') as
    | { literal: boolean; target?: unknown }
    | undefined;
  assert.ok(preview);
  assert.equal(preview.literal, true);
  assert.equal(preview.target, undefined);
  const quote = elements(view.tree).find(
    (node) => node.props.accessibilityRole === 'text' && node.props.onPress,
  );
  assert.ok(quote);
  assert.equal(quote.props.disabled, true);
  (quote.props.onPress as () => void)();
  assert.deepEqual(view.routes, []);
});
test('opening the original passes the exact source to the parent navigation boundary', () => {
  const message = plainMessage('Answer');
  message.reply_to = {
    id: 'private-message',
    author: message.author,
    text: 'Shared excerpt',
    origin: { space_id: 'private-chat', available: true },
  };
  const view = render(message);
  const quote = elements(view.tree).find(
    (node) => node.props.accessibilityLabel === 'chat.goToQuotedMessage',
  );
  assert.ok(quote);
  (quote.props.onPress as () => void)();
  assert.deepEqual(view.quotes, [{ id: 'private-message', sourceSpace: 'private-chat' }]);
});
