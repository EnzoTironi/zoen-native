import { t } from '../../i18n/index.ts';
import { Stack, useLocalSearchParams } from 'expo-router';
import { ScrollView } from 'react-native';
import { useCoreQuery } from '../../core/query';
import { EmptyState, Loading, NativeButton } from '../../components/primitives';
import { RequestCard } from '../../agents/request-card';

export default function RequestReview() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const read = useCoreQuery({ op: 'requests' });
  const request = read.data?.find((request) => request.id === id);
  if (!read.data)
    return read.error ? (
      <EmptyState title={t('agents.theRequestCouldNotBeOpened')} detail={read.error}>
        <NativeButton label={t('agents.tryAgain')} onPress={read.retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  if (!request)
    return (
      <EmptyState
        title={t('agents.requestUnavailable')}
        detail={t('agents.thisRequestHasNotReachedThisDeviceYet')}
      />
    );
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        maxWidth: 740,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: t('agents.reviewRequest') }} />
      <RequestCard request={request} onResolved={read.retry} />
    </ScrollView>
  );
}
