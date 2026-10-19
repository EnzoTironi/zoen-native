import { BottomSheet, RNHostView } from '@expo/ui';
import { useState } from 'react';
import { FlatList, Pressable, ScrollView, Text, View } from 'react-native';
import { normalizeReactionEmoji } from '../core/reaction-emoji';
import { t } from '../i18n';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { Field, IconButton, NativeButton } from './primitives';
import { ReactionButton, MORE_REACTIONS, QUICK_REACTIONS } from './reaction-palette';
import type { ReactionDetailsProps } from './reaction-details.types';

/** Other platforms retain the same canonical selection and explicit mutation controls. */
export function ReactionDetails(props: ReactionDetailsProps) {
  const { colors } = useTheme();
  const entry = props.entry && !props.entry.deleted ? props.entry : null;
  const reaction =
    entry?.reactions.find((value) => value.emoji === props.selectedEmoji) ?? entry?.reactions[0];
  const mine = entry?.reactions.find((value) => value.mine);
  return (
    <BottomSheet
      testID="reaction-details-sheet"
      isPresented={Boolean(entry)}
      onDismiss={props.onClose}
      snapPoints={['half', 'full']}
      contentPadding={0}
      containerColor={colors.surface}
    >
      <RNHostView>
        <View style={{ flex: 1, padding: 20, gap: 12 }}>
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
            {props.mode === 'emoji' && props.canGoBack ? (
              <IconButton
                name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
                label={t('messageReactions.back')}
                disabled={props.pending}
                onPress={props.onBack}
              />
            ) : null}
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 22, fontWeight: '600', flex: 1 }}
            >
              {t(
                props.mode === 'emoji' ? 'messageReactions.chooseEmoji' : 'messageReactions.title',
              )}
            </Text>
            <IconButton
              name={{ ios: 'xmark', android: 'close', web: 'close' }}
              label={t('messageReactions.close')}
              onPress={props.onClose}
            />
          </View>
          {props.error ? (
            <Text accessibilityRole="alert" style={{ color: colors.danger }}>
              {props.error}
            </Text>
          ) : null}
          {props.pending ? (
            <Text style={{ color: colors.secondary }}>{t('messageReactions.changing')}</Text>
          ) : null}
          {props.mode === 'emoji' ? (
            <EmojiEntry {...props} />
          ) : (
            <>
              <ScrollView
                horizontal
                showsHorizontalScrollIndicator={false}
                style={{ maxHeight: 52 }}
                contentContainerStyle={{ gap: 8 }}
              >
                {entry?.reactions.map((value) => (
                  <Pressable
                    key={value.emoji}
                    accessibilityRole="button"
                    accessibilityState={{
                      selected: value.emoji === reaction?.emoji,
                      disabled: props.pending,
                    }}
                    accessibilityLabel={t('messageReactions.participants', {
                      emoji: value.emoji,
                      count: value.people.length,
                    })}
                    disabled={props.pending}
                    onPress={() => props.onSelectEmoji(value.emoji)}
                    style={{
                      minWidth: 52,
                      minHeight: 44,
                      paddingHorizontal: 12,
                      justifyContent: 'center',
                      alignItems: 'center',
                      borderRadius: 22,
                      backgroundColor:
                        value.emoji === reaction?.emoji ? colors.muted : 'transparent',
                    }}
                  >
                    <Text style={{ color: colors.text, fontSize: 17 }}>
                      {value.emoji} {value.people.length}
                    </Text>
                  </Pressable>
                ))}
              </ScrollView>
              <FlatList
                data={reaction?.people ?? []}
                keyExtractor={(person) => person.id}
                nestedScrollEnabled
                style={{ flex: 1 }}
                renderItem={({ item }) => (
                  <View
                    testID={`reaction-person-${item.id}`}
                    style={{ flexDirection: 'row', alignItems: 'center', minHeight: 56, gap: 12 }}
                  >
                    <ProfileAvatar
                      person={item}
                      name={item.name}
                      color={item.tint_hex}
                      agent={item.kind === 'Agent'}
                      size={36}
                    />
                    <Text style={{ color: colors.text, fontSize: 17, flex: 1 }}>{item.name}</Text>
                    {item.is_me ? (
                      <Text style={{ color: colors.secondary }}>{t('messageReactions.you')}</Text>
                    ) : null}
                  </View>
                )}
                ListEmptyComponent={
                  <Text style={{ color: colors.secondary }}>{t('messageReactions.empty')}</Text>
                }
              />
              {reaction ? (
                <NativeButton
                  secondary
                  label={
                    reaction.mine
                      ? t('messageReactions.removeMine')
                      : t('messageReactions.addReaction', { emoji: reaction.emoji })
                  }
                  disabled={props.pending}
                  onPress={() => props.onReact(reaction.mine ? null : reaction.emoji)}
                />
              ) : null}
              <NativeButton
                secondary
                label={t('messageReactions.chooseAnother')}
                disabled={props.pending}
                onPress={props.onChooseEmoji}
              />
              {mine && !reaction?.mine ? (
                <NativeButton
                  secondary
                  label={t('messageReactions.removeMine')}
                  disabled={props.pending}
                  onPress={() => props.onReact(null)}
                />
              ) : null}
            </>
          )}
        </View>
      </RNHostView>
    </BottomSheet>
  );
}

function EmojiEntry(props: ReactionDetailsProps) {
  const { colors } = useTheme();
  const [value, setValue] = useState('');
  const emoji = normalizeReactionEmoji(value);
  const mine = props.entry?.reactions.find((reaction) => reaction.mine)?.emoji;
  function submit() {
    const normalized = normalizeReactionEmoji(value);
    if (normalized && !props.pending) props.onReact(normalized === mine ? null : normalized);
  }
  return (
    <View style={{ flex: 1, gap: 12 }}>
      <Field
        label={t('messageReactions.chooseEmoji')}
        placeholder={t('messageReactions.emojiPlaceholder')}
        value={value}
        maxLength={64}
        onChangeText={setValue}
        autoCorrect={false}
        autoCapitalize="none"
        returnKeyType="send"
        onSubmitEditing={submit}
      />
      <Text
        accessibilityRole={value.trim() && !emoji ? 'alert' : 'text'}
        style={{ color: value.trim() && !emoji ? colors.danger : colors.secondary }}
      >
        {t(value.trim() && !emoji ? 'messageReactions.emojiInvalid' : 'messageReactions.emojiHint')}
      </Text>
      <NativeButton
        label={
          emoji && emoji === mine ? t('messageReactions.removeMine') : t('messageReactions.react')
        }
        disabled={!emoji || props.pending}
        onPress={submit}
      />
      <ScrollView
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={{
          flexDirection: 'row',
          flexWrap: 'wrap',
          justifyContent: 'center',
          gap: 4,
        }}
      >
        {[...new Set<string>([...QUICK_REACTIONS, ...MORE_REACTIONS])].map((choice) => (
          <ReactionButton
            key={choice}
            emoji={choice}
            mine={choice === mine}
            pending={Boolean(props.pending)}
            onReact={props.onReact}
          />
        ))}
      </ScrollView>
    </View>
  );
}
