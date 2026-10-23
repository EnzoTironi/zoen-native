import type { ReactNode } from 'react';
import type { SymbolViewProps } from 'expo-symbols';

export type SettingsRow = {
  id: string;
  title: string;
  hint?: string;
  icon?: SymbolViewProps['name'];
  leading?: ReactNode;
  iconColor?: string;
  supportingText?: string;
  onPress: () => void;
};
