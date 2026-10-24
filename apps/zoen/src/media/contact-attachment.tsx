import * as Clipboard from 'expo-clipboard';
import * as Sharing from 'expo-sharing';
import { ActivityIndicator, ScrollView, Text, View } from 'react-native';
import { MessagePressable } from '../components/message-interaction';
import { Icon, IconButton, NativeButton } from '../components/primitives';
import { documentScope } from '../documents/document-authority';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { MediaSheet } from './media-sheet';
import { useContactPreview } from './use-contact-preview';
import type { MediaDriver } from './types';

export function ContactAttachment(props: {
  media: MediaDriver;
  source: { itemId: string; version?: number; expectedHash: string };
  spaceId?: string;
  readiness: string;
  message: boolean;
  onActions?: () => void;
  isCurrent(): boolean;
}) {
  const { store } = useClient();
  const scope = documentScope(store.getSnapshot(), store.sessionEpoch);
  return <ContactAttachmentView key={JSON.stringify([scope, props.source])} {...props} />;
}
function ContactAttachmentView(props: Parameters<typeof ContactAttachment>[0]) {
  const { colors } = useTheme();
  const { session, state, current } = useContactPreview(props);
  const contact = state.contact;
  const name = contact?.name || t('media.contact');
  const close = () => {
    void session.dismiss(state.ticket).catch(() => {});
  };
  return (
    <>
      <MessagePressable
        accessibilityRole="button"
        accessibilityLabel={t('media.openContactLabel', { name })}
        disabled={!current() || state.visible || state.busy}
        onLongPress={() => {
          if (current()) props.onActions?.();
        }}
        delayLongPress={350}
        onPress={() => {
          if (current()) void session.open().catch(() => {});
        }}
        style={({ pressed }) => ({
          flexDirection: 'row',
          alignItems: 'center',
          gap: 10,
          minHeight: 64,
          padding: props.message ? 12 : 0,
          opacity: pressed ? 0.65 : 1,
        })}
      >
        <View
          style={{
            width: 40,
            height: 40,
            borderRadius: 20,
            backgroundColor: colors.muted,
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <Icon
            name={{ ios: 'person.fill', android: 'person', web: 'person' }}
            color={colors.accent}
            size={24}
          />
        </View>
        <View style={{ flex: 1, gap: 4 }}>
          <Text numberOfLines={2} style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}>
            {name}
          </Text>
          <Text numberOfLines={2} style={{ color: colors.secondary, fontSize: 14 }}>
            {contact?.phones[0]?.value ?? props.readiness}
          </Text>
        </View>
      </MessagePressable>
      <MediaSheet visible={state.visible} locked={false} onDismiss={close}>
        <ScrollView
          keyboardShouldPersistTaps="handled"
          contentContainerStyle={{ padding: 24, gap: 18 }}
        >
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
            <Text
              accessibilityRole="header"
              style={{ flex: 1, color: colors.text, fontSize: 24, fontWeight: '600' }}
            >
              {name}
            </Text>
            <NativeButton label={t('media.closeContact')} secondary onPress={close} />
          </View>
          {state.phase === 'loading' ? (
            <ActivityIndicator accessibilityLabel={t('media.loading')} color={colors.accent} />
          ) : null}
          {state.phase === 'error' ? (
            <Text accessibilityRole="alert" style={{ color: colors.danger }}>
              {t('media.thisFileCouldNotBeOpened')}
            </Text>
          ) : null}
          {contact && state.phase === 'ready' ? (
            <>
              {contact.phones.map((phone, index) => (
                <View
                  key={index}
                  style={{
                    flexDirection: 'row',
                    alignItems: 'center',
                    gap: 12,
                    paddingVertical: 8,
                  }}
                >
                  <View style={{ flex: 1, gap: 4 }}>
                    {phone.label ? (
                      <Text style={{ color: colors.secondary, fontSize: 13 }}>{phone.label}</Text>
                    ) : null}
                    <Text selectable style={{ color: colors.text, fontSize: 18 }}>
                      {phone.value}
                    </Text>
                  </View>
                  <IconButton
                    name={{ ios: 'doc.on.doc', android: 'content_copy', web: 'content_copy' }}
                    label={t('media.copyContactPhone', { phone: phone.value })}
                    disabled={state.busy}
                    onPress={() => {
                      if (current())
                        void session
                          .perform(contact, async (_file, valid) => {
                            if (valid()) await Clipboard.setStringAsync(phone.value);
                          })
                          .catch(() => {});
                    }}
                  />
                </View>
              ))}
              <NativeButton
                label={t('media.shareContact')}
                disabled={state.busy}
                onPress={() => {
                  if (current())
                    void session
                      .perform(contact, async (file, valid) => {
                        const available = await Sharing.isAvailableAsync();
                        if (!valid()) return;
                        if (!available) throw Error('sharing-unavailable');
                        await Sharing.shareAsync(file.uri, {
                          mimeType: file.info.mime,
                          dialogTitle: name,
                        });
                      })
                      .catch(() => {});
                }}
              />
            </>
          ) : null}
        </ScrollView>
      </MediaSheet>
    </>
  );
}
