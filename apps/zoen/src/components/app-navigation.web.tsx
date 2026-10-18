import { t } from '../i18n/index.ts';
import { Tabs, TabList, TabSlot, TabTrigger } from 'expo-router/ui';
import { Pressable, Text, useWindowDimensions } from 'react-native';
import { useTheme } from '../theme';

export default function AppNavigation() {
  const destinations = [
    { name: 'home', href: '/', title: t('home.title') },
    { name: 'chats', href: '/chats', title: t('common.chats') },
    { name: 'files', href: '/files', title: t('common.files') },
    { name: 'you', href: '/you', title: t('common.you') },
  ] as const;
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const wide = width >= 820;
  return (
    <Tabs
      style={{
        flex: 1,
        flexDirection: wide ? 'row' : 'column',
        backgroundColor: 'transparent',
      }}
    >
      <TabList
        style={{
          flexDirection: wide ? 'column' : 'row',
          width: wide ? 190 : '100%',
          padding: 20,
          gap: 10,
          backgroundColor: colors.surface,
        }}
      >
        {wide ? (
          <Text
            style={{
              fontSize: 27,
              fontWeight: '700',
              letterSpacing: -1,
              color: colors.accent,
              padding: 12,
              marginBottom: 24,
            }}
          >
            zoen
          </Text>
        ) : null}
        {destinations.map((destination) => (
          <TabTrigger
            key={destination.name}
            name={destination.name}
            href={destination.href}
            asChild
          >
            <Pressable style={{ paddingHorizontal: 14, paddingVertical: 14, borderRadius: 14 }}>
              <Text style={{ color: colors.text, fontSize: 16, fontWeight: '600' }}>
                {destination.title}
              </Text>
            </Pressable>
          </TabTrigger>
        ))}
      </TabList>
      <TabSlot style={{ flex: 1, minWidth: 0 }} />
    </Tabs>
  );
}
