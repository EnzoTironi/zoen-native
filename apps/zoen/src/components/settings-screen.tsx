import type { ReactNode } from 'react';
import { Stack } from 'expo-router';
import { ScrollView } from 'react-native';
import { useTheme } from '../theme';

export function SettingsScreen({
  title,
  testID,
  children,
}: {
  title: string;
  testID: string;
  children: ReactNode;
}) {
  const { presentation: p } = useTheme();
  return (
    <ScrollView
      testID={testID}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 16,
        paddingBottom: 48,
        gap: 32,
        maxWidth: 640,
        width: '100%',
        alignSelf: 'center',
        backgroundColor: 'transparent',
      }}
    >
      <Stack.Screen
        options={{
          title,
          headerTintColor: p.navigation.control,
          headerTitleStyle: { ...p.typography.title, color: p.navigation.text },
        }}
      />
      {children}
    </ScrollView>
  );
}
