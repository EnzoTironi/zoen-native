import { useCallback, useEffect, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { AppState, Text, View } from 'react-native';
import { NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { homeSpacesFor } from '../home/access';
import { t } from '../i18n/index';
import { useTheme } from '../theme';
import { discussionScope, discussionTarget } from './discussion';

/** Pages and other artifacts open the same encrypted conversation thread. */
export function ArtifactDiscussion({ itemId, spaceId }: { itemId: string; spaceId: string }) {
  const state = useClient();
  const { store } = state;
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
  const [activation, setActivation] = useState<object | null>(null);
  const currentActivation = useRef<object | null>(null);
  const focused = useRef(false);
  const opened = useRef<object | null>(null);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      const token = AppState.currentState === 'active' ? {} : null;
      currentActivation.current = token;
      setActivation(token);
      return () => {
        focused.current = false;
        currentActivation.current = null;
      };
    }, []),
  );
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      const token = state === 'active' && focused.current ? {} : null;
      currentActivation.current = token;
      setActivation(token);
    });
    return () => {
      currentActivation.current = null;
      subscription.remove();
    };
  }, []);
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
        <NativeButton secondary wrapLabel label={t('documents.retryDiscussion')} onPress={retry} />
      </View>
    );
  if (data && !data.root) return null;
  return (
    <NativeButton
      secondary
      wrapLabel
      disabled={!data?.root || !activation}
      label={
        !data
          ? t('documents.loadingDiscussion')
          : data.root?.thread_replies
            ? t('documents.discussionCount', { count: data.root.thread_replies })
            : t('documents.discussion')
      }
      onPress={() => {
        // One opening belongs to this visible activation, never a later return.
        if (
          !data ||
          !activation ||
          currentActivation.current !== activation ||
          opened.current === activation ||
          store.isSessionRetiring
        )
          return;
        const target = discussionTarget(
          store.getSnapshot(),
          store.sessionEpoch,
          scope,
          spaceId,
          itemId,
          data,
        );
        if (target) {
          opened.current = activation;
          router.push({ pathname: '/thread/[space]/[root]', params: target });
        }
      }}
    />
  );
}
