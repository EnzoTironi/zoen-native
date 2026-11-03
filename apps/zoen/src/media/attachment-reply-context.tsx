import { Text, View } from 'react-native';
import { MessageContext } from '../components/message-context';
import { IconButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { messageCanInteract } from '../core/message-presentation';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { AttachmentDraftScope } from './attachment-draft';

type Props = { scope: AttachmentDraftScope; onCancel?: () => void };

/** A batch advertises its captured destination, even after the parent draft changes. */
export function AttachmentReplyContext({ scope, onCancel }: Props) {
  const client = useClient();
  const owner = homeOwnerFor(client);
  if (!scope.replyTo && !scope.sharedReply) return null;
  if (
    !owner ||
    owner.mode !== scope.mode ||
    owner.identityId !== scope.identityId ||
    owner.deviceId !== scope.deviceId ||
    !homeSpacesFor(client).has(scope.spaceId)
  )
    return <ReplyNotice label={t('media.attachmentDraftUnavailable')} />;
  if (scope.sharedReply) return <MessageContext shared={scope.sharedReply} onCancel={onCancel} />;
  return (
    <ReplyReader
      key={JSON.stringify([
        scope.mode,
        scope.identityId,
        scope.deviceId,
        scope.spaceId,
        scope.replyTo,
      ])}
      scope={scope}
      onCancel={onCancel}
    />
  );
}

function ReplyReader({ scope, onCancel }: Props) {
  const { data, error } = useCoreQuery({
    op: 'message',
    space_id: scope.spaceId,
    message_id: scope.replyTo!,
  });
  if (!error && data && data.id === scope.replyTo && messageCanInteract(data))
    return <MessageContext entry={data} quote={scope.replyQuote} onCancel={onCancel} />;
  return (
    <ReplyNotice
      label={t(error || data ? 'chat.messageUnavailable' : 'media.loading')}
      onCancel={onCancel}
    />
  );
}

function ReplyNotice({ label, onCancel }: { label: string; onCancel?: () => void }) {
  const { colors } = useTheme();
  return (
    <View
      testID="attachment-reply-notice"
      style={{ marginHorizontal: 20, flexDirection: 'row', alignItems: 'center', gap: 10 }}
    >
      <Text
        selectable
        style={{ flex: 1, color: colors.secondary, fontSize: 13, paddingVertical: 7 }}
      >
        {label}
      </Text>
      {onCancel ? (
        <IconButton
          label={t('chat.cancel')}
          name={{ ios: 'xmark', android: 'close', web: 'close' }}
          onPress={onCancel}
        />
      ) : null}
    </View>
  );
}
