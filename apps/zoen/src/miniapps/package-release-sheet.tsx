import { BottomSheet, Host, RNHostView } from '@expo/ui';
import { useCallback, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import type { ItemDetail } from '../core/types';
import { useCoreQuery } from '../core/query';
import { Loading, NativeButton, IconButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { isOwnedAppResource } from './canonical-resource';
import { AppPackageAction } from './package-action';
import { isAppPackage } from './package-intent';

/** Selecting a Core File Item changes the release, never the document's contents. */
export function AppPackageReleaseSheet({
  item,
  onUpdated,
}: {
  item: ItemDetail;
  onUpdated(): void;
}) {
  const { presentation: p, scheme } = useTheme();
  const [visible, setVisible] = useState(false);
  const files = useCoreQuery(visible ? { op: 'items' } : null);
  useFocusEffect(useCallback(() => () => setVisible(false), []));
  if (!item.app || !isOwnedAppResource(item.app.resource_uri)) return null;
  const packages = files.data?.filter(
    (file) => file.space_id === item.space_id && isAppPackage(file),
  );
  return (
    <>
      <NativeButton
        secondary
        testID="app-release-choose"
        label={t('miniapps.updateAppPackage')}
        onPress={() => setVisible(true)}
      />
      <Host colorScheme={scheme}>
        <BottomSheet
          testID="app-release-sheet"
          isPresented={visible}
          onDismiss={() => setVisible(false)}
          snapPoints={['half', 'full']}
          containerColor={p.list.background}
        >
          <RNHostView>
            <ScrollView contentContainerStyle={{ gap: 16, padding: 16, paddingBottom: 32 }}>
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
                <Text
                  accessibilityRole="header"
                  style={{ flex: 1, color: p.list.text, fontSize: 24, fontWeight: '600' }}
                >
                  {t('miniapps.chooseAppPackage')}
                </Text>
                <IconButton
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  label={t('common.close')}
                  onPress={() => setVisible(false)}
                />
              </View>
              {visible && packages ? (
                packages.map((file) => (
                  <AppPackageAction
                    key={file.id}
                    item={file}
                    target={item}
                    onUpdated={() => {
                      setVisible(false);
                      onUpdated();
                    }}
                  />
                ))
              ) : visible && !files.error ? (
                <Loading />
              ) : null}
              {visible && packages?.length === 0 ? (
                <Text style={{ color: p.list.secondary }}>{t('miniapps.noAppPackages')}</Text>
              ) : null}
              {files.error ? (
                <Text accessibilityRole="alert" style={{ color: p.destructive }}>
                  {files.error}
                </Text>
              ) : null}
            </ScrollView>
          </RNHostView>
        </BottomSheet>
      </Host>
    </>
  );
}
