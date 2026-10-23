import { router } from 'expo-router';
import { Text, View } from 'react-native';
import { NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { homeSpacesFor } from '../home/access';
import { t } from '../i18n/index';
import { useTheme } from '../theme';
import { discussionScope, discussionTarget } from './discussion';

/** Pages and other artifacts open the same encrypted conversation thread. */
export function ArtifactDiscussion({ itemId, spaceId }: { itemId: string; spaceId: string }) {
  const { store } = useClient();
  const state = store.getSnapshot();
  const scope = discussionScope(state, store.sessionEpoch);
  if (!scope || !homeSpacesFor(state).has(spaceId)) return null;
  return (
    <Discussion
      key={`${scope}:${spaceId}:${itemId}`}
      scope={scope}
      itemId={itemId}
      spaceId={spaceId}
    />
  );
}

function Discussion({
  scope,
  itemId,
  spaceId,
}: {
  scope: string;
  itemId: string;
  spaceId: string;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const { data, error, retry } = useCoreQuery({
    op: 'item_discussion',
    space_id: spaceId,
    item_id: itemId,
  });
  if (error)
    return (
      <View style={{ gap: 8 }}>
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {error}
        </Text>
        <NativeButton secondary label={t('documents.retryDiscussion')} onPress={retry} />
      </View>
    );
  if (data && !data.root) return null;
  return (
    <NativeButton
      secondary
      disabled={!data?.root}
      label={
        !data
          ? t('documents.loadingDiscussion')
          : data.root?.thread_replies
            ? t('documents.discussionCount', { count: data.root.thread_replies })
            : t('documents.discussion')
      }
      onPress={() => {
        if (!data) return;
        const target = discussionTarget(
          store.getSnapshot(),
          store.sessionEpoch,
          scope,
          spaceId,
          itemId,
          data,
        );
        if (target) router.push({ pathname: '/thread/[space]/[root]', params: target });
      }}
    />
  );
}
