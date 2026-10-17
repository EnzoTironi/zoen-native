import { Alert } from 'react-native';
import type { ReactNode } from 'react';
import type { Dialogs } from './dialog-types';

const dialogs: Dialogs = {
  confirm: (options) =>
    new Promise((resolve) => {
      Alert.alert(
        options.title,
        options.message,
        [
          {
            text: options.cancelLabel ?? 'Cancelar',
            style: 'cancel',
            onPress: () => resolve(false),
          },
          {
            text: options.confirmLabel ?? 'Continuar',
            style: options.destructive ? 'destructive' : 'default',
            onPress: () => resolve(true),
          },
        ],
        { cancelable: true, onDismiss: () => resolve(false) },
      );
    }),
  inform: (title, message) =>
    new Promise((resolve) => {
      Alert.alert(title, message, [{ text: 'OK', onPress: resolve }], {
        cancelable: true,
        onDismiss: resolve,
      });
    }),
};
export function DialogsProvider({ children }: { children: ReactNode }) {
  return children;
}
export function useDialogs(): Dialogs {
  return dialogs;
}
