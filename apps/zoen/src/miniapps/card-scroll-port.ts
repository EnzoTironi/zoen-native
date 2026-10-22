import {
  useAnimatedScrollHandler,
  useSharedValue,
  type SharedValue,
} from 'react-native-reanimated';

export function useCardScrollPort(horizontal: boolean) {
  const offset = useSharedValue(0);
  const extent = useSharedValue(0);
  const minimum = useSharedValue(0);
  const onScroll = useAnimatedScrollHandler((event) => {
    offset.set(horizontal ? event.contentOffset.x : event.contentOffset.y);
    minimum.set(horizontal ? -event.contentInset.left : -event.contentInset.top);
  });
  return { offset, extent, minimum, horizontal, onScroll };
}
export type ScrollPort = ReturnType<typeof useCardScrollPort>;
export type GestureAdmission = { generation: number; activation: number };

export type DragValues = {
  order: SharedValue<string[]>;
  active: SharedValue<string | null>;
  consumed: SharedValue<boolean>;
  nativeEditing: SharedValue<boolean>;
  nativeGeneration: SharedValue<number>;
  activation: SharedValue<number>;
  admission: SharedValue<GestureAdmission | null>;
  dragX: SharedValue<number>;
  dragY: SharedValue<number>;
  pointer: SharedValue<number>;
  dragScroll: SharedValue<number>;
  startOffset: SharedValue<number>;
  baseline: SharedValue<string[]>;
};
