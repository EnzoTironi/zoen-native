import { useCallback, useState } from 'react';
import { router, Stack, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useClient } from '../../../core/provider';
import { resolveMessageLink } from '../../../core/message-link';
import { EmptyState, Loading, NativeButton } from '../../../components/primitives';
import { t } from '../../../i18n/index.ts';

export default function MessageLink() {
  const { space, message } = useLocalSearchParams<{ space: string; message: string }>();
  const { store } = useClient();
  const [failed, setFailed] = useState<number | null>(null);
  const [retry, setRetry] = useState(0);
  useFocusEffect(
    useCallback(() => {
      let active = true;
      setFailed(null);
      void resolveMessageLink(store, space, message, () => active)
        .then((destination) => {
          if (!active) return;
          if (destination) router.replace(destination);
          else setFailed(retry);
        })
        .catch(() => {
          if (active) setFailed(retry);
        });
      return () => {
        active = false;
      };
    }, [store, space, message, retry]),
  );
  return (
    <>
      <Stack.Screen options={{ title: '' }} />
      {failed === retry ? (
        <EmptyState title={t('chat.messageUnavailable')}>
          <NativeButton
            label={t('chat.tryAgain2')}
            onPress={() => setRetry((value) => value + 1)}
          />
        </EmptyState>
      ) : (
        <Loading />
      )}
    </>
  );
}
