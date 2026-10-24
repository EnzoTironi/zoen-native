import { Stack, useLocalSearchParams } from 'expo-router';
import { useCoreQuery } from '../../core/query';
import { EmptyState, Loading, NativeButton } from '../../components/primitives';
import { useClient } from '../../core/provider';
import { AccountBudgetScreen } from '../../agents/account-budget-screen';
import { OwnerPolicyScreen } from '../../agents/owner-policy-screen';
import { imageToolReviewValid } from '../../core/image-tool-review';
import { t } from '../../i18n/index.ts';

export default function OwnerBudget() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { snapshot } = useClient();
  const read = useCoreQuery({ op: 'requests' });
  const request = read.data?.find((entry) => entry.id === id);
  const tool = request?.tool_approval;
  if (!read.data)
    return read.error ? (
      <EmptyState title={t('ownerPolicy.unavailable')} detail={read.error}>
        <NativeButton label={t('agents.tryAgain')} onPress={read.retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  if (
    !request?.agent.is_mine ||
    tool?.canonical_tool !== 'generate_image' ||
    !imageToolReviewValid(tool.image)
  )
    return <EmptyState title={t('ownerPolicy.unavailable')} />;
  const conversation = snapshot?.conversations.find((space) => space.id === request.space_id);
  if (conversation && conversation.kind !== 'Direct')
    return <AccountBudgetScreen agentId={request.agent.id} />;
  return (
    <>
      <Stack.Screen options={{ title: t('ownerPolicy.title') }} />
      <OwnerPolicyScreen key={`${request.id}:${tool.content_hash}`} request={request} />
    </>
  );
}
