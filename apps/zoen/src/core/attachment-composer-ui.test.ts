import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

type Element = { type: unknown; props: Record<string, unknown> };
type Handler = (...args: unknown[]) => unknown;
const source = new URL('../components/message-composer.tsx', import.meta.url);
const output = await build({
  stdin: { contents: readFileSync(source, 'utf8'), loader: 'tsx' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'composer-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
function fixture() {
  const jsx = (type: unknown, props: Record<string, unknown>): Element => ({ type, props });
  const scope = {
    mode: 'account',
    identityId: 'me',
    deviceId: 'device',
    spaceId: 'space',
    replyTo: null,
    thread: false,
  };
  const calls: string[] = [];
  const file = {
    scope,
    targetFrozen: false,
    value: '',
    position: 1,
    total: 2,
    editable: true,
    sending: false,
    remaining: 2,
    retry: false,
    onChange: (text: unknown) => calls.push(`caption:${text}`),
    onSend: () => calls.push('send-files'),
  };
  const ports: Record<string, unknown> = {
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react-native': { Text: 'Text', View: 'View', TextInput: 'TextInput' },
    'react-native-safe-area-context': { useSafeAreaInsets: () => ({ top: 59, bottom: 34 }) },
    'react-native-reanimated': {
      default: { View: 'AnimatedView' },
      useAnimatedStyle: (read: () => unknown) => read(),
    },
    'expo-glass-effect': { GlassView: 'GlassView', isLiquidGlassAvailable: () => false },
    './keyboard-motion': { useKeyboardMotion: () => ({ progress: { value: 0 } }) },
    '../theme': { useTheme: () => ({ scheme: 'dark', colors: {} }) },
    '../i18n/index.ts': { t: (key: string) => key },
    '../core/conversation-search': { conversationQueryFits: () => true },
    '../media/MediaComposer': { MediaComposer: 'MediaComposer' },
    '../media/attachment-reply-context': { AttachmentReplyContext: 'AttachmentReplyContext' },
    './message-context': { MessageContext: 'MessageContext' },
    './composer-action': { ComposerAction: 'ComposerAction' },
    './primitives': { NativeButton: 'NativeButton' },
  };
  const module = {
    exports: {} as { MessageComposer: (props: Record<string, unknown>) => Element },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    exports: module.exports,
    module,
    process: { env: { EXPO_OS: 'ios' } },
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  const props = {
    spaceId: 'space',
    value: 'My unsent normal text',
    sending: false,
    onChangeText: (text: unknown) => calls.push(`normal:${text}`),
    onBlur: () => {},
    onSend: () => calls.push('send-normal'),
  };
  function render(files: unknown = undefined, patch: Record<string, unknown> = {}) {
    const root = module.exports.MessageComposer({ ...props, ...patch });
    const media = find(root, (node) => node.type === 'MediaComposer');
    return (media.props.renderControls as Handler)({
      attachment: null,
      voice: 'Microphone',
      files,
    }) as Element;
  }
  return {
    file,
    calls,
    render,
    renderRoot: (patch: Record<string, unknown>) =>
      module.exports.MessageComposer({ ...props, ...patch }),
  };
}
function find(root: unknown, predicate: (element: Element) => boolean): Element {
  const result = scan(root).find(predicate);
  assert.ok(result, 'actual composer element missing');
  return result;
}
function scan(root: unknown): Element[] {
  if (Array.isArray(root)) return root.flatMap(scan);
  if (!root || typeof root !== 'object' || !('props' in root)) return [];
  const element = root as Element;
  return [element, ...scan(element.props.children)];
}
const byId = (id: string) => (node: Element) => node.props.testID === id;

test('native attachment mode uses the real message field and send button without posting the normal text draft', () => {
  const f = fixture();
  const root = f.render(f.file);
  const input = find(root, byId('message-input'));
  assert.equal(input.props.value, '');
  assert.equal(input.props.editable, true);
  (input.props.onChangeText as Handler)('First photo caption');
  const send = find(root, byId('send-attachments'));
  assert.equal(send.props.disabled, false, 'an attachment is sendable without a caption');
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['caption:First photo caption', 'send-files']);
  assert.equal(find(f.render(), byId('message-input')).props.value, 'My unsent normal text');
});

test('ambiguous attachment retry keeps the caption readonly and sends only through the batch owner', () => {
  const f = fixture();
  const root = f.render({
    ...f.file,
    editable: false,
    targetFrozen: true,
    value: 'Exact attempted caption',
    retry: true,
    remaining: 1,
  });
  assert.equal(find(root, byId('message-input')).props.editable, false);
  const send = find(root, byId('send-attachments'));
  assert.equal(send.props.label, 'media.attachmentsRetry');
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['send-files']);
});

test('attachment reply UI displays the frozen batch target rather than a later normal-draft reply', () => {
  const f = fixture();
  const scope = { ...f.file.scope, replyTo: 'captured-original' };
  const context = { entry: { id: 'later-reply' }, onCancel: () => {} };
  const root = f.render({ ...f.file, scope, targetFrozen: true }, { context });
  const quote = find(root, (node) => node.type === 'AttachmentReplyContext');
  assert.equal((quote.props.scope as typeof scope).replyTo, 'captured-original');
  assert.equal(quote.props.onCancel, undefined);
  assert.equal(
    scan(root).some((node) => node.type === 'MessageContext'),
    false,
  );
  assert.equal(
    find(f.render(undefined, { context }), (node) => node.type === 'MessageContext').props.entry,
    context.entry,
  );
});

test('search takes over the message field while attachment drafts stay outside its action path', () => {
  const f = fixture();
  const search = {
    state: { active: true, completed: true, loading: false, hits: [{}], index: 0, cursor: null },
    status: '1/1',
    onClose: () => {},
    onMove: () => {},
    onAsk: () => {},
    onSearch: () => {},
  };
  const root = f.render(f.file, { search, value: 'Search text' });
  assert.equal(find(root, byId('message-input')).props.value, 'Search text');
  assert.equal(scan(root).some(byId('send-attachments')), false);
  assert.ok(find(root, byId('older-search-result')));
});

test('a sent file caption can be cleared without allowing empty ordinary text edits', () => {
  const f = fixture();
  const emptyCaption = f.render(undefined, { value: '', editing: true, allowEmptyEdit: true });
  assert.equal(find(emptyCaption, byId('send-message')).props.disabled, false);
  assert.equal(
    (find(emptyCaption, byId('message-input')).props.style as { height: number }).height,
    52,
  );
  const emptyText = f.render(undefined, { value: '', editing: true });
  assert.equal(find(emptyText, byId('send-message')).props.disabled, true);
});

test('an unread native draft cannot be overwritten by an initially empty field; retry stays visible', () => {
  const f = fixture();
  const draft = { ready: false, error: true, onRetry: () => f.calls.push('retry-draft') };
  const root = f.render(undefined, { value: '', draft });
  assert.equal(find(root, byId('message-input')).props.editable, false);
  const status = f.renderRoot({ draft });
  (find(status, byId('retry-message-draft')).props.onPress as Handler)();
  assert.deepEqual(f.calls, ['retry-draft']);
  const loaded = f.render(undefined, { draft: { ...draft, ready: true } });
  assert.equal(find(loaded, byId('message-input')).props.editable, true);
  assert.equal(find(loaded, byId('message-input')).props.value, 'My unsent normal text');
});
