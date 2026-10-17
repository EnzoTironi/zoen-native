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
      <EmptyState title="Não foi possível abrir o pedido" detail={read.error}>
        <NativeButton label="Tentar novamente" onPress={read.retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  if (!request)
    return (
      <EmptyState
        title="Pedido indisponível"
        detail="Este pedido ainda não chegou neste aparelho ou não pertence às suas conversas."
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
      <Stack.Screen options={{ title: 'Revisar pedido' }} />
      <RequestCard request={request} onResolved={read.retry} />
    </ScrollView>
  );
}
