import { View } from 'react-native';
import { router } from 'expo-router';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import type { HomeActionsProps, HomeCardMenuProps } from './menus.types';
export function HomeActions({ hiddenCount = 0, onRestoreHidden }: HomeActionsProps) {
  return (
    <View style={{ gap: 8, alignItems: 'flex-end' }}>
      <NativeButton label={t('home.addMiniapp')} onPress={() => router.push('/miniapps')} />
      {hiddenCount > 0 && onRestoreHidden ? (
        <NativeButton
          testID="home-restore-hidden-apps"
          label={t('home.restoreHiddenCount', { count: hiddenCount })}
          secondary
          onPress={onRestoreHidden}
        />
      ) : null}
    </View>
  );
}
export function HomeCardMenu({ name, pinned, onToggle, onHistory, testID }: HomeCardMenuProps) {
  if (!onToggle)
    return onHistory ? (
      <NativeButton testID={testID} label={t('miniapps.history')} onPress={onHistory} />
    ) : null;
  return (
    <NativeButton
      testID={testID}
      label={`${t(pinned ? 'home.unpin' : 'home.pin')}: ${name}`}
      onPress={onToggle}
    />
  );
}
