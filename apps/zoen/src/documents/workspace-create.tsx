import { router } from 'expo-router';
import { IconButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
export function WorkspaceCreate({ space }: { space: string }) {
  return (
    <IconButton
      name={{ ios: 'plus', android: 'add', web: 'add' }}
      label={t('workspace.newPage')}
      onPress={() =>
        router.push(space ? { pathname: '/new-page', params: { space } } : '/new-page')
      }
    />
  );
}
