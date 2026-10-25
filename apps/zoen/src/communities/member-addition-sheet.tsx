import { BottomSheet, Host, RNHostView } from '@expo/ui';
import { FlatList, Pressable, Text, View } from 'react-native';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { useDialogs } from '../components/dialogs';
import { Field, IconButton, Loading, NativeButton } from '../components/primitives';
import { ProfileAvatar } from '../profile/profile-avatar';
import type { MemberAdditionSession, MemberAdditionState } from './member-addition';

export function MemberAdditionSheet({
  session,
  state,
}: {
  session: MemberAdditionSession;
  state: MemberAdditionState;
}) {
  const { presentation: p, scheme } = useTheme();
  const { confirm } = useDialogs();
  const ticket = session.ticket(state);
  const selected = state.selected;
  return (
    <Host
      style={{ position: 'absolute' }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={p.list.accent}
    >
      <BottomSheet
        testID="member-addition-sheet"
        isPresented={state.open}
        onDismiss={() => session.close(ticket)}
        snapPoints={['full']}
        containerColor={p.list.background}
      >
        <RNHostView>
          <View style={{ flex: 1, gap: 12, paddingTop: 16, paddingBottom: 24 }}>
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
              <Text
                accessibilityRole="header"
                style={{ ...p.typography.body, color: p.list.text, flex: 1 }}
              >
                {t('community.add')}
              </Text>
              <IconButton
                name={{ ios: 'xmark', android: 'close', web: 'close' }}
                label={t('common.close')}
                onPress={() => session.close(ticket)}
              />
            </View>
            <Field
              label={t('chat.searchPeople')}
              placeholder={t('chat.nameOr')}
              value={state.query}
              onChangeText={(query) => session.search(query, ticket)}
              editable={!state.busy && state.canManage}
              autoCapitalize="none"
              autoCorrect={false}
            />
            {state.phase === 'loading' ? <Loading /> : null}
            {!state.canManage ? (
              <Text accessibilityRole="alert" style={{ color: p.destructive }}>
                {t('community.addUnavailable')}
              </Text>
            ) : null}
            {state.phase === 'failed' ? (
              <NativeButton
                label={t('chat.tryAgain2')}
                onPress={() => {
                  void session.refresh();
                }}
              />
            ) : null}
            {state.phase === 'failed' || state.failed || selected?.addition_failed ? (
              <Text accessibilityRole="alert" style={{ color: p.destructive }}>
                {t(
                  selected?.addition_failed
                    ? 'community.addRefused'
                    : state.failed
                      ? 'community.addFailed'
                      : 'community.searchFailed',
                )}
              </Text>
            ) : null}
            {selected &&
            !selected.addition_failed &&
            (selected.state === 'Updating' || selected.state === 'Active') ? (
              <Text accessibilityLiveRegion="polite" style={{ color: p.list.secondary }}>
                {t(selected.state === 'Active' ? 'community.added' : 'community.adding')}
              </Text>
            ) : null}
            {state.phase === 'ready' && state.canManage && state.results.length === 0 ? (
              <Text style={{ ...p.typography.body, color: p.list.secondary }}>
                {t('chat.noResults')}
              </Text>
            ) : null}
            <FlatList
              data={state.results}
              keyExtractor={(item) => item.member_id}
              keyboardShouldPersistTaps="handled"
              renderItem={({ item }) => {
                const person = item.member ?? item.candidate;
                if (!person) return null;
                return (
                  <Pressable
                    testID={`member-add-${person.id}`}
                    accessibilityRole="button"
                    disabled={state.busy || !state.canManage || !item.can_add}
                    accessibilityState={{
                      disabled: state.busy || !state.canManage || !item.can_add,
                    }}
                    onPress={() => {
                      void session.select(
                        person.id,
                        (review) =>
                          confirm({
                            title: t('community.addNamed', {
                              name: review.candidate?.name ?? t('community.member'),
                            }),
                            message: t('community.addTo', { title: review.space_title }),
                            confirmLabel: t('community.add'),
                          }),
                        ticket,
                      );
                    }}
                    style={({ pressed }) => ({
                      flexDirection: 'row',
                      alignItems: 'center',
                      gap: 13,
                      minHeight: 58,
                      paddingHorizontal: 16,
                      paddingVertical: 9,
                      backgroundColor: pressed ? p.list.highlighted : 'transparent',
                    })}
                  >
                    <ProfileAvatar
                      person={person}
                      name={person.name}
                      color={person.tint_hex}
                      agent={person.kind === 'Agent'}
                      size={38}
                    />
                    <View style={{ flex: 1, gap: 3 }}>
                      <Text style={{ ...p.typography.body, color: p.list.text }}>
                        {person.name}
                      </Text>
                      {person.handle ? (
                        <Text style={{ ...p.typography.footnote, color: p.list.secondary }}>
                          @{person.handle}
                        </Text>
                      ) : null}
                    </View>
                    {item.state === 'Active' ? (
                      <Text style={{ ...p.typography.footnote, color: p.list.secondary }}>
                        {t('community.alreadyMember')}
                      </Text>
                    ) : null}
                  </Pressable>
                );
              }}
            />
          </View>
        </RNHostView>
      </BottomSheet>
    </Host>
  );
}
