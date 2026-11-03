import {
  createContext,
  use,
  type ReactElement,
  type ReactNode,
  type ComponentType,
  type ComponentProps,
} from 'react';
import type { LayoutAnimationFunction } from 'react-native-reanimated';
import type { MessagePressable } from './message-interaction';
import { View, type ViewProps } from 'react-native';
export const QuoteLayout = createContext<{
  layout: LayoutAnimationFunction;
  begin: (generation: number) => void;
  finish: (generation: number) => void;
  markdown: (element: ReactElement, generation: number) => ReactNode;
  View: ComponentType<ViewProps>;
  Pressable: ComponentType<ComponentProps<typeof MessagePressable>>;
} | null>(null);
export const useQuoteLayoutTransition = () => use(QuoteLayout);
export function QuoteMotionView(props: ViewProps) {
  const Component = use(QuoteLayout)?.View ?? View;
  return <Component {...props} />;
}
