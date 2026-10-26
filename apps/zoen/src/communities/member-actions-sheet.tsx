import { BottomSheet, Host, RNHostView } from '@expo/ui';
import { MenuView } from '@expo/ui/community/menu';
import { ScrollView, Text, View } from 'react-native';
import { router } from 'expo-router';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { Icon, IconButton, Loading, NativeButton } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { SettingsRows } from '../components/settings-rows';
import { ProfileAvatar } from '../profile/profile-avatar';
import type { MemberActionsSession, MemberActionsState } from './member-actions';

function roleLabel(role: string | null) {
  return t(
    role === 'owner'
      ? 'community.owner'
      : role === 'admin'
        ? 'community.admin'
        : role === 'reader'
          ? 'community.reader'
          : 'community.member',
  );
}

export function MemberActionsSheet({
  session,
  state,
}: {
  session: MemberActionsSession;
  state: MemberActionsState;
}) {
  const { presentation: p, scheme } = useTheme();
  const { confirm } = useDialogs();
  const context = state.context;
  const member = context?.member;
  // The rendered snapshot is a real Compiler dependency, not a mutable class read.
  const ticket = session.ticket(state);
  return (
    <Host
      style={{ position: 'absolute' }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={p.list.accent}
    >
      <BottomSheet
        testID="member-actions-sheet"
        isPresented={Boolean(state.selected)}
        onDismiss={() => session.close(ticket)}
        snapPoints={['half', 'full']}
        containerColor={p.list.background}
      >
        <RNHostView>
          <ScrollView
            contentInsetAdjustmentBehavior="automatic"
            contentContainerStyle={{ gap: 20, paddingVertical: 16, paddingBottom: 32 }}
          >
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
              {member ? (
                <ProfileAvatar
                  person={member}
                  name={member.name}
                  color={member.tint_hex}
                  size={40}
                />
              ) : null}
              <Text
                accessibilityRole="header"
                style={{ flex: 1, color: p.list.text, fontSize: 22, fontWeight: '600' }}
              >
                {member?.name ?? t('community.member')}
              </Text>
              <IconButton
                name={{ ios: 'xmark', android: 'close', web: 'close' }}
                label={t('common.close')}
                onPress={() => session.close(ticket)}
              />
            </View>
            {state.phase === 'loading' ? <Loading /> : null}
            {state.phase === 'failed' ? (
              <View style={{ gap: 12 }}>
                <Text accessibilityRole="alert" selectable style={{ color: p.destructive }}>
                  {t('community.readFailed')}
                </Text>
                <NativeButton
                  label={t('chat.tryAgain2')}
                  onPress={() => {
                    void session.refresh(ticket);
                  }}
                />
              </View>
            ) : null}
            {state.phase === 'ready' && member && context ? (
              <>
                {context.role_options.length ? (
                  <MenuView
                    testID="member-role-menu"
                    colorScheme={scheme}
                    actions={context.role_options.map((option) => ({
                      id: option.revision,
                      title: roleLabel(option.role),
                      attributes: { disabled: state.busy },
                    }))}
                    onPressAction={({ nativeEvent }) => {
                      const option = context.role_options.find(
                        (choice) => choice.revision === nativeEvent.event,
                      );
                      if (!option) return;
                      void session.setRole(
                        option,
                        (review) =>
                          confirm({
                            title: t('community.changeRoleNamed', {
                              name: review.member?.name ?? t('community.member'),
                              role: roleLabel(option.role),
                            }),
                            message: t('community.changeRoleIn', { title: review.space_title }),
                            confirmLabel: t('community.changeRole'),
                          }),
                        ticket,
                      );
                    }}
                  >
                    <View
                      accessible
                      accessibilityRole="button"
                      accessibilityLabel={`${t('community.changeRole')}, ${roleLabel(context.role)}`}
                      style={{
                        minHeight: 44,
                        paddingHorizontal: 16,
                        flexDirection: 'row',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                        borderRadius: 10,
                        backgroundColor: p.list.block,
                      }}
                    >
                      <Text
                        testID="member-current-role"
                        style={{ color: p.list.text, fontSize: 17 }}
                      >
                        {roleLabel(context.role)}
                      </Text>
                      <Icon
                        name={{
                          ios: 'chevron.up.chevron.down',
                          android: 'unfold_more',
                          web: 'unfold_more',
                        }}
                        color={p.list.accent}
                        size={15}
                      />
                    </View>
                  </MenuView>
                ) : (
                  <Text testID="member-current-role" style={{ color: p.list.secondary }}>
                    {roleLabel(context.role)}
                  </Text>
                )}
                {context.state === 'Updating' ? (
                  <Text accessibilityLiveRegion="polite" style={{ color: p.list.secondary }}>
                    {t('community.updating')}
                  </Text>
                ) : null}
                <SettingsRows
                  testID="member-actions"
                  rows={[
                    {
                      id: 'member-open-profile',
                      title: t('community.profile'),
                      icon: { ios: 'person.crop.circle', android: 'person', web: 'person' },
                      onPress: () => {
                        if (!session.canOpenProfile(ticket)) return;
                        session.close(ticket);
                        router.push({ pathname: '/person/[id]', params: { id: member.id } });
                      },
                    },
                  ]}
                />
                {context.can_remove || context.state === 'Removing' ? (
                  <NativeButton
                    testID="member-remove"
                    destructive
                    disabled={state.busy || !context.can_remove}
                    label={t(
                      context.state === 'Removing' ? 'community.removing' : 'community.remove',
                    )}
                    onPress={() => {
                      void session.remove(
                        (review) =>
                          confirm({
                            title: t('community.removeNamed', {
                              name: review.member?.name ?? t('community.member'),
                            }),
                            message: t('community.removeFrom', { title: review.space_title }),
                            confirmLabel: t('community.remove'),
                            destructive: true,
                          }),
                        ticket,
                      );
                    }}
                  />
                ) : null}
                {state.actionFailed ? (
                  <Text accessibilityRole="alert" selectable style={{ color: p.destructive }}>
                    {t(
                      context.role_change_failed
                        ? 'community.roleRefused'
                        : context.removal_failed
                          ? 'community.refused'
                          : 'community.actionFailed',
                    )}
                  </Text>
                ) : null}
              </>
            ) : null}
          </ScrollView>
        </RNHostView>
      </BottomSheet>
    </Host>
  );
}
