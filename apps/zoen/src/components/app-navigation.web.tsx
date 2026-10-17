import { Tabs, TabList, TabSlot, TabTrigger } from 'expo-router/ui';
import { Pressable, Text, useWindowDimensions } from 'react-native';
import { useTheme } from '../theme';

const destinations = [
  { name: 'chats', href: '/', title: 'Chats' },
  { name: 'activity', href: '/activity', title: 'Atividade' },
  { name: 'files', href: '/files', title: 'Arquivos' },
  { name: 'you', href: '/you', title: 'Você' },
] as const;
export default function AppNavigation() {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const wide = width >= 820;
  return (
    <Tabs
      style={{
        flex: 1,
        flexDirection: wide ? 'row' : 'column',
        backgroundColor: colors.background,
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
