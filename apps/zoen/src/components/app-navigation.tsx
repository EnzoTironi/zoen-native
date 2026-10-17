import { NativeTabs } from 'expo-router/native-tabs';
import { useTheme } from '../theme';
export default function AppNavigation() {
  const { colors } = useTheme();
  return (
    <NativeTabs tintColor={colors.accent}>
      <NativeTabs.Trigger name="(chats)">
        <NativeTabs.Trigger.Label>Chats</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="bubble.left.and.bubble.right" md="chat" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="activity">
        <NativeTabs.Trigger.Label>Atividade</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="square.stack" md="notifications" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="files">
        <NativeTabs.Trigger.Label>Arquivos</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="folder" md="folder" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="you">
        <NativeTabs.Trigger.Label>Você</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="person.crop.circle" md="account_circle" />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
