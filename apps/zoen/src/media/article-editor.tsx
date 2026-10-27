import { useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react';
import { ScrollView, Text, TextInput, View } from 'react-native';
import type { EnrichedMarkdownTextInputInstance } from 'react-native-enriched-markdown';
import { ArticleNativeInput } from './article-native-input';
import { Field, IconButton, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { articleText } from './attachment-content';
import type { ArticleEditorHandle } from './article-editor.types';

type LinkTarget = {
  editor: EnrichedMarkdownTextInputInstance;
  range: { start: number; end: number };
  current(): boolean;
  markdown: string | null;
  applying: boolean;
};
const linkUrl = (value: string) => /^(https?:\/\/|mailto:)[^\s\0]{1,2048}$/i.test(value);

/** The installed native editor owns selection and formatting; Markdown is its lossless transport. */
export function ArticleEditor({
  initial,
  disabled,
  source,
  onChange,
  canEdit,
  captureEditLease,
  ref,
}: {
  initial: string;
  disabled: boolean;
  source: boolean;
  onChange(text: string): void;
  canEdit(): boolean;
  captureEditLease(): () => boolean;
  ref?: Ref<ArticleEditorHandle>;
}) {
  const { colors } = useTheme();
  const input = useRef<EnrichedMarkdownTextInputInstance>(null);
  const raw = useRef(initial);
  const [link, setLink] = useState<string | null>(null);
  const [selection, setSelection] = useState({ start: 0, end: 0 });
  const range = useRef(selection);
  const target = useRef<LinkTarget | null>(null);
  const mounted = useRef(true);
  const refocus = useRef<LinkTarget | null>(null);
  const [linkBusy, setLinkBusy] = useState(false);
  const [linkError, setLinkError] = useState<string | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      target.current = null;
      refocus.current = null;
    };
  }, []);
  useEffect(() => {
    // Restore keyboard only after the URL form has released focus and the body is editable again.
    if (link !== null) return;
    const value = refocus.current;
    refocus.current = null;
    if (value && mounted.current && input.current === value.editor && value.current() && canEdit())
      value.editor.focus();
  }, [link, canEdit]);
  function currentLink(value: LinkTarget | null): value is LinkTarget {
    return Boolean(
      value &&
      mounted.current &&
      target.current === value &&
      input.current === value.editor &&
      value.current() &&
      canEdit(),
    );
  }
  function finishLink(value: LinkTarget | null) {
    if (target.current !== value) return;
    target.current = null;
    if (mounted.current) {
      setLink(null);
      setLinkBusy(false);
      setLinkError(null);
    }
  }
  async function openLink() {
    const editor = input.current;
    const selected = range.current;
    if (disabled || !canEdit() || !editor || target.current || selected.start === selected.end)
      return;
    const value: LinkTarget = {
      editor,
      range: selected,
      current: captureEditLease(),
      markdown: null,
      applying: false,
    };
    target.current = value;
    setLinkBusy(true);
    setLinkError(null);
    try {
      const markdown = articleText(await editor.getMarkdown());
      if (
        !currentLink(value) ||
        range.current.start !== selected.start ||
        range.current.end !== selected.end
      ) {
        finishLink(value);
        return;
      }
      value.markdown = markdown;
      setLink('');
    } catch {
      if (currentLink(value)) {
        finishLink(value);
        setLinkError(t('attachmentTabs.failed'));
      } else finishLink(value);
    } finally {
      if (mounted.current && target.current === value) setLinkBusy(false);
    }
  }
  async function applyLink(value: LinkTarget | null, url: string) {
    if (!currentLink(value)) {
      finishLink(value);
      return;
    }
    if (disabled || value.applying || value.markdown === null || !linkUrl(url)) return;
    value.applying = true;
    setLinkBusy(true);
    try {
      const markdown = articleText(await value.editor.getMarkdown());
      if (!currentLink(value)) {
        finishLink(value);
        return;
      }
      if (markdown !== value.markdown) throw new Error('changed-article');
      // Consume before native commands: retained/double-tap callbacks cannot apply twice.
      finishLink(value);
      value.editor.setSelection(value.range.start, value.range.end);
      value.editor.setLink(url);
      refocus.current = value;
    } catch {
      if (currentLink(value)) {
        finishLink(value);
        setLinkError(t('attachmentTabs.failed'));
      } else finishLink(value);
    }
  }
  const shownLink = target.current;
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
        void openLink();
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
            disabled={
              disabled ||
              linkBusy ||
              link !== null ||
              (tool.action === 'link' && selection.start === selection.end)
            }
            onPress={() => {
              if (canEdit() && !target.current) format(tool.action);
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
              if (currentLink(shownLink) && !shownLink.applying) setLink(value);
            }}
            autoCapitalize="none"
            autoCorrect={false}
            editable={!disabled && !linkBusy}
            autoFocus
          />
          <NativeButton
            label={t('attachmentTabs.applyLink')}
            disabled={disabled || linkBusy || !linkUrl(link)}
            onPress={() => {
              void applyLink(shownLink, link);
            }}
          />
          <NativeButton
            secondary
            label={t('common.cancel')}
            disabled={disabled}
            onPress={() => {
              const admitted = currentLink(shownLink);
              finishLink(shownLink);
              if (admitted) refocus.current = shownLink;
            }}
          />
        </View>
      ) : null}
      {linkError ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {linkError}
        </Text>
      ) : null}
      <ArticleNativeInput
        ref={input}
        defaultValue={initial}
        editable={!disabled && link === null && !linkBusy}
        multiline
        autoFocus
        markdownShortcuts
        onFocus={() => {
          // Returning to the body abandons the URL form, not the document.
          if (target.current && target.current.markdown !== null) finishLink(target.current);
        }}
        onChangeSelection={(value) => {
          if (
            !canEdit() ||
            !Number.isSafeInteger(value.start) ||
            !Number.isSafeInteger(value.end) ||
            value.start < 0 ||
            value.end < value.start
          )
            return;
          range.current = value;
          setSelection(value);
        }}
        onChangeMarkdown={(text) => {
          if (!canEdit()) return;
          finishLink(target.current);
          onChange(text);
        }}
        accessibilityLabel={t('attachmentTabs.body')}
        placeholder={t('attachmentTabs.body')}
        placeholderTextColor={colors.tertiary}
        style={{ minHeight: 320, color: colors.text, fontSize: 17, lineHeight: 24 }}
        markdownStyle={{ link: { color: colors.accent } }}
      />
    </View>
  );
}
