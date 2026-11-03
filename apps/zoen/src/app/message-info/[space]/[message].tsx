import { router, Stack, useLocalSearchParams } from 'expo-router';
import { MessageInfoSheet } from '../../../components/message-info-sheet';
import { useMessageInfo } from '../../../components/use-message-info';
import { NativeButton } from '../../../components/primitives';
import { t } from '../../../i18n/index.ts';

export default function MessageInfo() {
  const { space, message, owner } = useLocalSearchParams<{
    space: string;
    message: string;
    owner: string;
  }>();
  const state = useMessageInfo(space, message, owner);
  return (
    <>
      <Stack.Screen
        options={{
          title: t('messageInfo.title'),
          headerRight: () => (
            <NativeButton label={t('common.close')} onPress={() => router.back()} />
          ),
        }}
      />
      <MessageInfoSheet {...state} />
    </>
  );
}
