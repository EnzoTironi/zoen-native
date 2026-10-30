import { Alert } from 'react-native';
import type { ReactNode } from 'react';
import { t } from '../i18n/index.ts';
import type { Dialogs } from './dialog-types';
import { nativeDialogs } from './native-dialogs';

const dialogs: Dialogs = {
  ...nativeDialogs,
  choose: ({ title, choices, signal }) =>
    new Promise((resolve) => {
      if (signal?.aborted) {
        resolve(null);
        return;
      }
      let settled = false;
      const finish = (value: string | null) => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener('abort', cancel);
        resolve(value);
      };
      const cancel = () => finish(null);
      signal?.addEventListener('abort', cancel, { once: true });
      Alert.alert(
        title,
        undefined,
        [
          ...choices.map((choice) => ({
            text: choice.label,
            style: choice.destructive ? ('destructive' as const) : ('default' as const),
            onPress: () => finish(choice.id),
          })),
          { text: t('common.cancel'), style: 'cancel', onPress: cancel },
        ],
        { cancelable: true, onDismiss: cancel },
      );
    }),
};
export function DialogsProvider({ children }: { children: ReactNode }) {
  return children;
}
export function useDialogs(): Dialogs {
  return dialogs;
}
