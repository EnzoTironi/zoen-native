import { BottomSheet, RNHostView } from '@expo/ui';
import { useState } from 'react';
import { View } from 'react-native';
import { useDialogs } from '../components/dialogs';
import { SettingsScreen } from '../components/settings-screen';
import { SettingsRows } from '../components/settings-rows';
import { AuthorizeDevice } from '../components/authorize-device';
import { HistoryTransfer } from '../components/history-transfer';
import { DeviceDetails, deviceName } from '../devices/device-details';
import { DeviceList } from '../devices/device-list';
import { useDevices } from '../devices/use-devices';
import { t } from '../i18n';
import { useTheme } from '../theme';

export default function Devices() {
  const { state, session, accountRequired } = useDevices();
  const { confirm } = useDialogs();
  const { presentation: p } = useTheme();
  const [linkOwner, setLinkOwner] = useState<string | null>(null);
  const ticket = session.ticket();
  const selected = state.devices.find((device) => device.device_id === state.selected) ?? null;
  return (
    <SettingsScreen title={t('settings.devices')} testID="devices-screen">
      {!accountRequired ? (
        <SettingsRows
          testID="device-link-rows"
          rows={[
            {
              id: 'device-link',
              title: t('deviceDirectory.link'),
              icon: {
                ios: 'qrcode.viewfinder',
                android: 'qr_code_scanner',
                web: 'qr_code_scanner',
              },
              disabled: state.phase !== 'ready' || state.pending !== null,
              onPress: () => {
                const current = session.ticket();
                if (
                  current &&
                  current.scope === ticket?.scope &&
                  current.generation === ticket.generation
                )
                  setLinkOwner(current.scope);
              },
            },
          ]}
        />
      ) : null}
      <DeviceList
        state={state}
        session={session}
        ticket={ticket}
        accountRequired={accountRequired}
      />
      {!accountRequired ? <HistoryTransfer key={state.scope} /> : null}
      <DeviceDetails
        device={selected}
        pending={state.pending !== null}
        onClose={() => session.select(null, ticket)}
        onRevoke={() => {
          if (selected)
            void session.revoke(
              selected.device_id,
              () =>
                confirm({
                  title: t('deviceDirectory.revokeTitle', { name: deviceName(selected) }),
                  message: t('deviceDirectory.revokeDetail'),
                  confirmLabel: t('deviceDirectory.revoke'),
                  destructive: true,
                }),
              ticket,
            );
        }}
      />
      <BottomSheet
        testID="authorize-device-sheet"
        isPresented={Boolean(linkOwner && linkOwner === state.scope && state.phase !== 'inactive')}
        onDismiss={() => setLinkOwner(null)}
        snapPoints={['full']}
        containerColor={p.list.background}
      >
        <RNHostView matchContents>
          <View>
            {linkOwner && linkOwner === state.scope ? (
              <AuthorizeDevice key={linkOwner} onLinked={() => void session.load(ticket)} />
            ) : null}
          </View>
        </RNHostView>
      </BottomSheet>
    </SettingsScreen>
  );
}
