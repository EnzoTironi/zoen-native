import { Text } from 'react-native';
import { useLocalSearchParams } from 'expo-router';
import { t } from '../../i18n';
import { conversationAttachmentCategory } from '../../media/attachment-categories';
import { AttachmentRoute } from '../../media/attachment-route';
import type { AttachmentDestination } from '../../media/attachment-route-types';

/** One route pattern; all writes still go through the existing canonical text/media/App owners. */
export default function ChatAttachment() {
  const params = useLocalSearchParams<{
    id: string;
    kind: string;
    thread?: string;
    identity: string;
    device: string;
    epoch: string;
  }>();
  const kind = conversationAttachmentCategory(params.kind);
  const destination: AttachmentDestination = {
    id: params.id,
    thread: params.thread || null,
    identity: params.identity,
    device: params.device,
    epoch: params.epoch,
  };
  if (
    !kind ||
    Object.values(destination).some((value) => value !== null && typeof value !== 'string')
  )
    return <Text accessibilityRole="alert">{t('attachmentTabs.unavailable')}</Text>;
  return (
    <AttachmentRoute
      key={JSON.stringify([kind, destination])}
      kind={kind}
      destination={destination}
    />
  );
}
