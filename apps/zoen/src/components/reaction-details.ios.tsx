import { Host } from '@expo/ui';
import {
  BottomSheet,
  Button,
  Group,
  HStack,
  List,
  RNHostView,
  ScrollView,
  Spacer,
  Text,
  TextField,
  VStack,
} from '@expo/ui/swift-ui';
import {
  accessibilityAddTraits,
  accessibilityLabel,
  autocorrectionDisabled,
  background,
  buttonStyle,
  contentShape,
  disabled,
  font,
  foregroundStyle,
  frame,
  labelStyle,
  listRowBackground,
  listStyle,
  onSubmit,
  padding,
  presentationBackground,
  presentationDetents,
  presentationDragIndicator,
  scrollContentBackground,
  shapes,
  submitLabel,
  textFieldStyle,
  textInputAutocapitalization,
  tint,
} from '@expo/ui/swift-ui/modifiers';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AccessibilityInfo } from 'react-native';
import * as Haptics from 'expo-haptics';
import type { Persona } from '../core/generated/Persona';
import { normalizeReactionEmoji } from '../core/reaction-emoji';
import { t } from '../i18n';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { MORE_REACTIONS, QUICK_REACTIONS } from './reaction-palette';
import type { ReactionDetailsProps } from './reaction-details.types';

const personKey = (person: Persona) => person.id;
const choices = [...new Set<string>([...QUICK_REACTIONS, ...MORE_REACTIONS])];

/** Participant details and arbitrary emoji entry share one native presentation. */
export function ReactionDetails(props: ReactionDetailsProps) {
  const { colors, scheme, reducedMotion } = useTheme();
  const [presented, setPresented] = useState(true);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const entry = props.entry && !props.entry.deleted ? props.entry : null;
  const error = entry ? props.error : null;
  useEffect(() => {
    if (error) AccessibilityInfo.announceForAccessibility(error);
  }, [error]);
  const reaction =
    entry?.reactions.find((value) => value.emoji === props.selectedEmoji) ?? entry?.reactions[0];
  const mine = entry?.reactions.find((value) => value.mine);
  const react = (emoji: string | null) => {
    if (!entry || props.pending) return;
    if (!reducedMotion) void Haptics.selectionAsync().catch(() => {});
    props.onReact(emoji);
  };
  const person = useCallback(
    ({ item }: { item: Persona }) => (
      <HStack
        testID={`reaction-person-${item.id}`}
        spacing={12}
        modifiers={[frame({ minHeight: 52 }), listRowBackground(colors.surface)]}
      >
        <RNHostView matchContents>
          <ProfileAvatar
            person={item}
            name={item.name}
            color={item.tint_hex}
            agent={item.kind === 'Agent'}
            size={36}
          />
        </RNHostView>
        <Text modifiers={[font({ textStyle: 'body' }), foregroundStyle(colors.text)]}>
          {item.name}
        </Text>
        <Spacer />
        {item.is_me ? (
          <Text modifiers={[font({ textStyle: 'caption' }), foregroundStyle(colors.secondary)]}>
            {t('messageReactions.you')}
          </Text>
        ) : null}
      </HStack>
    ),
    [colors.surface, colors.text, colors.secondary],
  );
  const actionModifiers = [
    buttonStyle('bordered'),
    frame({ minHeight: 44, maxWidth: Infinity }),
    disabled(Boolean(props.pending)),
    tint(colors.accent),
  ];
  return (
    <Host
      testID="reaction-details-host"
      style={{ position: 'absolute', width: 1, height: 1 }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <BottomSheet
        testID="reaction-details-sheet"
        isPresented={Boolean(entry) && presented}
        onIsPresentedChange={setPresented}
        onDismiss={() => {
          if (mounted.current) props.onClose();
        }}
      >
        <Group
          modifiers={[
            presentationDetents(['medium', 'large']),
            presentationDragIndicator('visible'),
            presentationBackground(colors.surface),
          ]}
        >
          {entry ? (
            <VStack spacing={12}>
              <HStack spacing={8} modifiers={[padding({ horizontal: 20, top: 16 })]}>
                {props.mode === 'emoji' && props.canGoBack ? (
                  <Button
                    label={t('messageReactions.back')}
                    systemImage="chevron.left"
                    onPress={props.onBack}
                    modifiers={[
                      buttonStyle('borderless'),
                      labelStyle('iconOnly'),
                      frame({ minWidth: 44, minHeight: 44 }),
                      disabled(Boolean(props.pending)),
                    ]}
                  />
                ) : null}
                <Text
                  modifiers={[
                    font({ textStyle: 'headline' }),
                    foregroundStyle(colors.text),
                    accessibilityAddTraits(['isHeader']),
                  ]}
                >
                  {t(
                    props.mode === 'emoji'
                      ? 'messageReactions.chooseEmoji'
                      : 'messageReactions.title',
                  )}
                </Text>
                <Spacer />
                <Button
                  label={t('messageReactions.close')}
                  systemImage="xmark"
                  onPress={() => setPresented(false)}
                  modifiers={[
                    buttonStyle('borderless'),
                    labelStyle('iconOnly'),
                    frame({ minWidth: 44, minHeight: 44 }),
                    foregroundStyle(colors.secondary),
                  ]}
                />
              </HStack>
              {props.error ? (
                <Text
                  testID="reaction-details-error"
                  modifiers={[foregroundStyle(colors.danger), padding({ horizontal: 20 })]}
                >
                  {props.error}
                </Text>
              ) : null}
              {props.pending ? (
                <Text
                  modifiers={[font({ textStyle: 'caption' }), foregroundStyle(colors.secondary)]}
                >
                  {t('messageReactions.changing')}
                </Text>
              ) : null}
              {props.mode === 'emoji' ? (
                <EmojiEntry {...props} onReact={react} />
              ) : (
                <>
                  <ScrollView
                    axes="horizontal"
                    showsIndicators={false}
                    modifiers={[frame({ height: 52 })]}
                  >
                    <HStack spacing={8} modifiers={[padding({ horizontal: 20 })]}>
                      {entry.reactions.map((value) => (
                        <Button
                          key={value.emoji}
                          testID={`reaction-tab-${value.emoji}`}
                          label={`${value.emoji} ${value.people.length}`}
                          onPress={() => props.onSelectEmoji(value.emoji)}
                          modifiers={[
                            buttonStyle(
                              value.emoji === reaction?.emoji ? 'borderedProminent' : 'bordered',
                            ),
                            tint(colors.accent),
                            frame({ minWidth: 52, minHeight: 44 }),
                            accessibilityLabel(
                              t('messageReactions.participants', {
                                emoji: value.emoji,
                                count: value.people.length,
                              }),
                            ),
                            accessibilityAddTraits(
                              value.emoji === reaction?.emoji ? ['isSelected'] : [],
                            ),
                            disabled(Boolean(props.pending)),
                          ]}
                        />
                      ))}
                    </HStack>
                  </ScrollView>
                  {reaction ? (
                    <List modifiers={[listStyle('plain'), scrollContentBackground('hidden')]}>
                      <List.ForEach data={reaction.people} keyExtractor={personKey}>
                        {person}
                      </List.ForEach>
                    </List>
                  ) : (
                    <Text modifiers={[foregroundStyle(colors.secondary)]}>
                      {t('messageReactions.empty')}
                    </Text>
                  )}
                  <VStack spacing={8} modifiers={[padding({ horizontal: 20, bottom: 16 })]}>
                    {reaction ? (
                      <Button
                        testID="reaction-selected-action"
                        label={
                          reaction.mine
                            ? t('messageReactions.removeMine')
                            : t('messageReactions.addReaction', { emoji: reaction.emoji })
                        }
                        onPress={() => react(reaction.mine ? null : reaction.emoji)}
                        modifiers={actionModifiers}
                      />
                    ) : null}
                    <Button
                      testID="reaction-choose-emoji"
                      label={t('messageReactions.chooseAnother')}
                      onPress={props.onChooseEmoji}
                      modifiers={actionModifiers}
                    />
                    {mine && !reaction?.mine ? (
                      <Button
                        label={t('messageReactions.removeMine')}
                        onPress={() => react(null)}
                        modifiers={actionModifiers}
                      />
                    ) : null}
                  </VStack>
                </>
              )}
            </VStack>
          ) : null}
        </Group>
      </BottomSheet>
    </Host>
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
    <VStack spacing={12} modifiers={[padding({ horizontal: 20, bottom: 16 })]}>
      <TextField
        testID="reaction-emoji-input"
        autoFocus
        placeholder={t('messageReactions.emojiPlaceholder')}
        maxLength={64}
        onTextChange={setValue}
        modifiers={[
          textFieldStyle('plain'),
          font({ size: 22 }),
          foregroundStyle(colors.text),
          padding({ horizontal: 14, vertical: 12 }),
          frame({ minHeight: 48 }),
          background(colors.muted, shapes.roundedRectangle({ cornerRadius: 16 })),
          tint(colors.accent),
          autocorrectionDisabled(),
          textInputAutocapitalization('never'),
          submitLabel('send'),
          onSubmit(submit),
          disabled(Boolean(props.pending)),
        ]}
      >
        <TextField.Placeholder>
          <Text modifiers={[font({ textStyle: 'body' }), foregroundStyle(colors.secondary)]}>
            {t('messageReactions.emojiPlaceholder')}
          </Text>
        </TextField.Placeholder>
      </TextField>
      <Text
        modifiers={[
          font({ textStyle: 'footnote' }),
          foregroundStyle(value.trim() && !emoji ? colors.danger : colors.secondary),
        ]}
      >
        {t(value.trim() && !emoji ? 'messageReactions.emojiInvalid' : 'messageReactions.emojiHint')}
      </Text>
      <Button
        testID="reaction-emoji-submit"
        label={
          emoji && emoji === mine ? t('messageReactions.removeMine') : t('messageReactions.react')
        }
        onPress={submit}
        modifiers={[
          buttonStyle('borderedProminent'),
          tint(colors.accent),
          foregroundStyle(colors.onAccent),
          frame({ minHeight: 44, maxWidth: Infinity }),
          disabled(!emoji || Boolean(props.pending)),
        ]}
      />
      <ScrollView>
        <VStack spacing={4}>
          {Array.from({ length: Math.ceil(choices.length / 5) }, (_, index) => (
            <HStack key={index} spacing={4}>
              {choices.slice(index * 5, index * 5 + 5).map((choice) => (
                <Button
                  key={choice}
                  label={choice}
                  onPress={() => {
                    if (!props.pending) props.onReact(choice === mine ? null : choice);
                  }}
                  modifiers={[
                    buttonStyle(choice === mine ? 'bordered' : 'borderless'),
                    font({ size: 27 }),
                    frame({ minWidth: 44, minHeight: 44 }),
                    contentShape(shapes.rectangle()),
                    disabled(Boolean(props.pending)),
                    accessibilityLabel(
                      t(choice === mine ? 'chat.removeReaction' : 'chat.reactWithEmoji', {
                        emoji: choice,
                      }),
                    ),
                  ]}
                />
              ))}
            </HStack>
          ))}
        </VStack>
      </ScrollView>
    </VStack>
  );
}
