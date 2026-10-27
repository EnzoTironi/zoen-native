import { useRef, useState } from 'react';
import { Modal, ScrollView, Text, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import type { ReplySelection } from '../core/generated/ReplySelection';
import type { TimelineEntry } from '../core/types';
import { messageText } from '../core/presentation';
import { selectedMessageQuote } from '../core/selected-message-quote';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { MessageMarkdown } from './message-markdown';
import { NativeButton } from './primitives';

/** Uses the installed native viewer's real selection menu and mapped source spans. */
export function MessageQuoteSelection({
  entry,
  current,
  onClose,
  onQuote,
  review,
}: {
  entry: TimelineEntry;
  current: () => boolean;
  onClose: () => void;
  onQuote: (selection: ReplySelection) => void;
  review: (selection: ReplySelection) => Promise<ReplySelection>;
}) {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const [visible, setVisible] = useState(true);
  const [unsupported, setUnsupported] = useState(false);
  const pending = useRef<ReplySelection | null>(null);
  const attempt = useRef(0);
  const reviewing = useRef(false);
  function cancel() {
    ++attempt.current;
    pending.current = null;
    setVisible(false);
  }
  return (
    <Modal
      visible={visible}
      animationType="slide"
      presentationStyle="pageSheet"
      onRequestClose={cancel}
      onDismiss={() => {
        const selection = pending.current;
        pending.current = null;
        if (selection && current()) onQuote(selection);
        onClose();
      }}
    >
      <View
        style={{
          flex: 1,
          backgroundColor: colors.background,
          paddingTop: insets.top,
        }}
      >
        <View
          style={{
            flexDirection: 'row',
            alignItems: 'center',
            padding: 16,
            gap: 12,
          }}
        >
          <Text
            accessibilityRole="header"
            style={{
              flex: 1,
              color: colors.text,
              fontSize: 20,
              fontWeight: '600',
            }}
          >
            {t('chat.selectText')}
          </Text>
          <NativeButton label={t('chat.cancel')} secondary onPress={cancel} />
        </View>
        {unsupported ? (
          <Text
            accessibilityRole="alert"
            style={{
              color: colors.secondary,
              paddingHorizontal: 20,
              paddingBottom: 12,
            }}
          >
            {t('chat.quoteSelectionUnavailable')}
          </Text>
        ) : null}
        <ScrollView
          contentContainerStyle={{
            paddingHorizontal: 20,
            paddingBottom: insets.bottom + 24,
          }}
        >
          <MessageMarkdown
            text={messageText(entry)}
            color={colors.text}
            fontSize={17}
            maxWidth={Math.max(1, width - 40)}
            selectable
            testID="quote-selection-text"
            onQuoteSelection={async (text, ranges) => {
              if (!current() || pending.current || reviewing.current) return;
              const selection = selectedMessageQuote(entry, text, ranges);
              if (!selection) {
                setUnsupported(true);
                return;
              }
              const request = ++attempt.current;
              reviewing.current = true;
              try {
                const canonical = await review(selection);
                if (request !== attempt.current || !current()) return;
                pending.current = canonical;
                setVisible(false);
              } catch {
                if (request === attempt.current && current()) setUnsupported(true);
              } finally {
                if (request === attempt.current) reviewing.current = false;
              }
            }}
          />
        </ScrollView>
      </View>
    </Modal>
  );
}
