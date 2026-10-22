import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  byLabel,
  fixture,
  interactiveNodes,
  nodes,
  tick,
} from './telegram-audio-controls.fixtures.ts';

test('composer uses the same field for reply/edit/search/caption and retains realAI/arrows/send/voice callbacks', async () => {
  const f = fixture('composer');
  const calls: string[] = [];
  const props: any = {
    spaceId: 'one',
    value: '',
    onChangeText: () => {},
    onBlur: () => {},
    onSend: () => calls.push('send'),
    sending: false,
    context: { entry: { id: 'reply' }, onCancel: () => calls.push('cancel') },
  };
  const controls = (next: any, files?: any) => {
    const owner = nodes(f.render('MessageComposer', next)).find(
      (node) => node.type === 'MediaComposer',
    )!;
    return owner.props.renderControls({
      attachment: { type: 'Attachment', props: {} },
      voice: f.microphone({
        disabled: Boolean(next.editing || next.search),
        active: false,
        locked: false,
        onStart: async (held: boolean) => {
          calls.push(`voice:${held}`);
          return true;
        },
        onReview: async () => calls.push('voice-review'),
        onCancel: async () => calls.push('voice-cancel'),
        onLock: () => calls.push('voice-lock'),
        onError: () => calls.push('voice-error'),
      }),
      files,
    });
  };
  let tree = controls(props);
  const field = nodes(tree).find((node) => node.type === 'ComposerTextField')!;
  assert.equal(nodes(tree).filter((node) => node.type === 'ComposerTextField').length, 1);
  assert.equal(field.props.context, 'one::normal');
  assert.ok(nodes(tree).some((node) => node.type === 'MessageContext'));
  assert.equal(
    interactiveNodes(tree).some((node) => node.props.accessibilityLabel === 'chat.sendMessage'),
    false,
    'retained hidden Send must not masquerade as an available action',
  );
  for (const label of ['chat.sendMessage', 'chat.olderSearchResult', 'chat.newerSearchResult']) {
    const hidden = nodes(tree).find((node) => node.props.accessibilityLabel === label)!;
    assert.equal(hidden.props.accessible, false);
    assert.equal(hidden.props.accessibilityElementsHidden, true);
    assert.equal(hidden.props.importantForAccessibility, 'no-hide-descendants');
  }
  const oldVoiceTap = byLabel(tree, 'media.recordVoiceMessage').props.onAccessibilityTap;
  oldVoiceTap();
  await tick();
  tree = controls({ ...props, value: 'text', editing: true });
  assert.equal(
    nodes(tree).find((node) => node.type === 'ComposerTextField')!.props.context,
    'one::edit',
  );
  assert.ok(nodes(tree).some((node) => node.props.testID === 'record-voice'));
  assert.equal(
    interactiveNodes(tree).some((node) => node.props.testID === 'record-voice'),
    false,
    'the actual recording gesture stays mounted while editing retires its hit target',
  );
  oldVoiceTap();
  byLabel(tree, 'chat.saveEdit').props.onPress();
  const search = {
    status: 'none',
    state: {
      query: 'question',
      completed: true,
      loading: false,
      error: null,
      hits: [],
      cursor: null,
      index: -1,
    },
    onAsk: () => calls.push('AI'),
    onMove: (v: string) => calls.push(v),
    onClose: () => calls.push('close'),
    onSearch: () => calls.push('search'),
  };
  tree = controls({ ...props, search });
  const oldAsk = byLabel(tree, 'chat.askAboutChat').props.onPress;
  oldAsk();
  search.state.hits = [{}, {}, {}] as never;
  search.state.index = 1;
  tree = controls({ ...props, search });
  assert.ok(byLabel(tree, 'chat.olderSearchResult'));
  assert.ok(byLabel(tree, 'chat.newerSearchResult'));
  byLabel(tree, 'chat.olderSearchResult').props.onPress();
  byLabel(tree, 'chat.newerSearchResult').props.onPress();
  assert.equal(
    interactiveNodes(tree).some((node) => node.props.accessibilityLabel === 'chat.askAboutChat'),
    false,
    'the retained AI action is retired when real results replace it',
  );
  oldAsk();
  tree = controls(props, {
    value: 'caption',
    position: 1,
    total: 2,
    editable: true,
    remaining: 2,
    scope: {},
    onChange: () => {},
    onSend: () => calls.push('files'),
  });
  const caption = nodes(tree).find((node) => node.type === 'ComposerTextField')!;
  assert.equal(caption.type, field.type);
  assert.equal(caption.props.value, 'caption');
  const oldAttachmentSend = byLabel(tree, 'media.attachmentSendCount').props.onPress;
  oldAttachmentSend();
  controls(props);
  oldAttachmentSend();
  assert.deepEqual(calls, ['voice-lock', 'voice:false', 'send', 'AI', 'older', 'newer', 'files']);
});

test('reply/edit accessory uses the pinned45pt frame, source typography and real Cancel for unavailable entries too', () => {
  const f = fixture('accessory');
  let cancelled = 0;
  for (const editing of [false, true]) {
    const tree = f.render('MessageContext', {
      editing,
      entry: { id: 'quote', author: { name: 'Ana' }, text: 'Original accepted message' },
      onCancel: () => cancelled++,
    });
    assert.equal(tree.props.style.height, 45);
    assert.equal(
      tree.props.style.paddingLeft + 8,
      55,
      'the composer already owns8pt of the viewport',
    );
    const title = nodes(tree).find((node) => node.type === 'Text')!;
    assert.equal(title.props.style.fontSize, 15);
    assert.equal(title.props.style.fontWeight, '500');
    const icon = nodes(tree).find((node) => node.props.entering?.kind === 'zoom-in')!;
    assert.equal(icon.props.entering.ms, 200);
    assert.equal(icon.props.entering.initial.transform[0].scale, 0.001);
    byLabel(tree, 'chat.cancel').props.onPress();
  }
  f.theme.reducedMotion = true;
  const unavailable = f.render('MessageContext', { onCancel: () => cancelled++ });
  assert.ok(nodes(unavailable).some((node) => node.props.children === 'chat.messageUnavailable'));
  assert.equal(
    nodes(unavailable).some((node) => node.props.entering),
    false,
  );
  byLabel(unavailable, 'chat.cancel').props.onPress();
  assert.equal(cancelled, 3);
});
