import { ForwardOptionsMenu } from './forward-options-menu';
import { forwardHasCaption, DEFAULT_FORWARD_OPTIONS } from '../core/forward-options';
import type { ForwardOptions } from '../core/generated/ForwardOptions';
import { messageForwardLabel } from '../core/message-presentation';
import { ScrollView, StyleSheet, Text, View, useWindowDimensions } from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import type { TimelineEntry } from '../core/types';
import type { Persona } from '../core/generated/Persona';
import { t } from '../i18n/index.ts';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { NativeButton } from './primitives';
import { ForwardMessagePreview } from './forward-message-preview';

/** Compact share review: destination, original content and explicit native actions. */
export function ForwardReviewPanel({
  spaceId,
  entry,
  recipient,
  counterpart,
  sourceTitle,
  options = DEFAULT_FORWARD_OPTIONS,
  onOptions,
  accepted,
  busy,
  canSend,
  insets,
  isCurrent,
  onSend,
  onCancel,
}: {
  spaceId: string;
  entry: TimelineEntry;
  recipient: string;
  counterpart?: Persona | null;
  sourceTitle?: string;
  options?: ForwardOptions;
  onOptions?(options: ForwardOptions): void;
  accepted: boolean;
  busy: boolean;
  canSend: boolean;
  insets: { bottom: number; left?: number; right?: number };
  isCurrent(): boolean;
  onSend(): void;
  onCancel(): void;
}) {
  const { presentation: p } = useTheme();
  const { width, fontScale } = useWindowDimensions();
  const panelWidth = Math.min(560, width - 32 - (insets.left ?? 0) - (insets.right ?? 0));
  return (
    <View
      testID="forward-message-review"
      style={{ flex: 1, paddingBottom: Math.max(20, insets.bottom) }}
    >
      <KeyboardAvoidingView behavior="padding" automaticOffset style={{ flex: 1 }}>
        <View
          style={{
            flex: 1,
            justifyContent: 'center',
            paddingVertical: 16,
            paddingLeft: 16 + (insets.left ?? 0),
            paddingRight: 16 + (insets.right ?? 0),
          }}
        >
          <View
            style={{
              width: '100%',
              maxWidth: 560,
              maxHeight: '100%',
              flexShrink: 1,
              alignSelf: 'center',
              backgroundColor: p.list.block,
              borderRadius: 16,
              borderCurve: 'continuous',
              overflow: 'hidden',
            }}
          >
            <View
              style={{
                flexDirection: 'row',
                alignItems: 'center',
                gap: 12,
                padding: 16,
                flexShrink: 0,
              }}
            >
              <ProfileAvatar
                person={counterpart}
                name={recipient}
                color={counterpart?.tint_hex}
                size={44}
              />
              <Text
                accessibilityRole="header"
                style={{ color: p.list.text, fontSize: 17, fontWeight: '600', flex: 1 }}
              >
                {recipient}
              </Text>
              {onOptions && !accepted ? (
                <ForwardOptionsMenu
                  options={options}
                  canHideCaption={forwardHasCaption(entry)}
                  disabled={busy || !canSend}
                  onChange={onOptions}
                />
              ) : null}
            </View>
            <ScrollView
              testID="forward-message-preview-scroll"
              keyboardShouldPersistTaps="handled"
              keyboardDismissMode="interactive"
              contentInsetAdjustmentBehavior="never"
              style={{
                flexGrow: 0,
                flexShrink: 1,
                borderTopWidth: StyleSheet.hairlineWidth,
                borderColor: p.list.separator,
              }}
              contentContainerStyle={{ padding: 16, gap: 12 }}
            >
              {!options.hide_author ? (
                <View style={{ gap: 3 }}>
                  {entry.author.name ? (
                    <Text style={{ color: p.list.accent, fontSize: 15, fontWeight: '600' }}>
                      {entry.forwarded ? messageForwardLabel(entry) : entry.author.name}
                    </Text>
                  ) : null}
                  {sourceTitle ? (
                    <Text style={{ color: p.list.secondary, fontSize: 13 }}>{sourceTitle}</Text>
                  ) : null}
                </View>
              ) : null}
              <ForwardMessagePreview
                key={`${spaceId}:${entry.id}`}
                spaceId={spaceId}
                entry={entry}
                options={options}
                contentWidth={Math.max(1, panelWidth - 32)}
                isCurrent={isCurrent}
              />
            </ScrollView>
            <View
              style={{
                padding: 16,
                gap: 12,
                flexShrink: 0,
                borderTopWidth: StyleSheet.hairlineWidth,
                borderColor: p.list.separator,
              }}
            >
              {accepted ? (
                <Text
                  testID="forward-message-accepted"
                  accessibilityLiveRegion="polite"
                  style={{ color: p.list.secondary, fontSize: 15 }}
                >
                  {t('chat.alreadyForwarded')}
                </Text>
              ) : null}
              <View style={{ flexDirection: fontScale > 1.3 ? 'column' : 'row', gap: 12 }}>
                <View style={fontScale > 1.3 ? undefined : { flex: 1 }}>
                  <NativeButton
                    testID="forward-message-cancel"
                    label={t('common.cancel')}
                    fullWidth
                    secondary
                    disabled={busy}
                    onPress={onCancel}
                  />
                </View>
                <View style={fontScale > 1.3 ? undefined : { flex: 1.6 }}>
                  <NativeButton
                    testID="forward-message-send"
                    label={t(accepted ? 'chat.forwardAgain' : 'chat.send')}
                    fullWidth
                    disabled={busy || (!accepted && !canSend)}
                    onPress={onSend}
                  />
                </View>
              </View>
            </View>
          </View>
        </View>
      </KeyboardAvoidingView>
    </View>
  );
}
