import { BottomSheet, Host, RNHostView } from '@expo/ui';
import { MenuView } from '@expo/ui/community/menu';
import type { MemberRoleOption } from '../core/generated/MemberRoleOption';
import { memberRoleLabelKey } from './member-role-label';
import { useEffect, useRef, useState } from 'react';
import type { SpaceMemberContext } from '../core/generated/SpaceMemberContext';
import { FlatList, Pressable, Text, View } from 'react-native';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { useDialogs } from '../components/dialogs';
import { Field, Icon, IconButton, Loading, NativeButton } from '../components/primitives';
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
  const ticket = session.ticket(state);
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
          <MemberAdditionContent session={session} state={state} />
        </RNHostView>
      </BottomSheet>
    </Host>
  );
}

/** The same selector can replace content inside an existing sheet without presenting another. */
export function MemberAdditionContent({
  session,
  state,
  onBack,
  onClose,
  title,
  inlineConfirmation = false,
}: {
  session: MemberAdditionSession;
  state: MemberAdditionState;
  onBack?: () => void;
  onClose?: () => void;
  title?: string;
  inlineConfirmation?: boolean;
}) {
  const { presentation: p, scheme } = useTheme();
  const { confirm } = useDialogs();
  const ticket = session.ticket(state);
  const selected = state.selected;
  type Review = {
    context: SpaceMemberContext;
    options: Parameters<typeof confirm>[0];
    scope: string;
    generation: number;
    token: object;
  };
  const [pendingReview, setReview] = useState<Review | null>(null);
  const answer = useRef<{ token: object; resolve(approved: boolean): void } | null>(null);
  const review =
    pendingReview?.scope === ticket?.scope && pendingReview?.generation === ticket?.generation
      ? pendingReview?.context
      : null;
  const finish = (approved: boolean) => {
    const current = answer.current;
    if (!current || current.token !== pendingReview?.token) return;
    answer.current = null;
    setReview(null);
    current.resolve(approved);
  };
  useEffect(
    () => () => {
      const current = answer.current;
      answer.current = null;
      current?.resolve(false);
    },
    [session, ticket?.scope, ticket?.generation],
  );
  const requestConfirmation = (
    context: SpaceMemberContext,
    options: Parameters<typeof confirm>[0],
  ) => {
    if (!inlineConfirmation) return confirm(options);
    if (!ticket) return Promise.resolve(false);
    return new Promise<boolean>((resolve) => {
      answer.current?.resolve(false);
      const token = {};
      answer.current = { token, resolve };
      setReview({ context, options, token, scope: ticket.scope, generation: ticket.generation });
    });
  };
  const reviewAddition = (context: SpaceMemberContext, option: MemberRoleOption) =>
    requestConfirmation(context, {
      title: t('community.addNamedRole', {
        name: context.candidate?.name ?? t('community.member'),
        role: t(memberRoleLabelKey(option.role)),
      }),
      message: t('community.addTo', { title: context.space_title }),
      confirmLabel: t('community.addAs', { role: t(memberRoleLabelKey(option.role)) }),
    });
  return (
    <View style={{ flex: 1, gap: 12, paddingTop: 16, paddingBottom: 24 }}>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
        {onBack ? (
          <IconButton
            name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
            label={t('common.back')}
            onPress={() => {
              if (review) finish(false);
              else onBack();
            }}
          />
        ) : null}
        <Text
          accessibilityRole="header"
          style={{ ...p.typography.body, color: p.list.text, flex: 1 }}
        >
          {title ?? t('community.add')}
        </Text>
        <IconButton
          name={{ ios: 'xmark', android: 'close', web: 'close' }}
          label={t('common.close')}
          onPress={onClose ?? (() => session.close(ticket))}
        />
      </View>
      {review ? (
        <View style={{ gap: 16 }}>
          <Text style={{ ...p.typography.body, color: p.list.text }}>
            {pendingReview?.options.title}
          </Text>
          <Text style={{ ...p.typography.body, color: p.list.secondary }}>
            {pendingReview?.options.message}
          </Text>
          <NativeButton
            testID="member-add-confirm"
            label={pendingReview?.options.confirmLabel ?? t('community.add')}
            disabled={!state.canManage || state.phase !== 'ready'}
            onPress={() => finish(true)}
          />
          <NativeButton
            testID="member-add-cancel"
            label={t('common.cancel')}
            secondary
            onPress={() => finish(false)}
          />
        </View>
      ) : (
        <>
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
              {t(
                selected.state === 'Active'
                  ? selected.delivery_ready
                    ? 'community.added'
                    : 'community.connecting'
                  : 'community.adding',
              )}
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
                <View style={{ flexDirection: 'row', alignItems: 'center' }}>
                  <Pressable
                    testID={`member-add-${person.id}`}
                    accessibilityRole="button"
                    disabled={state.busy || !state.canManage || !item.can_add}
                    accessibilityState={{
                      disabled: state.busy || !state.canManage || !item.can_add,
                    }}
                    onPress={() => {
                      void session.select(person.id, reviewAddition, ticket);
                    }}
                    style={({ pressed }) => ({
                      flex: 1,
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
                        {t(
                          item.delivery_ready ? 'community.alreadyMember' : 'community.connecting',
                        )}
                      </Text>
                    ) : null}
                  </Pressable>
                  {item.can_add && item.role_options.length ? (
                    <MenuView
                      testID={`member-add-role-${person.id}`}
                      colorScheme={scheme}
                      actions={['reader', 'member', 'admin'].flatMap((role) =>
                        item.role_options
                          .filter((option) => option.role === role)
                          .map((option) => ({
                            id: option.revision,
                            title: t('community.addAs', {
                              role: t(memberRoleLabelKey(option.role)),
                            }),
                            attributes: { disabled: state.busy || !state.canManage },
                          })),
                      )}
                      onPressAction={({ nativeEvent }) => {
                        const option = item.role_options.find(
                          (choice) => choice.revision === nativeEvent.event,
                        );
                        if (option) void session.select(person.id, reviewAddition, ticket, option);
                      }}
                    >
                      <View
                        accessible
                        accessibilityRole="button"
                        accessibilityLabel={t('community.chooseRole')}
                        style={{
                          minHeight: 44,
                          paddingHorizontal: 16,
                          flexDirection: 'row',
                          alignItems: 'center',
                          gap: 4,
                        }}
                      >
                        <Text style={{ color: p.list.accent }}>{t('community.member')}</Text>
                        <Icon
                          name={{
                            ios: 'chevron.down',
                            android: 'arrow_drop_down',
                            web: 'arrow_drop_down',
                          }}
                          size={13}
                          color={p.list.accent}
                        />
                      </View>
                    </MenuView>
                  ) : null}
                </View>
              );
            }}
          />
        </>
      )}
    </View>
  );
}
