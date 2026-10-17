import { t } from '../i18n/index.ts';
import { NativeTabs } from 'expo-router/native-tabs';
import { useTheme } from '../theme';
export default function AppNavigation() {
  const { colors } = useTheme();
  return (
    <NativeTabs tintColor={colors.accent}>
      <NativeTabs.Trigger name="(chats)">
        <NativeTabs.Trigger.Label>{t('common.chats')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="bubble.left.and.bubble.right" md="chat" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="activity">
        <NativeTabs.Trigger.Label>{t('common.activity')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="square.stack" md="notifications" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="files">
        <NativeTabs.Trigger.Label>{t('common.files')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="folder" md="folder" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="you">
        <NativeTabs.Trigger.Label>{t('common.you')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="person.crop.circle" md="account_circle" />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
