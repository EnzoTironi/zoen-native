import { use } from 'react';
import { View, type ViewProps } from 'react-native';
import {
  MessageContextOwnsPress,
  MessageContextStageCommands,
  MessageContextControlBarrier,
} from './message-context-ports';
/** Keep nested real Pressables, with an actual native-removal navigation barrier. */
export function MessageContextControl({
  controlKey,
  ...props
}: ViewProps & { controlKey: string }) {
  const dismiss = use(MessageContextStageCommands);
  return (
    <MessageContextOwnsPress value={false}>
      <MessageContextControlBarrier value={dismiss}>
        <View {...props} collapsable={false} testID={`message-context-control:${controlKey}`} />
      </MessageContextControlBarrier>
    </MessageContextOwnsPress>
  );
}
