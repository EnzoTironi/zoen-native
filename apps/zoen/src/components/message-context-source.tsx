import { useImperativeHandle, type ReactNode, type Ref } from 'react';
import type { StyleProp, ViewStyle } from 'react-native';
import type { MessageContextRequest, MessageContextStageHandle } from './message-context-ports';
export type { MessageContextStageHandle } from './message-context-ports';
export type MessageContextSourceProps = Readonly<{
  ownerKey: string;
  active: boolean;
  handlesTap: boolean;
  reducedMotion?: boolean;
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
  onGesture(request: MessageContextRequest): void;
}>;
/** Deferred platforms retain their existing safe context menu and gesture ownership. */
export function MessageContextStage({
  children,
  ref,
}: {
  children: ReactNode;
  active?: boolean;
  ref?: Ref<MessageContextStageHandle>;
}) {
  useImperativeHandle(
    ref,
    () => ({ retireContexts: async () => {}, dismissContexts: async () => {} }),
    [],
  );
  return children;
}
export function MessageContextSource({ children }: MessageContextSourceProps) {
  return children;
}

export { MessageContextControl } from './message-context-control';
