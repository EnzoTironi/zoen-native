import { useLayoutEffect } from 'react';
import { Modal, Platform, ScrollView, Text, View } from 'react-native';
import { t } from '../i18n';
import { NativeButton, Surface } from '../components/primitives';
import { useTheme } from '../theme';
import type { DeviceConsentProps } from './device-consent.types';

/** Web's modal acknowledges its actual exit. Android uses no native presentation. */
export function DeviceConsent(props: DeviceConsentProps) {
  const { colors } = useTheme();
  const request = props.request;
  const { phase, onDismiss } = props;
  useLayoutEffect(() => {
    // Android's RN Modal has no dismissal-completion callback. Its device
    // capabilities are deferred; remove this inline overlay before acknowledgement.
    if (Platform.OS === 'android' && request && phase === 'dismissing') onDismiss(request.id);
  }, [request, phase, onDismiss]);
  if (!request) return null;
  const content = (
    <View
      accessibilityViewIsModal
      style={{
        flex: 1,
        backgroundColor: '#0007',
        justifyContent: 'center',
        padding: 24,
      }}
    >
      <ScrollView
        contentContainerStyle={{ flexGrow: 1, justifyContent: 'center', alignItems: 'center' }}
      >
        <Surface style={{ width: '100%', maxWidth: 460, backgroundColor: colors.background }}>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 23, fontWeight: '600' }}
          >
            {props.title}
          </Text>
          <Text selectable style={{ color: colors.text, fontSize: 17, lineHeight: 24 }}>
            {request.purpose}
          </Text>
          <Text style={{ color: colors.secondary, fontSize: 15, lineHeight: 22 }}>
            {props.scope}
          </Text>
          <NativeButton
            label={t('miniapps.allowOnce')}
            onPress={() => props.onChoose(request.id, 'once')}
          />
          <NativeButton
            secondary
            label={t('miniapps.allowUntilIRevoke')}
            onPress={() => props.onChoose(request.id, 'always')}
          />
          <NativeButton
            secondary
            label={t('miniapps.doNotAllow')}
            onPress={() => props.onChoose(request.id, 'deny')}
          />
        </Surface>
      </ScrollView>
    </View>
  );
  if (Platform.OS === 'android')
    return props.phase === 'presented' ? (
      <View style={{ position: 'absolute', inset: 0, zIndex: 100 }}>{content}</View>
    ) : null;
  return (
    <Modal
      key={request.id}
      visible={props.phase === 'presented'}
      transparent
      animationType="none"
      onRequestClose={() => props.onChoose(request.id, 'deny')}
      onDismiss={() => props.onDismiss(request.id)}
    >
      {content}
    </Modal>
  );
}
