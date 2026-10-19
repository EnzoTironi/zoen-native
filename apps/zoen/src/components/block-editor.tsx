import { t } from '../i18n/index.ts';
import { useRef } from 'react';
import { TextInput, View } from 'react-native';
import {
  EnrichedMarkdownTextInput,
  type EnrichedMarkdownTextInputInstance,
} from 'react-native-enriched-markdown';
import { useTheme } from '../theme';
import { IconButton } from './primitives';

export function BlockEditor({
  initial,
  onChange,
  disabled,
  sourceOnly = false,
}: {
  initial: string;
  onChange: (markdown: string) => void;
  disabled: boolean;
  sourceOnly?: boolean;
}) {
  const input = useRef<EnrichedMarkdownTextInputInstance>(null);
  const { colors } = useTheme();
  if (sourceOnly)
    return (
      <TextInput
        defaultValue={initial}
        multiline
        autoFocus
        editable={!disabled}
        onChangeText={onChange}
        accessibilityLabel={t('documents.blockContentInMarkdown')}
        style={{
          minHeight: 230,
          padding: 18,
          color: colors.text,
          backgroundColor: colors.surface,
          borderRadius: 18,
          fontFamily: process.env.EXPO_OS === 'ios' ? 'Menlo' : 'monospace',
          fontSize: 17,
          lineHeight: 27,
          textAlignVertical: 'top',
        }}
      />
    );
  return (
    <View style={{ gap: 12 }}>
      <View style={{ flexDirection: 'row', gap: 6, flexWrap: 'wrap' }}>
        <IconButton
          label={t('documents.bold')}
          name={{ ios: 'bold', android: 'format_bold', web: 'format_bold' }}
          onPress={() => input.current?.toggleBold()}
          disabled={disabled}
        />
        <IconButton
          label={t('documents.italic')}
          name={{ ios: 'italic', android: 'format_italic', web: 'format_italic' }}
          onPress={() => input.current?.toggleItalic()}
          disabled={disabled}
        />
        <IconButton
          label={t('documents.strikethrough')}
          name={{
            ios: 'strikethrough',
            android: 'format_strikethrough',
            web: 'format_strikethrough',
          }}
          onPress={() => input.current?.toggleStrikethrough()}
          disabled={disabled}
        />
        <IconButton
          label={t('documents.title')}
          name={{ ios: 'textformat.size', android: 'title', web: 'title' }}
          onPress={() => input.current?.toggleHeading(2)}
          disabled={disabled}
        />
        <IconButton
          label={t('documents.list')}
          name={{
            ios: 'list.bullet',
            android: 'format_list_bulleted',
            web: 'format_list_bulleted',
          }}
          onPress={() => input.current?.toggleUnorderedList()}
          disabled={disabled}
        />
      </View>
      <EnrichedMarkdownTextInput
        ref={input}
        defaultValue={initial}
        onChangeMarkdown={onChange}
        editable={!disabled}
        multiline
        autoFocus
        markdownShortcuts
        formatMenuConfig={{ underline: { enabled: false }, spoiler: { enabled: false } }}
        placeholder={t('documents.writeYourIdea')}
        placeholderTextColor={colors.tertiary}
        style={{
          minHeight: 230,
          padding: 16,
          color: colors.text,
          backgroundColor: colors.surface,
          borderRadius: 18,
          fontSize: 18,
          lineHeight: 27,
        }}
        markdownStyle={{ link: { color: colors.accent } }}
      />
    </View>
  );
}
