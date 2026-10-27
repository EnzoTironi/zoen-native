import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, nodes } from './telegram-audio-controls.fixtures.ts';

test('a recycled waveform cannot seek its replacement or an ABA-restored source', () => {
  const seeks: string[] = [];
  const sourceA = {};
  const sourceB = {};
  const f = fixture('waveform');
  const props = {
    sourceIdentity: sourceA,
    levels: [1],
    progress: 0.2,
    duration: 40,
    ink: '#fff',
    compact: true,
    disabled: false,
    onSeek: (value: number) => seeks.push(`A:${value}`),
  };
  let tree = f.render('AudioProgress', props);
  nodes(tree)
    .find((node) => node.props.onLayout)!
    .props.onLayout({ nativeEvent: { layout: { width: 120 } } });
  tree = f.render('AudioProgress', props);
  const [oldPan, oldTap] = tree.props.gesture;
  oldPan.onBegin();
  oldTap.onBegin();
  tree = f.render('AudioProgress', {
    ...props,
    sourceIdentity: sourceB,
    onSeek: (value: number) => seeks.push(`B:${value}`),
  });
  oldPan.onDeactivate({ canceled: false, x: 90 });
  oldTap.onDeactivate({ canceled: false, x: 30 });
  assert.deepEqual(seeks, [], 'late pointers may not seek the same-width replacement');
  tree = f.render('AudioProgress', props);
  oldPan.onDeactivate({ canceled: false, x: 90 });
  oldTap.onDeactivate({ canceled: false, x: 30 });
  assert.deepEqual(seeks, [], 'A → B → A does not resurrect an old gesture');
  const [pan] = tree.props.gesture;
  pan.onBegin();
  pan.onDeactivate({ canceled: false, x: 60 });
  pan.onFinalize();
  assert.deepEqual(seeks, ['A:0.5']);
});

test('scrubbing previews the timestamp and preserves a gesture through ordinary playback updates', () => {
  const seeks: number[] = [];
  const source = {};
  const f = fixture('waveform');
  const props = {
    sourceIdentity: source,
    levels: [1, 0.5],
    progress: 0.2,
    duration: 40,
    ink: '#fff',
    compact: true,
    disabled: false,
    onSeek: (value: number) => seeks.push(-value),
  };
  let tree = f.render('AudioProgress', props);
  nodes(tree)
    .find((node) => node.props.onLayout)!
    .props.onLayout({ nativeEvent: { layout: { width: 120 } } });
  tree = f.render('AudioProgress', props);
  const [pan] = tree.props.gesture;
  pan.onBegin();
  pan.onUpdate({ x: 90 });
  tree = f.render('AudioProgress', {
    ...props,
    progress: 0.3,
    onSeek: (at: number) => seeks.push(at),
  });
  assert.equal(nodes(tree).find((node) => node.type === 'Text')?.props.children, '0:30');
  assert.deepEqual(seeks, [], 'drag preview must not trigger a media seek');
  pan.onDeactivate({ canceled: false, x: 90 });
  pan.onFinalize();
  tree = f.render('AudioProgress', { ...props, progress: 0.75 });
  assert.deepEqual(seeks, [0.75]);
  assert.equal(nodes(tree).find((node) => node.type === 'Text')?.props.children, '0:30');
});

test('cancelled, disabled and late gestures cannot seek or interrupt a replacement', () => {
  const seeks: number[] = [];
  let consumed = 0;
  const f = fixture('waveform', {
    '../components/message-interaction': {
      useMessagePan: () => 'message-pan',
      useMessagePressConsumption: () => () => consumed++,
    },
  });
  const props = {
    sourceIdentity: {},
    levels: [1],
    progress: 0.2,
    duration: 40,
    ink: '#fff',
    compact: true,
    disabled: false,
    onSeek: (value: number) => seeks.push(value),
  };
  let tree = f.render('AudioProgress', props);
  nodes(tree)
    .find((node) => node.props.onLayout)!
    .props.onLayout({ nativeEvent: { layout: { width: 120 } } });
  tree = f.render('AudioProgress', props);
  const [oldPan, oldTap] = tree.props.gesture;
  oldPan.onBegin();
  oldPan.onUpdate({ x: 90 });
  oldPan.onDeactivate({ canceled: true, x: 90 });
  oldPan.onFinalize();
  tree = f.render('AudioProgress', { ...props, disabled: true });
  oldPan.onBegin();
  oldTap.onBegin();
  oldPan.onDeactivate({ canceled: false, x: 90 });
  oldTap.onDeactivate({ canceled: false, x: 90 });
  assert.deepEqual(seeks, []);
  tree = f.render('AudioProgress', props);
  const [pan, tap] = tree.props.gesture;
  pan.onBegin();
  pan.onUpdate({ x: 60 });
  tap.onBegin();
  oldPan.onBegin();
  oldTap.onBegin();
  oldPan.onActivate();
  oldTap.onActivate();
  assert.equal(consumed, 0, 'retired activation must not consume the replacement press');
  pan.onActivate();
  tap.onActivate();
  assert.equal(consumed, 2);
  oldPan.onFinalize();
  oldTap.onFinalize();
  tree = f.render('AudioProgress', props);
  assert.equal(nodes(tree).find((node) => node.type === 'Text')?.props.children, '0:20');
  pan.onDeactivate({ canceled: false, x: 60 });
  tap.onDeactivate({ canceled: false, x: 30 });
  assert.deepEqual(seeks, [0.5, 0.25]);
});
