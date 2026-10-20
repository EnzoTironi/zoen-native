import type { ReactNode } from 'react';
import { Stack } from 'expo-router';
import { ScrollView } from 'react-native';

export function SettingsScreen({
  title,
  testID,
  children,
}: {
  title: string;
  testID: string;
  children: ReactNode;
}) {
  return (
    <ScrollView
      testID={testID}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 24,
        maxWidth: 640,
        width: '100%',
        alignSelf: 'center',
        backgroundColor: 'transparent',
      }}
    >
      <Stack.Screen options={{ title }} />
      {children}
    </ScrollView>
  );
}
