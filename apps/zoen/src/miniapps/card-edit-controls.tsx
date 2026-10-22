import { View } from 'react-native';
import { IconButton } from '../components/primitives';
import { t } from '../i18n';

/** The preference controller owns the draft; this header only presents its actions. */
export function AppCardEditControls({
  pending = false,
  onDone,
  onDiscard,
}: {
  pending?: boolean;
  onDone: () => void;
  onDiscard?: () => void;
}) {
  return (
    <View testID="app-card-edit-controls" style={{ flexDirection: 'row', alignItems: 'center' }}>
      {onDiscard ? (
        <IconButton
          testID="app-card-edit-discard"
          name={{ ios: 'xmark', android: 'close' }}
          label={t('common.discard')}
          disabled={pending}
          onPress={onDiscard}
        />
      ) : null}
      <IconButton
        testID="app-card-edit-done"
        name={{ ios: 'checkmark', android: 'check' }}
        label={pending ? t('home.saving') : t('home.doneRearranging')}
        disabled={pending}
        onPress={onDone}
      />
    </View>
  );
}
