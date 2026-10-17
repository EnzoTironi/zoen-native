import { createContext, use, useLayoutEffect, type ReactNode } from 'react';
import {
  useKeyboardHandler,
  useReanimatedKeyboardAnimation,
} from 'react-native-keyboard-controller';
import { useSharedValue, type SharedValue } from 'react-native-reanimated';

type Motion = Readonly<{ progress: SharedValue<number>; height: SharedValue<number> }>;
const KeyboardMotion = createContext<Motion | null>(null);

/** Real movement frames, shared by the composer and cards, including interactive dismissal. */
export function KeyboardMotionProvider({ children }: { children: ReactNode }) {
  const initial = useReanimatedKeyboardAnimation();
  const progress = useSharedValue(0);
  const height = useSharedValue(0);
  useLayoutEffect(() => {
    progress.set(initial.progress.value);
    height.set(-initial.height.value);
  }, [initial, progress, height]);
  useKeyboardHandler(
    {
      onMove: (event) => {
        'worklet';
        progress.set(event.progress);
        height.set(event.height);
      },
      onInteractive: (event) => {
        'worklet';
        progress.set(event.progress);
        height.set(event.height);
      },
      onEnd: (event) => {
        'worklet';
        progress.set(event.progress);
        height.set(event.height);
      },
    },
    [],
  );
  return <KeyboardMotion value={{ progress, height }}>{children}</KeyboardMotion>;
}

export function useKeyboardMotion(): Motion {
  const motion = use(KeyboardMotion);
  if (!motion) throw new Error('KeyboardMotionProvider is required.');
  return motion;
}
