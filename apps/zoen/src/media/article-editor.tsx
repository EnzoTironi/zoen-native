import { useImperativeHandle, useRef, useState, type Ref } from 'react';
import { ScrollView, TextInput, View } from 'react-native';
import {
  EnrichedMarkdownTextInput,
  type EnrichedMarkdownTextInputInstance,
} from 'react-native-enriched-markdown';
import { Field, IconButton, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { articleText } from './attachment-content';
import type { ArticleEditorHandle } from './article-editor.types';

/** The installed native editor owns selection and formatting; Markdown is its lossless transport. */
export function ArticleEditor({
  initial,
  disabled,
  source,
  onChange,
  canEdit,
  ref,
}: {
  initial: string;
  disabled: boolean;
  source: boolean;
  onChange(text: string): void;
  canEdit(): boolean;
  ref?: Ref<ArticleEditorHandle>;
}) {
  const { colors } = useTheme();
  const input = useRef<EnrichedMarkdownTextInputInstance>(null);
  const raw = useRef(initial);
  const [link, setLink] = useState<string | null>(null);
  useImperativeHandle(
    ref,
    () => ({
      capture: async () => articleText(source ? raw.current : await input.current!.getMarkdown()),
    }),
    [source],
  );
  if (source)
    return (
      <TextInput
        testID="article-markdown"
        defaultValue={initial}
        multiline
        autoFocus
        editable={!disabled}
        accessibilityLabel={t('attachmentTabs.source')}
        onChangeText={(text) => {
          raw.current = text;
          onChange(text);
        }}
        style={{
          minHeight: 320,
          color: colors.text,
          fontFamily: 'Menlo',
          fontSize: 17,
          lineHeight: 24,
        }}
      />
    );
  const tools = [
    { label: t('documents.bold'), icon: 'bold', action: 'bold' },
    { label: t('documents.italic'), icon: 'italic', action: 'italic' },
    {
      label: t('documents.strikethrough'),
      icon: 'strikethrough',
      action: 'strike',
    },
    {
      label: t('attachmentTabs.heading'),
      icon: 'textformat.size',
      action: 'heading',
    },
    {
      label: t('documents.list'),
      icon: 'list.bullet',
      action: 'bullet',
    },
    {
      label: t('attachmentTabs.numbered'),
      icon: 'list.number',
      action: 'numbered',
    },
    { label: t('attachmentTabs.link'), icon: 'link', action: 'link' },
  ] as const;
  function format(action: (typeof tools)[number]['action']) {
    const editor = input.current;
    switch (action) {
      case 'bold':
        editor?.toggleBold();
        break;
      case 'italic':
        editor?.toggleItalic();
        break;
      case 'strike':
        editor?.toggleStrikethrough();
        break;
      case 'heading':
        editor?.toggleHeading(2);
        break;
      case 'bullet':
        editor?.toggleUnorderedList();
        break;
      case 'numbered':
        editor?.toggleOrderedList();
        break;
      case 'link':
        setLink('');
        break;
    }
  }
  return (
    <View style={{ gap: 12 }}>
      <ScrollView
        horizontal
        keyboardShouldPersistTaps="handled"
        showsHorizontalScrollIndicator={false}
        contentContainerStyle={{ gap: 4 }}
      >
        {tools.map((tool) => (
          <IconButton
            key={tool.icon}
            label={tool.label}
            name={{ ios: tool.icon, android: 'edit', web: 'edit' }}
            disabled={disabled}
            onPress={() => {
              if (canEdit()) format(tool.action);
            }}
          />
        ))}
      </ScrollView>
      {link !== null ? (
        <View style={{ gap: 8 }}>
          <Field
            label={t('attachmentTabs.linkUrl')}
            value={link}
            onChangeText={(value) => {
              if (canEdit()) setLink(value);
            }}
            autoCapitalize="none"
            autoCorrect={false}
            editable={!disabled}
          />
          <NativeButton
            label={t('attachmentTabs.applyLink')}
            disabled={disabled || !/^(https?:\/\/|mailto:)[^\s\0]{1,2048}$/i.test(link)}
            onPress={() => {
              if (!disabled && canEdit() && /^(https?:\/\/|mailto:)[^\s\0]{1,2048}$/i.test(link)) {
                input.current?.setLink(link);
                setLink(null);
              }
            }}
          />
          <NativeButton
            secondary
            label={t('common.cancel')}
            disabled={disabled}
            onPress={() => {
              if (canEdit()) setLink(null);
            }}
          />
        </View>
      ) : null}
      <EnrichedMarkdownTextInput
        ref={input}
        defaultValue={initial}
        editable={!disabled}
        multiline
        autoFocus
        markdownShortcuts
        onChangeMarkdown={onChange}
        accessibilityLabel={t('attachmentTabs.body')}
        placeholder={t('attachmentTabs.body')}
        placeholderTextColor={colors.tertiary}
        style={{ minHeight: 320, color: colors.text, fontSize: 17, lineHeight: 24 }}
        markdownStyle={{ link: { color: colors.accent } }}
      />
    </View>
  );
}
