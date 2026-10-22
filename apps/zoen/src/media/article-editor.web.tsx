import { useImperativeHandle, useRef, type Ref } from 'react';
import { TextInput } from 'react-native';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { articleText } from './attachment-content';
import type { ArticleEditorHandle } from './article-editor.types';
export function ArticleEditor({
  initial,
  disabled,
  onChange,
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
  const value = useRef(initial);
  useImperativeHandle(ref, () => ({ capture: async () => articleText(value.current) }), []);
  return (
    <TextInput
      defaultValue={initial}
      multiline
      editable={!disabled}
      accessibilityLabel={t('attachmentTabs.source')}
      onChangeText={(text) => {
        value.current = text;
        onChange(text);
      }}
      style={{ minHeight: 320, color: colors.text, fontSize: 17, fontFamily: 'monospace' }}
    />
  );
}
