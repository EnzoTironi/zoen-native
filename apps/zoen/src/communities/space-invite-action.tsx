import { useState } from 'react';
import { BottomSheet, Column, Host, Picker, RNHostView, Row, Text as NativeText } from '@expo/ui';
import { ScrollView, Share, Text, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { ProfileInfoRow } from '../profile/profile-section';
import { IconButton, Loading, NativeButton } from '../components/primitives';
import { messageInfoDate } from '../core/message-info-presentation';
import { t, formatLocale } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { useSpaceInvite } from './use-space-invite';
import { inviteDurations, inviteUses } from './space-invite-actions';

export function SpaceInviteAction({
  space,
  disabled = false,
}: {
  space: string;
  disabled?: boolean;
}) {
  const invite = useSpaceInvite(space);
  const [copied, setCopied] = useState<string | null>(null);
  const { presentation: p, scheme } = useTheme();
  if (!invite.available || (invite.ready && !invite.permitted)) return null;
  const expiryLabels = [
    t('spaceInvite.hour'),
    t('spaceInvite.day'),
    t('spaceInvite.week'),
    t('spaceInvite.month'),
  ];
  return (
    <>
      <ProfileInfoRow
        label={t('chat.inviteSomeone')}
        disabled={disabled || (!invite.permitted && !invite.readFailed)}
        onPress={invite.readFailed ? invite.retry : invite.show}
      />
      <Host
        style={{ position: 'absolute' }}
        pointerEvents="none"
        colorScheme={scheme}
        seedColor={p.list.accent}
      >
        <BottomSheet
          testID="space-invite-sheet"
          isPresented={invite.open}
          onDismiss={invite.close}
          snapPoints={['half', 'full']}
          containerColor={p.list.background}
        >
          <RNHostView>
            <ScrollView
              contentInsetAdjustmentBehavior="automatic"
              contentContainerStyle={{ padding: 20, gap: 20, paddingBottom: 32 }}
            >
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
                <Text
                  accessibilityRole="header"
                  style={{ color: p.list.text, fontSize: 22, fontWeight: '600', flex: 1 }}
                >
                  {t('spaceInvite.title')}
                </Text>
                <IconButton
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  label={t('common.close')}
                  onPress={invite.close}
                />
              </View>
              <Text style={{ color: p.list.secondary, fontSize: 17 }}>{invite.title}</Text>
              {invite.loading ? (
                <Loading />
              ) : invite.result ? (
                <>
                  <Text selectable style={{ color: p.list.text, fontSize: 17 }}>
                    {invite.result.link}
                  </Text>
                  <Text style={{ color: p.list.secondary }}>
                    {t('spaceInvite.expires', {
                      date:
                        messageInfoDate(invite.result.expires_at_ms, formatLocale) ??
                        t('messageInfo.timeUnavailable'),
                    })}
                  </Text>
                  <NativeButton
                    disabled={invite.busy}
                    testID="space-invite-share"
                    label={t('spaceInvite.share')}
                    onPress={() => invite.disclose((link) => Share.share({ message: link }))}
                  />
                  <NativeButton
                    disabled={invite.busy}
                    testID="space-invite-copy"
                    secondary
                    label={t(
                      copied === invite.result.link ? 'spaceInvite.copied' : 'spaceInvite.copy',
                    )}
                    onPress={() =>
                      invite.disclose(
                        (link) => Clipboard.setStringAsync(link),
                        () => setCopied(invite.result?.link ?? null),
                      )
                    }
                  />
                </>
              ) : (
                <>
                  <Host
                    matchContents={{ vertical: true }}
                    style={{ width: '100%' }}
                    colorScheme={scheme}
                    seedColor={p.list.accent}
                  >
                    <Column spacing={16}>
                      <Row spacing={12}>
                        <NativeText>{t('spaceInvite.expiry')}</NativeText>
                        <Picker
                          testID="space-invite-expiry"
                          enabled={!invite.busy}
                          selectedValue={invite.ttl}
                          onValueChange={invite.setTtl}
                        >
                          {inviteDurations.map((value, index) => (
                            <Picker.Item key={value} value={value} label={expiryLabels[index]!} />
                          ))}
                        </Picker>
                      </Row>
                      <Row spacing={12}>
                        <NativeText>{t('spaceInvite.uses')}</NativeText>
                        <Picker
                          testID="space-invite-uses"
                          enabled={!invite.busy}
                          selectedValue={invite.uses}
                          onValueChange={invite.setUses}
                        >
                          {inviteUses.map((value) => (
                            <Picker.Item
                              key={value}
                              value={value}
                              label={
                                value === 1
                                  ? t('spaceInvite.singleUse')
                                  : t('spaceInvite.usage', {
                                      count: value,
                                    })
                              }
                            />
                          ))}
                        </Picker>
                      </Row>
                    </Column>
                  </Host>
                  <NativeButton
                    testID="space-invite-create"
                    label={t(invite.busy ? 'spaceInvite.creating' : 'spaceInvite.create')}
                    disabled={!invite.permitted || invite.busy}
                    onPress={invite.create}
                  />
                </>
              )}
              {invite.error ? (
                <Text accessibilityRole="alert" style={{ color: p.list.destructive }}>
                  {t('spaceInvite.failed')}
                </Text>
              ) : null}
            </ScrollView>
          </RNHostView>
        </BottomSheet>
      </Host>
    </>
  );
}
