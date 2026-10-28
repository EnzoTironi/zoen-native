import {
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
  type RefObject,
  type ComponentProps,
} from 'react';
import { useWindowDimensions, type ViewProps } from 'react-native';
import Animated, {
  defineAnimation,
  runOnUI,
  runOnJS,
  useAnimatedProps,
  useSharedValue,
  withTiming,
  type AnimationObject,
  type LayoutAnimationFunction,
  type SharedValue,
} from 'react-native-reanimated';
import { EnrichedMarkdownText } from 'react-native-enriched-markdown';
import { useTheme } from '../theme';
import { quoteMotionCurve } from '../core/quote-motion';
import { MessagePressable } from './message-interaction';
import { QuoteLayout } from './quote-layout-context';

type Target = { sequence: number; generation: number; active: boolean; ready: boolean };
type Motion = { progress: SharedValue<number>; target: SharedValue<Target> };
const MotionPressable = Animated.createAnimatedComponent(MessagePressable);
const noMotion = () => {};
const RichText = Animated.createAnimatedComponent(EnrichedMarkdownText);

function QuoteAnimatedView(props: ViewProps) {
  const motion = use(QuoteLayout);
  return <Animated.View {...props} layout={motion?.layout} />;
}
function QuoteAnimatedBubble(props: ComponentProps<typeof MessagePressable>) {
  const motion = use(QuoteLayout);
  return <MotionPressable {...props} layout={motion?.layout} />;
}

/** Each frame reads the one UI progress; late FlashList position commits join it. */
function follow(from: number, to: number, motion: Motion) {
  'worklet';
  return defineAnimation(from, () => ({
    current: from,
    toValue: to,
    from,
    sequence: motion.target.get().sequence,
    onStart(animation: AnimationObject & { from: number }, value: number) {
      animation.current = value;
      animation.from = value;
    },
    onFrame(animation: AnimationObject & { from: number; sequence: number }) {
      const target = motion.target.get();
      if (animation.sequence !== target.sequence) {
        animation.sequence = target.sequence;
        animation.from = animation.current as number;
      }
      const progress = target.active ? motion.progress.get() : 1;
      animation.current = animation.from + (to - animation.from) * progress;
      return !target.active || progress >= 1;
    },
  })) as unknown as number;
}
function MotionMarkdown({
  element,
  generation,
  motion,
}: {
  element: ReactElement;
  generation: number;
  motion: Motion;
}) {
  const animatedProps = useAnimatedProps(() => ({
    quoteMotion: {
      progress: motion.target.get().generation === generation ? motion.progress.get() : 1,
      sequence: motion.target.get().sequence,
      generation,
    },
  }));
  return (
    <RichText
      {...(element.props as React.ComponentProps<typeof EnrichedMarkdownText>)}
      animatedProps={animatedProps}
    />
  );
}

/** Existing Reanimated layout engine owns the clock; no entrance or per-row clock. */
export function QuoteLayoutTransition({
  list,
  onBegin,
  children,
}: {
  onBegin?: () => void;
  list: RefObject<{ prepareForLayoutAnimationRender(): void } | null>;
  children: (
    active: boolean,
    commit: () => void,
    cancel: () => void,
    reading: boolean,
  ) => ReactNode;
}) {
  const { reducedMotion } = useTheme();
  const { width, height, fontScale } = useWindowDimensions();
  const progress = useSharedValue(1);
  const target = useSharedValue<Target>({
    sequence: 0,
    generation: 0,
    active: false,
    ready: false,
  });
  const [phase, setPhase] = useState<'idle' | 'transition' | 'reading'>('idle');
  const mounted = useRef(true);
  const sequence = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      target.set({ ...target.get(), active: false });
      progress.set(1);
    };
  }, [target, progress]);
  const complete = useCallback((owner: number) => {
    if (mounted.current && sequence.current === owner) setPhase('reading');
  }, []);
  const begin = useCallback(
    (generation: number) => {
      if (process.env.EXPO_OS !== 'ios') return;
      const current = ++sequence.current;
      onBegin?.();
      list.current?.prepareForLayoutAnimationRender();
      setPhase(reducedMotion ? 'reading' : 'transition');
      runOnUI(() => {
        'worklet';
        target.set({ sequence: current, generation, active: !reducedMotion, ready: false });
        progress.set(reducedMotion ? 1 : 0);
      })();
    },
    [list, onBegin, reducedMotion, target, progress],
  );
  // FlashList invokes this after measuring and committing its canonical row
  // positions. The UI queue starts motion after those actual target commits.
  const commit = useCallback(
    () =>
      runOnUI(() => {
        'worklet';
        const owner = target.get();
        if (!owner.active || owner.ready) return;
        target.set({ ...owner, ready: true });
        const sequence = owner.sequence;
        progress.set(
          withTiming(1, { duration: 400, easing: quoteMotionCurve }, (finished) => {
            if (finished && target.get().sequence === sequence) {
              target.set({ ...target.get(), active: false });
              runOnJS(complete)(sequence);
            }
          }),
        );
      })(),
    [target, progress, complete],
  );
  const finishReduced = useCallback(
    () => setPhase((previous) => (previous === 'idle' ? 'idle' : 'reading')),
    [],
  );
  useEffect(() => {
    runOnUI(() => {
      'worklet';
      if (!target.get().active) return;
      target.set({ ...target.get(), active: false });
      progress.set(1);
      runOnJS(finishReduced)();
    })();
  }, [reducedMotion, width, height, fontScale, target, progress, finishReduced]);
  const finish = useCallback(
    (generation: number) =>
      runOnUI(() => {
        'worklet';
        const owner = target.get();
        if (owner.generation !== generation || !owner.active) return;
        target.set({ ...owner, active: false });
        progress.set(1);
        runOnJS(complete)(owner.sequence);
      })(),
    [target, progress, complete],
  );
  const cancel = useCallback(() => {
    sequence.current++;
    target.set({ ...target.get(), active: false });
    progress.set(1);
    setPhase('idle');
  }, [target, progress]);
  const layout: LayoutAnimationFunction = useCallback(
    (values) => {
      'worklet';
      const motion = { progress, target };
      return {
        initialValues: {
          originX: values.currentOriginX,
          originY: values.currentOriginY,
          width: values.currentWidth,
          height: values.currentHeight,
        },
        animations: {
          originX: follow(values.currentOriginX, values.targetOriginX, motion),
          originY: follow(values.currentOriginY, values.targetOriginY, motion),
          width: follow(values.currentWidth, values.targetWidth, motion),
          height: follow(values.currentHeight, values.targetHeight, motion),
        },
      };
    },
    [progress, target],
  );
  const value = useMemo(
    () => ({
      layout,
      begin,
      finish,
      markdown: (element: ReactElement, generation: number) => (
        <MotionMarkdown element={element} generation={generation} motion={{ progress, target }} />
      ),
      View: QuoteAnimatedView,
      Pressable: QuoteAnimatedBubble,
    }),
    [begin, finish, progress, target, layout],
  );
  return (
    <QuoteLayout value={process.env.EXPO_OS === 'ios' ? value : null}>
      <QuoteTimeline
        active={process.env.EXPO_OS === 'ios' && phase === 'transition'}
        commit={process.env.EXPO_OS === 'ios' ? commit : noMotion}
        cancel={process.env.EXPO_OS === 'ios' ? cancel : noMotion}
        reading={process.env.EXPO_OS === 'ios' && phase !== 'idle'}
      >
        {children}
      </QuoteTimeline>
    </QuoteLayout>
  );
}

// Render beneath the provider while keeping event callbacks as component props.
function QuoteTimeline({
  children,
  active,
  commit,
  cancel,
  reading,
}: {
  children: (
    active: boolean,
    commit: () => void,
    cancel: () => void,
    reading: boolean,
  ) => ReactNode;
  active: boolean;
  commit: () => void;
  cancel: () => void;
  reading: boolean;
}) {
  return children(active, commit, cancel, reading);
}
