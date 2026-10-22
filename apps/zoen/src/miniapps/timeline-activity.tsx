import { router } from 'expo-router';
import { Text } from 'react-native';
import type { MiniappTimelineActivity } from '../core/conversation-timeline';
import { messageText } from '../core/presentation';
import { t } from '../i18n/index.ts';
import { ServiceMessage } from '../components/service-message';
import { messageDeliveryLabel } from '../components/message-receipt';

/** Typed App changes retain their canonical audit entries and history action. */
export function MiniappTimelineActivityView({
  activity,
  highlighted = false,
  disabled = false,
}: {
  activity: MiniappTimelineActivity;
  highlighted?: boolean;
  disabled?: boolean;
}) {
  const entry = activity.entries.at(-1);
  const edit = entry && 'ItemEdited' in entry.kind ? entry.kind.ItemEdited : null;
  const deleted = !entry || entry.deleted;
  const note = entry ? messageText(entry) : '';
  const attribution =
    !deleted && entry
      ? activity.entries.length > 1
        ? t('miniapps.timelineUpdates', { name: entry.author.name, count: activity.entries.length })
        : entry.author.name
      : '';
  const title = !deleted && edit && !note.includes(edit.title) ? edit.title : '';
  const delivery =
    !deleted && entry && ['Local', 'Sending', 'Failed'].includes(entry.delivery)
      ? messageDeliveryLabel(entry.delivery)
      : '';
  const label = [attribution, note, title, delivery].filter(Boolean).join(' · ');
  if (!entry || !edit) return null;
  const canOpen = !deleted && !disabled && activity.item?.id === edit.item_id;
  return (
    <ServiceMessage
      id={entry.id}
      label={label}
      highlighted={highlighted}
      testID={`miniapp-activity-${activity.entries[0]!.id}`}
      actionTestID={`miniapp-activity-${canOpen ? 'history' : 'label'}-${entry.id}`}
      backgroundTestID={`miniapp-activity-background-${entry.id}`}
      textTestID={`miniapp-activity-text-${entry.id}`}
      accessibilityHint={canOpen ? t('miniapps.timelineHistory', { title: edit.title }) : undefined}
      onPress={
        canOpen
          ? () =>
              router.push({ pathname: '/item/[id]', params: { id: edit.item_id, history: '1' } })
          : undefined
      }
    >
      {attribution ? <Text style={{ fontWeight: '500' }}>{attribution}</Text> : null}
      {[note, title, delivery].filter(Boolean).map((part, index) => (
        <Text key={index}>{`${attribution || index > 0 ? ' · ' : ''}${part}`}</Text>
      ))}
    </ServiceMessage>
  );
}
