import type { ComponentProps } from 'react';
import { Text } from 'react-native';
import { MessageContext } from './message-context';
import { ReplyOptions } from './reply-options';
import { useReplyOptions } from './use-reply-options';
import { useTheme } from '../theme';

/** Attachment previews remain passive; only the normal composer edits its durable reply. */
export function ComposerReplyContext({
  spaceId,
  threadRootId,
  ...context
}: ComponentProps<typeof MessageContext> & { spaceId: string; threadRootId?: string }) {
  const options = useReplyOptions(spaceId, threadRootId ?? null);
  const { colors } = useTheme();
  return (
    <>
      <MessageContext
        {...context}
        onOptions={
          !context.editing && !options.pending && options.supported ? options.open : undefined
        }
      />
      {!options.entry && options.error ? (
        <Text accessibilityRole="alert" style={{ color: colors.secondary, paddingHorizontal: 16 }}>
          {options.error}
        </Text>
      ) : null}
      <ReplyOptions options={options} />
    </>
  );
}
