import { SkyScene } from '../../components/sky-background';
import { t } from '../../i18n/index.ts';
import { useCallback, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { Pressable, Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import type { ItemDetail } from '../../core/types';
import { useTheme } from '../../theme';
import { EmptyState, Icon, NativeButton } from '../../components/primitives';

function FilesContent() {
  const { store, snapshot } = useClient();
  const { colors } = useTheme();
  const [items, setItems] = useState<ItemDetail[]>([]);
  const [error, setError] = useState<string | null>(null);
  useFocusEffect(
    useCallback(() => {
      if (!snapshot) return;
      let current = true;
      void store.driver
        .request({ op: 'items' })
        .then((result) => {
          if (current) {
            setItems(result);
            setError(null);
          }
        })
        .catch((failure: unknown) => {
          if (current)
            setError(
              failure instanceof Error ? failure.message : t('documents.yourFilesCouldNotBeOpened'),
            );
        });
      return () => {
        current = false;
      };
    }, [store, snapshot]),
  );
  return (
    <View style={{ flex: 1, backgroundColor: 'transparent' }}>
      <FlashList
        data={items}
        keyExtractor={(item) => item.id}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{ paddingHorizontal: 20, paddingBottom: 24 }}
        ListHeaderComponent={
          <View style={{ paddingHorizontal: 4, paddingVertical: 24, gap: 12 }}>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 34, fontWeight: '700', letterSpacing: -1 }}
            >
              {t('documents.madeToStay')}
            </Text>
            <Text style={{ color: colors.secondary, fontSize: 16 }}>
              {t('documents.plansPagesAndFilesFromYourChats')}
            </Text>
            <NativeButton
              secondary
              label={t('documents.newPage')}
              onPress={() => router.push('/new-page')}
            />
            {error ? (
              <Text accessibilityRole="alert" style={{ color: colors.danger }}>
                {error}
              </Text>
            ) : null}
          </View>
        }
        ListEmptyComponent={
          <EmptyState
            title={t('documents.aHomeForYourIdeas')}
            detail={t('documents.everythingYouCreateAndShareInChatsIs')}
            icon={{ ios: 'folder', android: 'folder', web: 'folder' }}
          />
        }
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={t('documents.openNamedItem', { title: item.title })}
            onPress={() => router.push({ pathname: '/item/[id]', params: { id: item.id } })}
            style={{
              flexDirection: 'row',
              alignItems: 'center',
              gap: 16,
              padding: 18,
              marginBottom: 10,
              backgroundColor: 'transparent',
              borderRadius: 22,
              borderCurve: 'continuous',
            }}
          >
            <View
              style={{
                width: 44,
                height: 48,
                backgroundColor: colors.muted,
                borderRadius: 13,
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Icon
                name={{
                  ios: item.kind_id === 'plan' ? 'checklist' : 'doc.text',
                  android: item.kind_id === 'plan' ? 'checklist' : 'description',
                  web: item.kind_id === 'plan' ? 'checklist' : 'description',
                }}
                color={colors.accent}
              />
            </View>
            <View style={{ flex: 1, gap: 5 }}>
              <Text
                numberOfLines={2}
                style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
              >
                {item.title}
              </Text>
              <Text style={{ color: colors.secondary, fontSize: 13 }}>
                {item.space_title} · {item.kind_label}
              </Text>
            </View>
            <Text style={{ color: colors.tertiary, fontSize: 12 }}>v{item.version}</Text>
          </Pressable>
        )}
      />
    </View>
  );
}

export default function Files() {
  return (
    <SkyScene>
      <FilesContent />
    </SkyScene>
  );
}
