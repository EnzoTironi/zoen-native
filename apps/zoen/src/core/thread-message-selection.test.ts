import assert from 'node:assert/strict';
import type { TimelineEntry } from './types';
import { test } from 'node:test';
import { fixture, find, type Element } from './thread-message-selection.fixtures.ts';
test('thread binds selection to its root and expands every original entry into toggle-only rows', () => {
  const f = fixture();
  assert.equal(f.hookArgs[0], 'space');
  assert.equal(f.hookArgs[1], f.entries);
  assert.equal(f.hookArgs[2], false);
  assert.equal(f.hookArgs[3], 'root-message');
  assert.equal(f.actionArgs!.disabled, true);
  assert.equal(f.actionArgs!.onSelect, f.selection.toggle);
  const rows = f.list.props.data as Array<{ entry: TimelineEntry; album?: unknown }>;
  assert.equal(rows.length, 2);
  assert.equal(rows[0]!.album, undefined);
  for (const [index, row] of rows.entries()) {
    const rendered = (f.list.props.renderItem as (value: unknown) => unknown)({ item: row, index });
    const message = find(rendered, 'Message')!;
    assert.equal(message.props.disabled, true);
    const control = message.props.selection as {
      selected: boolean;
      disabled: boolean;
      onToggle(entry: TimelineEntry): void;
    };
    assert.equal(control.selected, index === 0);
    assert.equal(control.disabled, index !== 0);
    control.onToggle(row.entry);
  }
  assert.deepEqual(f.calls, ['toggle:original', 'toggle:reply']);
});

test('thread selection replaces composition and exposes native count and Cancel without a back button', () => {
  const f = fixture();
  assert.equal(find(f.viewport.props.composer, 'Composer'), undefined);
  const toolbar = find(f.viewport.props.composer, 'SelectionToolbar')!;
  assert.equal(toolbar.props.canDelete, true);
  assert.equal(toolbar.props.pending, false);
  (toolbar.props.onDelete as () => void)();
  (toolbar.props.onCopy as () => void)();
  (toolbar.props.onForward as () => void)();
  const options = find(f.tree, 'Screen')!.props.options as {
    headerBackVisible: boolean;
    headerTitle(): Element;
    unstable_headerLeftItems(): unknown[];
    unstable_headerRightItems(): Array<{ onPress(): void }>;
  };
  assert.equal(options.headerBackVisible, false);
  assert.equal(options.headerTitle().props.count, 1);
  assert.equal(options.unstable_headerLeftItems().length, 0);
  options.unstable_headerRightItems()[0]!.onPress();
  assert.deepEqual(f.calls, ['delete', 'copy', 'forward', 'cancel']);
});

test('ordinary thread keeps existing album projection, composer and message actions', () => {
  const f = fixture(false);
  assert.ok(find(f.viewport.props.composer, 'Composer'));
  assert.equal(find(f.viewport.props.composer, 'SelectionToolbar'), undefined);
  assert.equal(f.actionArgs!.disabled, false);
  const rows = f.list.props.data as Array<{ album?: unknown }>;
  assert.equal(rows.length, 1);
  assert.ok(rows[0]!.album);
  const options = find(f.tree, 'Screen')!.props.options as {
    title: string;
    headerBackVisible: boolean;
  };
  assert.equal(options.title, 'chat.replies');
  assert.equal(options.headerBackVisible, true);
});

test('thread quote navigation checks current source membership and session at the tap boundary', () => {
  const f = fixture(false);
  const rows = f.list.props.data as unknown[];
  const rendered = (f.list.props.renderItem as (value: unknown) => unknown)({
    item: rows[0],
    index: 0,
  });
  const message = find(rendered, 'Message')!;
  const open = message.props.onQuote as (messageId: string, sourceSpace: string) => void;
  open('source-message', 'source');
  assert.deepEqual(f.routes, [
    { pathname: '/chat/[id]', params: { id: 'source', jump: 'source-message' } },
  ]);
  f.state.snapshot.conversations[0]!.members = [];
  open('source-message', 'source');
  assert.equal(f.routes.length, 1, 'A rendered quote must not retain revoked source access');
  f.state.snapshot.conversations[0]!.members = [{ id: 'alice' }];
  f.state.snapshot.account.unlocked = false;
  open('source-message', 'source');
  assert.equal(f.routes.length, 1, 'Locking the account must prevent origin navigation');
});

test('message links reveal an album child and suppress automatic bottom following', () => {
  const separate = fixture(false, 'reply', true);
  assert.equal(separate.list.props.initialScrollIndex, 1);
  const f = fixture(false, 'reply');
  assert.equal(f.list.props.initialScrollIndex, 0);
  assert.equal(
    (f.list.props.maintainVisibleContentPosition as any).autoscrollToBottomThreshold,
    undefined,
  );
  const rows = f.list.props.data as unknown[];
  const rendered = (f.list.props.renderItem as (value: unknown) => unknown)({
    item: rows[0],
    index: 0,
  });
  assert.equal(find(rendered, 'Message')!.props.highlighted, true);
  assert.ok(find(f.viewport.props.composer, 'Composer'));
  const missing = fixture(false, 'missing');
  assert.equal(missing.list.props.initialScrollIndex, undefined);
  assert.equal(
    find(missing.viewport.props.composer, 'Text')!.props.children,
    'chat.messageUnavailable',
  );
});

test('thread send failure is hidden after reopening the same account', async () => {
  const f = fixture(false);
  const composer = find(f.viewport.props.composer, 'Composer')!;
  (composer.props.onSend as () => void)();
  await new Promise<void>((resolve) => setImmediate(resolve));
  const failed = find(f.render(), 'Viewport')!;
  assert.match(JSON.stringify(failed.props.composer), /chat.theReplyCouldNotBeSent/);
  f.reopen();
  const reopened = find(f.render(), 'Viewport')!;
  assert.doesNotMatch(JSON.stringify(reopened.props.composer), /chat.theReplyCouldNotBeSent/);
});

test('finishing an old send keeps the reopened session’s send pending', async () => {
  const pending: (() => void)[] = [];
  const f = fixture(
    false,
    undefined,
    false,
    () => new Promise<void>((resolve) => pending.push(resolve)),
  );
  const composer = () => find(find(f.render(), 'Viewport')!.props.composer, 'Composer')!.props;
  (composer().onSend as () => void)();
  assert.equal(composer().sending, true);
  f.reopen();
  assert.equal(composer().sending, false);
  (composer().onSend as () => void)();
  assert.equal(pending.length, 2);
  pending[0]!();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(composer().sending, true);
  pending[1]!();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(composer().sending, false);
});

test('an old send rejection cannot replace the current session’s alert', async () => {
  const pending: (() => void)[] = [];
  const f = fixture(
    false,
    undefined,
    false,
    () =>
      new Promise<void>((_resolve, reject) => pending.push(() => reject(new Error('send failed')))),
  );
  const composer = () => find(f.render(), 'Viewport')!.props.composer;
  const send = () => (find(composer(), 'Composer')!.props.onSend as () => void)();
  send();
  f.reopen();
  send();
  pending[1]!();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.match(JSON.stringify(composer()), /chat.theReplyCouldNotBeSent/);
  pending[0]!();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.match(JSON.stringify(composer()), /chat.theReplyCouldNotBeSent/);
});

test('returning to a thread does not revive an alert from its earlier route scope', async () => {
  const f = fixture(false);
  const composer = () => find(f.render(), 'Viewport')!.props.composer;
  (find(composer(), 'Composer')!.props.onSend as () => void)();
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.match(JSON.stringify(composer()), /chat.theReplyCouldNotBeSent/);
  f.setRoot('another-root');
  assert.doesNotMatch(JSON.stringify(composer()), /chat.theReplyCouldNotBeSent/);
  f.setRoot('root-message');
  assert.doesNotMatch(JSON.stringify(composer()), /chat.theReplyCouldNotBeSent/);
});
