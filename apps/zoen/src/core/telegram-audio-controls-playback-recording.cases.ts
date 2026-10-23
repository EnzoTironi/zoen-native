import assert from 'node:assert/strict';
import { test } from 'node:test';
import { byLabel, fixture, nodes, tick } from './telegram-audio-controls.fixtures.ts';

test('actual playback controls retain play/pause, realseek andrate ports without allocating another player', () => {
  const events: unknown[] = [];
  const status = {
    durationSeconds: 40,
    currentSeconds: 12,
    rate: 1,
    loaded: true,
    loadFailed: false,
    busy: false,
    buffering: false,
    playing: false,
    error: null,
  };
  const playback = {
    subscribe() {},
    getSnapshot: () => status,
    pause: () => events.push('pause'),
    playFrom: async () => {
      events.push('play');
    },
    seekSeconds: async (s: number) => {
      events.push(s);
    },
    setRate: (rate: number) => {
      events.push(rate);
      status.rate = rate;
    },
  };
  const f = fixture('playback');
  let tree = f.render('AudioClipControls', {
    playback,
    compact: true,
    appearance: 'mine',
    levels: [1, 0],
  });
  byLabel(tree, 'media.playAudio').props.onPress();
  status.playing = true;
  tree = f.render('AudioClipControls', {
    playback,
    compact: true,
    appearance: 'mine',
    levels: [1, 0],
  });
  byLabel(tree, 'media.pauseAudio').props.onPress();
  const progress = nodes(tree).find((node) => node.type === 'AudioProgress')!;
  progress.props.onSeek(0.75);
  for (let i = 0; i < 3; i++) {
    tree = f.render('AudioClipControls', { playback, compact: true, appearance: 'mine' });
    byLabel(tree, 'media.playbackRateLabel').props.onPress();
  }
  assert.deepEqual(events, ['play', 'pause', 30, 1.5, 2, 1]);
  assert.equal(byLabel(tree, 'media.pauseAudio').props.style({ pressed: false }).width, 44);
  status.busy = true;
  status.playing = false;
  tree = f.render('AudioClipControls', { playback, compact: true });
  byLabel(tree, 'media.pauseAudio').props.onPress();
  assert.equal(events.at(-1), 'pause');
});

test('waveform gesture preserves timeline yield, exactseek and rejects queued resize/retired callbacks', () => {
  const seeks: number[] = [];
  const f = fixture('waveform');
  const props = {
    levels: [1],
    progress: 0.2,
    duration: 40,
    ink: '#fff',
    compact: true,
    disabled: false,
    onSeek: (at: number) => seeks.push(at),
  };
  let tree = f.render('AudioProgress', props);
  nodes(tree)
    .find((node) => node.props.onLayout)!
    .props.onLayout({ nativeEvent: { layout: { width: 120 } } });
  tree = f.render('AudioProgress', props);
  let [pan, tap] = tree.props.gesture;
  assert.equal(pan.block, 'message-pan');
  assert.deepEqual(Array.from(pan.failOffsetY), [-12, 12]);
  pan.onBegin();
  pan.onDeactivate({ canceled: false, x: 90 });
  pan.onFinalize();
  tap.onBegin();
  tap.onDeactivate({ canceled: false, x: 30 });
  tap.onFinalize();
  assert.deepEqual(seeks, [0.75, 0.25]);
  pan.onBegin();
  tap.onBegin();
  nodes(tree)
    .find((node) => node.props.onLayout)!
    .props.onLayout({ nativeEvent: { layout: { width: 240 } } });
  tree = f.render('AudioProgress', props);
  pan.onDeactivate({ canceled: false, x: 90 });
  tap.onDeactivate({ canceled: false, x: 30 });
  assert.deepEqual(
    seeks,
    [0.75, 0.25],
    'old native callback cannot seek with the previous viewport',
  );
  [pan] = tree.props.gesture;
  pan.onBegin();
  f.unmount();
  pan.onDeactivate({ canceled: false, x: 120 });
  assert.deepEqual(seeks, [0.75, 0.25]);
});

test('actual transcript preserves accepted text, interruption and reduced-motion geometry', () => {
  const f = fixture('transcript');
  const props = { text: 'The accepted transcript', ink: '#fff', message: true };
  let tree = f.render('VoiceTranscript', props);
  const toggle = byLabel(tree, 'media.voiceTranscript');
  assert.equal(toggle.props['aria-expanded'], false);
  toggle.props.onPress();
  tree = f.render('VoiceTranscript', props);
  assert.equal(byLabel(tree, 'media.voiceTranscript').props['aria-expanded'], true);
  assert.ok(
    nodes(tree).some(
      (node) => node.type === 'Text' && node.props.selectable && node.props.children === props.text,
    ),
  );
  const entered = nodes(tree).find((node) => node.props.entering)!;
  assert.equal(entered.props.entering.ms, 200);
  f.theme.reducedMotion = true;
  tree = f.render('VoiceTranscript', props);
  assert.equal(tree.props.layout, undefined);
  assert.equal(
    nodes(tree).some((node) => node.props.entering),
    false,
  );
  byLabel(tree, 'media.voiceTranscript').props.onPress();
  tree = f.render('VoiceTranscript', props);
  assert.equal(
    nodes(tree).some((node) => node.props.selectable),
    false,
  );
});

test('microphone holds190ms, preserves pointer throughbusy, locks once and has an accessible tap alternative', async () => {
  const events: string[] = [];
  const f = fixture('microphone');
  const props = {
    disabled: false,
    active: false,
    locked: false,
    onStart: async (held: boolean) => {
      events.push(`start:${held}`);
      return true;
    },
    onReview: async () => {
      events.push('review');
    },
    onCancel: async () => {
      events.push('cancel');
    },
    onLock: () => events.push('lock'),
    onError: () => events.push('error'),
  };
  let tree = f.render('VoiceRecordButton', props);
  let [pan] = tree.props.gesture;
  assert.equal(pan.activateAfterLongPress, 190);
  pan.onActivate();
  await tick();
  tree = f.render('VoiceRecordButton', { ...props, disabled: true, active: true });
  [pan] = tree.props.gesture;
  assert.equal(pan.enabled, true);
  pan.onUpdate({ translationX: 0, translationY: -111 });
  pan.onDeactivate({
    canceled: false,
    translationX: 0,
    translationY: -111,
    velocityX: 0,
    velocityY: 0,
  });
  assert.deepEqual(events, ['start:true', 'lock']);
  tree = f.render('VoiceRecordButton', { ...props, active: false });
  byLabel(tree, 'media.recordVoiceMessage').props.onAccessibilityAction({
    nativeEvent: { actionName: 'activate' },
  });
  await tick();
  assert.deepEqual(events, ['start:true', 'lock', 'lock', 'start:false']);
  byLabel(tree, 'media.recordVoiceMessage').props.onAccessibilityTap();
  await tick();
  assert.deepEqual(
    events,
    ['start:true', 'lock', 'lock', 'start:false', 'lock', 'start:false'],
    'Fabric default activation uses onAccessibilityTap, while custom actions remain available',
  );
});
