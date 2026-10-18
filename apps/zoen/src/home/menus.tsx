import { router } from 'expo-router';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import type { HomeActionsProps, HomeCardMenuProps } from './menus.types';
export function HomeActions(_props: HomeActionsProps) {
  return <NativeButton label={t('home.addMiniapp')} onPress={() => router.push('/miniapps')} />;
}
export function HomeCardMenu({ name, pinned, onToggle, testID }: HomeCardMenuProps) {
  return (
    <NativeButton
      testID={testID}
      label={`${t(pinned ? 'home.unpin' : 'home.pin')}: ${name}`}
      onPress={onToggle}
    />
  );
}
