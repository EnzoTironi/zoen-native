import { t } from '../i18n';
import { Stack } from 'expo-router';
import { EmptyState } from '../components/primitives';

export default function Backup() {
  return (
    <>
      <Stack.Screen options={{ title: t('access.backupTitle') }} />
      <EmptyState title={t('access.backupIosOnly')} detail={t('access.backupWebExplanation')} />
    </>
  );
}
