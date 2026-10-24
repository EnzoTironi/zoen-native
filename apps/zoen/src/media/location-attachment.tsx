import { useSyncExternalStore } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { MessagePressable } from '../components/message-interaction';
import { Icon, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { documentScope } from '../documents/document-authority';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { LocationMap } from './location-map';
import { LocationPreviewSession } from './location-preview-session';
import { MediaSheet } from './media-sheet';
import { useFilePreview } from './use-file-preview';
import type { MediaDriver } from './types';

type Props = {
  media: MediaDriver;
  source: { itemId: string; version?: number; expectedHash: string };
  spaceId?: string;
  messageId?: string;
  readiness: string;
  message: boolean;
  onActions?: () => void;
  isCurrent(): boolean;
};
export function LocationAttachment(props: Props) {
  const { store } = useClient();
  const scope = documentScope(store.getSnapshot(), store.sessionEpoch);
  return (
    <LocationAttachmentView
      key={JSON.stringify([scope, props.spaceId, props.messageId, props.source])}
      {...props}
    />
  );
}
function LocationAttachmentView(props: Props) {
  const { colors } = useTheme();
  const { session, current } = useFilePreview({
    ...props,
    create: (valid) => new LocationPreviewSession({ ...props, current: valid }),
  });
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const close = () => {
    void session.dismiss(state.ticket).catch(() => {});
  };
  return (
    <>
      <MessagePressable
        accessibilityRole="button"
        accessibilityLabel={t('attachmentTabs.openLocation')}
        disabled={!current() || state.visible}
        onPress={() => {
          if (current()) void session.open().catch(() => {});
        }}
        onLongPress={() => {
          if (current()) props.onActions?.();
        }}
        delayLongPress={350}
        style={({ pressed }) => ({
          flexDirection: 'row',
          alignItems: 'center',
          gap: 10,
          minHeight: 64,
          padding: props.message ? 12 : 0,
          opacity: pressed ? 0.65 : 1,
        })}
      >
        <Icon
          name={{ ios: 'mappin.and.ellipse', android: 'location_on', web: 'location_on' }}
          size={28}
          color={colors.accent}
        />
        <View style={{ flex: 1, gap: 4 }}>
          <Text style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}>
            {t('attachmentTabs.location')}
          </Text>
          <Text style={{ color: colors.secondary, fontSize: 14 }}>{props.readiness}</Text>
        </View>
      </MessagePressable>
      <MediaSheet visible={state.visible} locked={false} onDismiss={close}>
        <View style={{ padding: 24, gap: 18 }}>
          <NativeButton label={t('attachmentTabs.close')} secondary onPress={close} />
          {state.phase === 'loading' ? (
            <ActivityIndicator color={colors.accent} accessibilityLabel={t('media.loading')} />
          ) : null}
          {state.phase === 'error' ? (
            <Text accessibilityRole="alert" style={{ color: colors.danger }}>
              {t('attachmentTabs.locationOpenFailed')}
            </Text>
          ) : null}
          {state.point && state.phase === 'ready' && current() ? (
            <>
              <LocationMap
                binding={`preview-${state.ticket}`}
                editable={false}
                point={state.point}
                label={t('attachmentTabs.locationPreview')}
              />
              {state.point.label ? (
                <Text style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}>
                  {state.point.label}
                </Text>
              ) : null}
              <Text selectable style={{ color: colors.text }}>
                {state.point.latitude}, {state.point.longitude}
              </Text>
            </>
          ) : null}
        </View>
      </MediaSheet>
    </>
  );
}
