import { useImperativeHandle, useRef, type Ref } from 'react';
import {
  EnrichedMarkdownTextInput,
  type EnrichedMarkdownTextInputInstance,
} from 'react-native-enriched-markdown';
import {
  StyleSheet,
  useWindowDimensions,
  type TextInputInstance,
  type TextInputProps,
  type TextInputChangeEvent,
  type TextStyle,
} from 'react-native';
import { t } from '../i18n';

/** One native editor owns visible text, selected UTF16 range and formatting.
 * The message writer receives only its canonical Markdown event receipt. */
export function ComposerRichInput({
  ref,
  composerBinding,
  composerEventCount = 0,
  composerRichText = true,
  onChange,
  onChangeText,
  onSelectionChange,
  onFocus,
  onBlur,
  onKeyPress,
  onSubmitEditing,
  style,
  value,
  ...props
}: TextInputProps & {
  ref?: Ref<TextInputInstance>;
  composerBinding?: string;
  composerEventCount?: number;
  composerRichText?: boolean;
}) {
  const editor = useRef<EnrichedMarkdownTextInputInstance>(null);
  useImperativeHandle(ref, () => editor.current as unknown as TextInputInstance, []);
  const textStyle = StyleSheet.flatten(style) as TextStyle | undefined;
  const { fontScale } = useWindowDimensions();
  const scale =
    props.allowFontScaling === false
      ? 1
      : props.maxFontSizeMultiplier && props.maxFontSizeMultiplier > 0
        ? Math.min(fontScale, props.maxFontSizeMultiplier)
        : fontScale;
  const nativeStyle = {
    ...textStyle,
    fontSize: (textStyle?.fontSize ?? 17) * scale,
    ...(textStyle?.lineHeight ? { lineHeight: textStyle.lineHeight * scale } : {}),
  };
  return (
    <EnrichedMarkdownTextInput
      {...props}
      ref={editor}
      style={nativeStyle}
      composerBinding={composerBinding}
      composerValue={value ?? ''}
      composerMostRecentEventCount={composerEventCount}
      composerRichText={composerRichText}
      composerSubmitOnReturn={props.submitBehavior === 'submit'}
      composerKeyboardAppearance={props.keyboardAppearance}
      composerCodeLabel={t('composerKeyboard.code')}
      composerQuoteLabel={t('composerKeyboard.quote')}
      composerLinkTitle={t('composerKeyboard.link')}
      composerLinkConfirm={t('common.oK')}
      composerCancelLabel={t('common.cancel')}
      selectionMenuConfig={{
        format: { enabled: composerRichText, label: t('composerKeyboard.format') },
        copyAsMarkdown: { enabled: false },
      }}
      formatMenuConfig={{
        bold: { label: t('documents.bold') },
        italic: { label: t('documents.italic') },
        underline: { enabled: false },
        strikethrough: { label: t('documents.strikethrough') },
        spoiler: { enabled: false },
        link: { label: t('composerKeyboard.link') },
      }}
      onComposerChange={(event) => {
        // Preserve the real native event for Reanimated and the existing receipt fence.
        onChange?.(event as unknown as TextInputChangeEvent);
        onChangeText?.(event.nativeEvent.text);
      }}
      onChangeSelection={({ start, end }) =>
        onSelectionChange?.({ nativeEvent: { selection: { start, end } } } as never)
      }
      onFocus={() => onFocus?.({ nativeEvent: {} } as never)}
      onBlur={() => onBlur?.({ nativeEvent: {} } as never)}
      onKeyPress={(event) => {
        onKeyPress?.({
          ...event,
          nativeEvent: { ...event.nativeEvent, eventCount: composerEventCount },
        });
        if (event.nativeEvent.key === 'Enter' && props.submitBehavior === 'submit')
          onSubmitEditing?.({ nativeEvent: { text: value ?? '' } } as never);
      }}
    />
  );
}
